//! Applying WASM relocations against the link plan.
//!
//! A relocatable object stores object-local indices in padded LEB128 (or fixed
//! `i32`) placeholders inside its CODE and DATA payloads. The values they must
//! become are decided by [`WasmLinkInfo`] — the final function and global index
//! spaces and the linear-memory offsets of static data — so this stage only
//! looks up the plan and rewrites the placeholder in place. The writer then
//! copies the patched payloads verbatim.
//!
//! Every relocation is either applied exactly or reported: a fixup that cannot
//! be resolved is a hard error, never a dropped one.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::{Relocation, RelocationKind};
use crate::section::SectionKind;
use crate::symbol::Symbol;
use crate::wasm::link_info::{STACK_POINTER_SYMBOL, TABLE_SYMBOL, WasmLinkInfo};
use crate::wasm::object_info::{WasmImportDesc, WasmObjectInfo};
use std::path::Path;

/// Apply every relocation in every input object, rewriting the placeholders to
/// the plan's final indices and linear-memory offsets.
pub fn apply(objects: &mut [ObjectFile], info: &WasmLinkInfo) -> LinkResult<()> {
    for (object_index, object) in objects.iter_mut().enumerate() {
        if object.wasm.is_none() {
            continue;
        }
        for section_index in 0..object.sections.len() {
            if object.sections[section_index].relocations.is_empty() {
                continue;
            }
            let kind = object.sections[section_index].kind;
            if !matches!(kind, SectionKind::Text | SectionKind::Data) {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "WASM object `{}` has relocations against `{}`, which is not a CODE or \
                         DATA payload",
                        object.path.display(),
                        object.sections[section_index].name
                    ),
                )
                .with_suggestion(
                    "Only the CODE and DATA sections carry linkable relocations in a WASM object.",
                ));
            }

            let relocations = std::mem::take(&mut object.sections[section_index].relocations);
            for relocation in &relocations {
                let value = {
                    let symbol = relocation
                        .symbol_index
                        .and_then(|index| object.symbols.get(index))
                        .ok_or_else(|| {
                            LinkError::new(
                                ErrorCode::InvalidObject,
                                format!(
                                    "WASM object `{}` has a relocation against symbol {}, but the \
                                     symbol table has {} entries",
                                    object.path.display(),
                                    relocation
                                        .symbol_index
                                        .map_or("<none>".to_string(), |index| index.to_string()),
                                    object.symbols.len()
                                ),
                            )
                        })?;
                    resolve_value(
                        object_index,
                        object.path.as_path(),
                        object.wasm.as_ref(),
                        info,
                        relocation,
                        symbol,
                    )?
                };
                patch(&mut object.sections[section_index].data, relocation, value)?;
            }
        }
    }
    Ok(())
}

fn resolve_value(
    object_index: usize,
    path: &Path,
    facts: Option<&WasmObjectInfo>,
    info: &WasmLinkInfo,
    relocation: &Relocation,
    symbol: &Symbol,
) -> LinkResult<u64> {
    match relocation.kind {
        RelocationKind::WasmFunctionIndex => {
            resolve_function_index(object_index, path, facts, info, relocation, symbol)
        }
        RelocationKind::WasmGlobalIndex | RelocationKind::WasmGlobalIndexI32 => {
            if symbol.name != STACK_POINTER_SYMBOL {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "WASM relocation against global `{}` in `{}`: only the linker-defined \
                         `{}` global exists in the output",
                        symbol.name,
                        path.display(),
                        STACK_POINTER_SYMBOL
                    ),
                )
                .with_suggestion(
                    "defined and other imported globals must be produced by the linker that \
                     defines them; this linker emits only the stack pointer",
                ));
            }
            Ok(info.stack_pointer_global_index() as u64)
        }
        RelocationKind::WasmMemoryOffset32
        | RelocationKind::WasmMemoryOffsetSleb
        | RelocationKind::WasmMemoryOffsetI32 => {
            let base = if symbol.is_defined {
                info.data_address(object_index, symbol.value)
                    .ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::UnsupportedRelocation,
                            format!(
                                "WASM data symbol `{}` at offset 0x{:x} of `{}`'s data payload is \
                                 not inside any placed segment",
                                symbol.name,
                                symbol.value,
                                path.display()
                            ),
                        )
                    })? as i64
            } else if let Some(address) = info.local_data_address(&symbol.name) {
                // Another input holds the storage; this object only names it.
                address as i64
            } else if symbol.is_imported {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "WASM relocation in `{}` takes the address of imported data `{}`",
                        path.display(),
                        symbol.name
                    ),
                )
                .with_suggestion("imported data objects have no storage in the linked module"));
            } else {
                // An undefined symbol sits at offset 0, which would otherwise read
                // as the start of this object's own data.
                return Err(LinkError::new(
                    ErrorCode::UndefinedSymbol,
                    format!(
                        "WASM object `{}` takes the address of `{}`, which no input defines",
                        path.display(),
                        symbol.name
                    ),
                ));
            };
            let address = base.checked_add(relocation.addend).ok_or_else(|| {
                LinkError::new(
                    ErrorCode::MemoryExhausted,
                    format!(
                        "WASM data address for `{}` overflows: 0x{:x} + {}",
                        symbol.name, base, relocation.addend
                    ),
                )
            })?;
            u64::try_from(address).ok().filter(|value| *value <= u32::MAX as u64).ok_or_else(|| {
                LinkError::new(
                    ErrorCode::MemoryExhausted,
                    format!(
                        "WASM data address for `{}` is {} (0x{:x} + {}), which does not fit in the \
                         linear memory's 32-bit address space",
                        symbol.name, address, base, relocation.addend
                    ),
                )
            })
        }
        RelocationKind::WasmTypeIndex => {
            // `call_indirect`'s type immediate must be the signature of the
            // function the relocation names; the plan interned that signature
            // when it emitted the body or the import.
            let function =
                resolve_function_index(object_index, path, facts, info, relocation, symbol)?;
            info.function_type_index(function as u32)
                .map(|index| index as u64)
                .ok_or_else(|| {
                    LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "WASM type-index relocation in `{}` names function `{}`, which the \
                             link plan does not define or import",
                            path.display(),
                            symbol.name
                        ),
                    )
                })
        }
        RelocationKind::WasmTableIndexSleb | RelocationKind::WasmTableIndexI32 => {
            // A table-index value is a slot in the output element segment, not
            // a function index: the two numbering spaces are independent.
            let slot = if symbol.is_defined {
                info.table_slot(object_index, symbol.value)
            } else {
                info.table_slot_for_name(&symbol.name)
            };
            slot.map(|slot| slot as u64).ok_or_else(|| {
                LinkError::new(
                    ErrorCode::UndefinedSymbol,
                    format!(
                        "WASM object `{}` takes the address of `{}`, but no input places it in \
                         the function table",
                        path.display(),
                        symbol.name
                    ),
                )
                .with_suggestion(
                    "a function whose address is taken must appear in an element segment; a \
                     function that is only called directly has no table slot",
                )
            })
        }
        RelocationKind::WasmTableNumber => {
            if symbol.name != TABLE_SYMBOL {
                return Err(LinkError::new(
                    ErrorCode::UnsupportedRelocation,
                    format!(
                        "WASM object `{}` names table `{}`, but the linker emits a single table, \
                         `{}`",
                        path.display(),
                        symbol.name,
                        TABLE_SYMBOL
                    ),
                ));
            }
            // The module defines one table, and it is index 0.
            Ok(0)
        }
        _ => Err(LinkError::new(
            ErrorCode::UnsupportedRelocation,
            format!(
                "WASM object `{}` has a non-WASM relocation `{}` against `{}`",
                path.display(),
                relocation.kind.name(),
                relocation.symbol_name
            ),
        )),
    }
}

fn resolve_function_index(
    object_index: usize,
    path: &Path,
    facts: Option<&WasmObjectInfo>,
    info: &WasmLinkInfo,
    relocation: &Relocation,
    symbol: &Symbol,
) -> LinkResult<u64> {
    // A reference to a name another input defines is a call into the module. The
    // object writes it exactly as it writes a host import, and the plan dropped
    // that import, so the name is the only handle left to bind on.
    if !symbol.is_defined {
        if let Some(index) = info.local_function_index(&symbol.name) {
            return Ok(index as u64);
        }
    }
    if symbol.is_imported {
        // `value` holds the symbol's function index in its object, whose first
        // entries are imports; every import carries its host module and field
        // name, which is what the plan deduplicated on.
        let import = facts
            .and_then(|facts| {
                facts
                    .imports
                    .iter()
                    .filter(|import| matches!(import, WasmImportDesc::Function { .. }))
                    .nth(symbol.value as usize)
            })
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM object `{}` references imported function `{}` as import {}, but \
                         declares fewer function imports",
                        path.display(),
                        symbol.name,
                        symbol.value
                    ),
                )
            })?;
        let WasmImportDesc::Function { module, name, .. } = import else {
            unreachable!("filtered to function imports above");
        };
        return info
            .imported_function_index(module, name)
            .map(|index| index as u64)
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM import `{}::{}` referenced in `{}` is missing from the link plan",
                        module,
                        name,
                        path.display()
                    ),
                )
            });
    }
    if symbol.is_defined && symbol.sym_type == crate::symbol::SymbolType::Function {
        return info
            .defined_function_index(object_index, symbol.value)
            .map(|index| index as u64)
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM function `{}` at body offset 0x{:x} of `{}` is missing from the \
                         link plan",
                        symbol.name,
                        symbol.value,
                        path.display()
                    ),
                )
            });
    }
    Err(LinkError::new(
        ErrorCode::UnsupportedRelocation,
        format!(
            "WASM relocation in `{}` calls `{}`, which is neither an import nor a defined \
             function",
            path.display(),
            relocation.symbol_name
        ),
    ))
}

fn patch(data: &mut [u8], relocation: &Relocation, value: u64) -> LinkResult<()> {
    let offset = relocation.offset as usize;
    match relocation.kind {
        RelocationKind::WasmGlobalIndexI32
        | RelocationKind::WasmMemoryOffsetI32
        | RelocationKind::WasmTableIndexI32 => {
            let len = data.len();
            let field = data
                .get_mut(offset..offset + 4)
                .ok_or_else(|| truncated_field(offset, 4, len))?;
            let value = u32::try_from(value).map_err(|_| field_overflow(value, 4))?;
            field.copy_from_slice(&value.to_le_bytes());
            Ok(())
        }
        RelocationKind::WasmMemoryOffsetSleb | RelocationKind::WasmTableIndexSleb => {
            let width = leb_width(data, offset)?;
            let encoded =
                encode_sleb(value as i64, width).ok_or_else(|| field_overflow(value, width))?;
            data[offset..offset + width].copy_from_slice(&encoded);
            Ok(())
        }
        _ => {
            let width = leb_width(data, offset)?;
            let encoded = encode_uleb(value, width).ok_or_else(|| field_overflow(value, width))?;
            data[offset..offset + width].copy_from_slice(&encoded);
            Ok(())
        }
    }
}

/// Width of the LEB128 placeholder at `offset`, read from its continuation bits.
fn leb_width(data: &[u8], offset: usize) -> LinkResult<usize> {
    for width in 1..=5 {
        let byte = *data.get(offset + width - 1).ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "WASM relocation field at 0x{:x} runs past the end of its section (0x{:x} bytes)",
                    offset,
                    data.len()
                ),
            )
        })?;
        if byte & 0x80 == 0 {
            return Ok(width);
        }
    }
    Err(LinkError::new(
        ErrorCode::InvalidObject,
        format!(
            "WASM relocation field at 0x{:x} is not a LEB128 value",
            offset
        ),
    ))
}

/// Encode `value` in exactly `width` LEB128 bytes, padding with redundant zero
/// groups the way LLVM does for relocatable fields. `None` when it does not
/// fit: a truncated field would silently change the value.
fn encode_uleb(mut value: u64, width: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(width);
    for index in 0..width {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if index + 1 < width {
            byte |= 0x80;
        } else if value != 0 {
            return None;
        }
        out.push(byte);
    }
    Some(out)
}

fn encode_sleb(mut value: i64, width: usize) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(width);
    for index in 0..width {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        if index + 1 < width {
            out.push(byte | 0x80);
        } else {
            let sign_bit = byte & 0x40 != 0;
            let fits = (value == 0 && !sign_bit) || (value == -1 && sign_bit);
            if !fits {
                return None;
            }
            out.push(byte);
        }
    }
    Some(out)
}

fn truncated_field(offset: usize, width: usize, len: usize) -> LinkError {
    LinkError::new(
        ErrorCode::InvalidObject,
        format!(
            "WASM relocation field at 0x{:x} needs {} bytes, but its section has 0x{:x}",
            offset, width, len
        ),
    )
}

fn field_overflow(value: u64, width: usize) -> LinkError {
    LinkError::new(
        ErrorCode::MemoryExhausted,
        format!(
            "WASM relocation value {} does not fit in the {}-byte field reserved for it",
            value, width
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relocation(kind: RelocationKind, offset: u64) -> Relocation {
        Relocation {
            offset,
            symbol_name: "sym".to_string(),
            symbol_index: Some(0),
            file_index: None,
            kind,
            addend: 0,
        }
    }

    #[test]
    fn unsigned_leb_fields_keep_their_reserved_width() {
        // LLVM pads relocatable fields to five bytes; the patched value must
        // occupy exactly that many, or every following byte shifts.
        let mut data = [0x10, 0x80, 0x80, 0x80, 0x80, 0x00, 0x0B];
        patch(
            &mut data,
            &relocation(RelocationKind::WasmFunctionIndex, 1),
            300,
        )
        .expect("patch");
        assert_eq!(data, [0x10, 0xAC, 0x82, 0x80, 0x80, 0x00, 0x0B]);
    }

    #[test]
    fn narrow_leb_fields_reject_values_that_do_not_fit() {
        let mut data = [0x41, 0x00, 0x0B];
        let error = patch(
            &mut data,
            &relocation(RelocationKind::WasmGlobalIndex, 1),
            1_048_576,
        )
        .expect_err("one byte cannot hold 2^20");
        assert_eq!(error.code, ErrorCode::MemoryExhausted);
    }

    #[test]
    fn signed_leb_fields_carry_a_sign_bit() {
        let mut data = [0x80, 0x80, 0x00];
        patch(
            &mut data,
            &relocation(RelocationKind::WasmMemoryOffsetSleb, 0),
            1024,
        )
        .expect("patch");
        // 1024 needs the sign bit clear in the last group, so the encoder adds
        // a redundant group: 0x80 0x88 0x00.
        assert_eq!(data, [0x80, 0x88, 0x00]);
    }

    #[test]
    fn i32_fields_are_fixed_width_little_endian() {
        let mut data = [0, 0, 0, 0, 0x0B];
        patch(
            &mut data,
            &relocation(RelocationKind::WasmMemoryOffsetI32, 0),
            0xDEAD_BEEF,
        )
        .expect("patch");
        assert_eq!(data, [0xEF, 0xBE, 0xAD, 0xDE, 0x0B]);
    }

    #[test]
    fn a_field_running_past_the_section_is_a_loud_error() {
        let mut data = [0x80, 0x80];
        let error = patch(
            &mut data,
            &relocation(RelocationKind::WasmFunctionIndex, 0),
            1,
        )
        .expect_err("the continuation bits never terminate");
        assert_eq!(error.code, ErrorCode::InvalidObject);
    }
}
