//! WebAssembly section identifiers, LEB128 codecs, and the object reader.
//!
//! A relocatable wasm object is a normal module plus a `linking` custom
//! section (symbol table) and `reloc.<section>` custom sections. Modules and
//! objects share version 1 — LLVM 19 writes `01 00 00 00` for both — so the
//! reader detects an object by the presence of the `linking` section, not by
//! the version field.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::relocation::Relocation;
use crate::section::{Section, SectionKind, flags};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
use crate::wasm::object_info::{
    WasmDataSegmentInfo, WasmExportKind, WasmFuncType, WasmImportDesc, WasmObjectExport,
    WasmObjectInfo,
};
use crate::wasm::reloc;
use std::path::Path;

pub const WASM_MAGIC: [u8; 4] = [0x00, 0x61, 0x73, 0x6D];
pub const WASM_VERSION: [u8; 4] = [0x01, 0x00, 0x00, 0x00];

pub const WASM_SEC_CUSTOM: u8 = 0;
pub const WASM_SEC_TYPE: u8 = 1;
pub const WASM_SEC_IMPORT: u8 = 2;
pub const WASM_SEC_FUNCTION: u8 = 3;
pub const WASM_SEC_TABLE: u8 = 4;
pub const WASM_SEC_MEMORY: u8 = 5;
pub const WASM_SEC_GLOBAL: u8 = 6;
pub const WASM_SEC_EXPORT: u8 = 7;
pub const WASM_SEC_START: u8 = 8;
pub const WASM_SEC_ELEMENT: u8 = 9;
pub const WASM_SEC_CODE: u8 = 10;
pub const WASM_SEC_DATA: u8 = 11;
pub const WASM_SEC_DATA_COUNT: u8 = 12;

/// `linking` subsection holding the symbol table.
const LINK_SUBSEC_SYMBOL_TABLE: u8 = 8;

// `linking` symbol kinds.
const SYMTAB_FUNCTION: u8 = 0;
const SYMTAB_DATA: u8 = 1;
const SYMTAB_GLOBAL: u8 = 2;
const SYMTAB_SECTION: u8 = 3;
const SYMTAB_EVENT: u8 = 4;
const SYMTAB_TABLE: u8 = 5;

// `linking` symbol flags. The remaining flags (NO_STRIP 0x80, TLS 0x100,
// ABSOLUTE 0x200) are not modelled: TLS and absolute symbols are rejected
// loudly rather than silently mis-linked.
const SYM_BINDING_MASK: u32 = 0x3;
const SYM_BINDING_WEAK: u32 = 0x1;
const SYM_BINDING_LOCAL: u32 = 0x2;
const SYM_BINDING_COMMON: u32 = 0x3;
const SYM_VISIBILITY_HIDDEN: u32 = 0x4;
const SYM_UNDEFINED: u32 = 0x10;
const SYM_EXPORTED: u32 = 0x20;
const SYM_EXPLICIT_NAME: u32 = 0x40;
const SYM_TLS: u32 = 0x100;
const SYM_ABSOLUTE: u32 = 0x200;

/// Encode unsigned 32-bit integer as LEB128.
pub fn encode_u32_leb128(mut value: u32, buf: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if value == 0 {
            break;
        }
    }
}

/// Decode unsigned 32-bit integer from LEB128.
pub fn decode_u32_leb128(bytes: &[u8], offset: &mut usize) -> LinkResult<u32> {
    let mut result = 0u32;
    let mut shift = 0;

    loop {
        if *offset >= bytes.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "unexpected end of LEB128 stream",
            ));
        }
        let byte = bytes[*offset];
        *offset += 1;

        result |= ((byte & 0x7F) as u32) << shift;
        if (byte & 0x80) == 0 {
            break;
        }
        shift += 7;
        if shift >= 35 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "LEB128 integer overflow",
            ));
        }
    }

    Ok(result)
}

/// Decode a signed LEB128 value as `i64` (used for relocation addends).
pub fn decode_i64_leb128(bytes: &[u8], offset: &mut usize) -> LinkResult<i64> {
    let mut result = 0i64;
    let mut shift = 0u32;

    loop {
        if *offset >= bytes.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "unexpected end of signed LEB128 stream",
            ));
        }
        let byte = bytes[*offset];
        *offset += 1;

        result |= ((byte & 0x7F) as i64) << shift;
        shift += 7;
        if (byte & 0x80) == 0 {
            if shift < 64 && (byte & 0x40) != 0 {
                result |= !0i64 << shift;
            }
            return Ok(result);
        }
        if shift >= 64 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "signed LEB128 integer overflow",
            ));
        }
    }
}

fn read_u8(bytes: &[u8], offset: &mut usize) -> LinkResult<u8> {
    let byte = bytes.get(*offset).copied().ok_or_else(|| {
        LinkError::new(ErrorCode::InvalidObject, "unexpected end of WASM payload")
    })?;
    *offset += 1;
    Ok(byte)
}

fn read_name(bytes: &[u8], offset: &mut usize, what: &str) -> LinkResult<String> {
    let len = decode_u32_leb128(bytes, offset)? as usize;
    let end = offset
        .checked_add(len)
        .filter(|end| *end <= bytes.len())
        .ok_or_else(|| LinkError::new(ErrorCode::InvalidObject, format!("truncated {}", what)))?;
    let name = std::str::from_utf8(&bytes[*offset..end])
        .map_err(|_| {
            LinkError::new(
                ErrorCode::InvalidObject,
                format!("{} is not valid UTF-8", what),
            )
        })?
        .to_string();
    *offset = end;
    Ok(name)
}

struct RawSection<'a> {
    id: u8,
    /// Custom-section name; `None` for every other section.
    name: Option<String>,
    /// Section content, with a custom section's name already stripped.
    data: &'a [u8],
}

impl RawSection<'_> {
    fn is_custom(&self, expected: &str) -> bool {
        self.id == WASM_SEC_CUSTOM && self.name.as_deref() == Some(expected)
    }
}

fn parse_sections(bytes: &[u8]) -> LinkResult<Vec<RawSection<'_>>> {
    let mut sections = Vec::new();
    let mut offset = 8;

    while offset < bytes.len() {
        let id = bytes[offset];
        offset += 1;
        let len = decode_u32_leb128(bytes, &mut offset)? as usize;
        let end = offset
            .checked_add(len)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("truncated WASM section (id {} declares {} bytes)", id, len),
                )
            })?;

        let mut data = &bytes[offset..end];
        let name = if id == WASM_SEC_CUSTOM {
            let mut name_offset = 0;
            let name = read_name(data, &mut name_offset, "custom section name")?;
            data = &data[name_offset..];
            Some(name)
        } else {
            None
        };

        sections.push(RawSection { id, name, data });
        offset = end;
    }

    Ok(sections)
}

/// Names of the module's imports, in each import index space's order. An
/// undefined `linking` symbol carries only an index, so its name is recovered
/// from here.
#[derive(Default)]
struct ImportNameSpaces {
    funcs: Vec<String>,
    tables: Vec<String>,
    globals: Vec<String>,
    tags: Vec<String>,
}

impl ImportNameSpaces {
    fn from_imports(imports: &[WasmImportDesc]) -> Self {
        let mut spaces = ImportNameSpaces::default();
        for import in imports {
            match import {
                WasmImportDesc::Function { name, .. } => spaces.funcs.push(name.clone()),
                WasmImportDesc::Table { name, .. } => spaces.tables.push(name.clone()),
                WasmImportDesc::Memory { .. } => {}
                WasmImportDesc::Global { name, .. } => spaces.globals.push(name.clone()),
                WasmImportDesc::Tag { name, .. } => spaces.tags.push(name.clone()),
            }
        }
        spaces
    }
}

/// Decode a limits prefix, returning the minimum.
fn parse_limits(bytes: &[u8], offset: &mut usize) -> LinkResult<u32> {
    let limit_flags = decode_u32_leb128(bytes, offset)?;
    let min = decode_u32_leb128(bytes, offset)?;
    if limit_flags & 0x1 != 0 {
        let _max = decode_u32_leb128(bytes, offset)?;
    }
    Ok(min)
}

fn parse_imports(data: &[u8]) -> LinkResult<Vec<WasmImportDesc>> {
    let mut imports = Vec::new();
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;

    for _ in 0..count {
        let module = read_name(data, &mut offset, "import module name")?;
        let name = read_name(data, &mut offset, "import field name")?;
        let kind = read_u8(data, &mut offset)?;
        let import = match kind {
            0 => {
                let type_index = decode_u32_leb128(data, &mut offset)?;
                WasmImportDesc::Function {
                    module,
                    name,
                    type_index,
                }
            }
            1 => {
                let _element_type = read_u8(data, &mut offset)?;
                let min = parse_limits(data, &mut offset)?;
                WasmImportDesc::Table { module, name, min }
            }
            2 => {
                let min = parse_limits(data, &mut offset)?;
                WasmImportDesc::Memory { module, name, min }
            }
            3 => {
                let val_type = read_u8(data, &mut offset)?;
                let mutable = read_u8(data, &mut offset)? != 0;
                WasmImportDesc::Global {
                    module,
                    name,
                    val_type,
                    mutable,
                }
            }
            4 => {
                let type_index = decode_u32_leb128(data, &mut offset)?;
                WasmImportDesc::Tag {
                    module,
                    name,
                    type_index,
                }
            }
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("unknown WASM import kind {}", other),
                ));
            }
        };
        imports.push(import);
    }

    Ok(imports)
}

/// Decode the type section into function signatures.
fn parse_types(data: &[u8]) -> LinkResult<Vec<WasmFuncType>> {
    let mut types = Vec::new();
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;

    for index in 0..count {
        let form = read_u8(data, &mut offset)?;
        if form != 0x60 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "unsupported WASM type form 0x{:02x} at type {}",
                    form, index
                ),
            )
            .with_suggestion("Only function types are supported; GC/struct/array types are not."));
        }
        let param_count = decode_u32_leb128(data, &mut offset)? as usize;
        let params = read_val_types(data, &mut offset, param_count)?;
        let result_count = decode_u32_leb128(data, &mut offset)? as usize;
        let results = read_val_types(data, &mut offset, result_count)?;
        types.push(WasmFuncType { params, results });
    }

    Ok(types)
}

fn read_val_types(data: &[u8], offset: &mut usize, count: usize) -> LinkResult<Vec<u8>> {
    let end = offset
        .checked_add(count)
        .filter(|end| *end <= data.len())
        .ok_or_else(|| LinkError::new(ErrorCode::InvalidObject, "truncated WASM value types"))?;
    let types = data[*offset..end].to_vec();
    *offset = end;
    Ok(types)
}

/// Decode the function section: the type index of each defined function, in
/// code order.
fn parse_function_types(data: &[u8]) -> LinkResult<Vec<u32>> {
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;
    let mut types = Vec::with_capacity(count as usize);
    for _ in 0..count {
        types.push(decode_u32_leb128(data, &mut offset)?);
    }
    Ok(types)
}

struct WasmSymbol {
    kind: u8,
    flags: u32,
    name: String,
    /// Function/table/global/tag index, data segment index, or section index,
    /// depending on `kind`.
    index: u32,
    data_offset: u64,
    data_size: u64,
}

impl WasmSymbol {
    fn is_undefined(&self) -> bool {
        self.flags & SYM_UNDEFINED != 0
    }
}

fn import_name(kind: u8, index: u32, imports: &ImportNameSpaces) -> LinkResult<String> {
    let (space, list) = match kind {
        SYMTAB_FUNCTION => ("function", &imports.funcs),
        SYMTAB_TABLE => ("table", &imports.tables),
        SYMTAB_GLOBAL => ("global", &imports.globals),
        SYMTAB_EVENT => ("tag", &imports.tags),
        other => {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "undefined WASM symbol of kind {} has no import namespace",
                    other
                ),
            ));
        }
    };
    list.get(index as usize).cloned().ok_or_else(|| {
        LinkError::new(
            ErrorCode::InvalidObject,
            format!(
                "undefined WASM symbol points at {} import {}, but the module imports {}",
                space,
                index,
                list.len()
            ),
        )
    })
}

fn parse_symbol_table(data: &[u8], imports: &ImportNameSpaces) -> LinkResult<Vec<WasmSymbol>> {
    let mut symbols = Vec::new();
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;

    for _ in 0..count {
        let kind = read_u8(data, &mut offset)?;
        let flags = decode_u32_leb128(data, &mut offset)?;

        if flags & SYM_ABSOLUTE != 0 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "WASM `linking` symbol uses WASM_SYM_ABSOLUTE, which this reader does not implement",
            ));
        }
        if flags & SYM_TLS != 0 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "WASM `linking` symbol uses WASM_SYM_TLS, which this reader does not implement",
            )
            .with_suggestion("Thread-local storage is not supported by this linker."));
        }

        let undefined = flags & SYM_UNDEFINED != 0;
        let common = flags & SYM_BINDING_MASK == SYM_BINDING_COMMON;
        let (name, index, data_offset, data_size) = match (kind, undefined) {
            // Data symbols always carry their name first. A defined,
            // non-common symbol then names its segment and offset/size; a
            // common symbol carries size and alignment because the linker
            // allocates the storage; an undefined one carries nothing else.
            (SYMTAB_DATA, _) => {
                let name = read_name(data, &mut offset, "data symbol name")?;
                if common {
                    if undefined {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!("WASM data symbol `{}` is both undefined and COMMON", name),
                        ));
                    }
                    return Err(LinkError::new(
                        ErrorCode::UnsupportedFormat,
                        format!(
                            "WASM common data symbol `{}` is not implemented: the linker \
                             cannot allocate BSS storage for it yet",
                            name
                        ),
                    )
                    .with_suggestion(
                        "Compile with `-fno-common` so tentative definitions become \
                         zero-initialized data segments.",
                    ));
                }
                if undefined {
                    (name, 0, 0, 0)
                } else {
                    let segment = decode_u32_leb128(data, &mut offset)?;
                    let offset_in_segment = decode_u32_leb128(data, &mut offset)?;
                    let size = decode_u32_leb128(data, &mut offset)?;
                    (name, segment, offset_in_segment as u64, size as u64)
                }
            }
            // Section symbols name a section by index; the name is filled in
            // from the section table once it is known.
            (SYMTAB_SECTION, false) => {
                let index = decode_u32_leb128(data, &mut offset)?;
                (String::new(), index, 0, 0)
            }
            (SYMTAB_SECTION, true) => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    "WASM `linking` section symbol is marked undefined",
                ));
            }
            // Every other symbol names a function/global/table/tag index. An
            // import takes its name from the import section unless
            // WASM_SYM_EXPLICIT_NAME supplies one, which is how clang remaps
            // e.g. `import_name("host_fn")` onto the local symbol `local_alias`.
            (kind, _) => {
                let index = decode_u32_leb128(data, &mut offset)?;
                let name = if !undefined || flags & SYM_EXPLICIT_NAME != 0 {
                    read_name(data, &mut offset, "symbol name")?
                } else {
                    import_name(kind, index, imports)?
                };
                (name, index, 0, 0)
            }
        };

        symbols.push(WasmSymbol {
            kind,
            flags,
            name,
            index,
            data_offset,
            data_size,
        });
    }

    Ok(symbols)
}

fn parse_linking(data: &[u8], imports: &ImportNameSpaces) -> LinkResult<Vec<WasmSymbol>> {
    let mut offset = 0;
    let version = decode_u32_leb128(data, &mut offset)?;
    if version != 2 {
        return Err(LinkError::new(
            ErrorCode::InvalidObject,
            format!(
                "unsupported WASM `linking` section version {} (expected 2)",
                version
            ),
        ));
    }

    let mut symbols = Vec::new();
    while offset < data.len() {
        let subsection = read_u8(data, &mut offset)?;
        let size = decode_u32_leb128(data, &mut offset)? as usize;
        let end = offset
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("truncated WASM `linking` subsection {}", subsection),
                )
            })?;

        if subsection == LINK_SUBSEC_SYMBOL_TABLE {
            symbols = parse_symbol_table(&data[offset..end], imports)?;
        }
        // Segment info, init functions and comdat are length-delimited and
        // carry nothing this linker consumes yet; skipping them is exact.
        offset = end;
    }

    Ok(symbols)
}

/// Half-open byte ranges of each function body within the raw CODE section
/// payload. These share a coordinate space with `reloc.CODE` offsets, which
/// are also relative to the section payload.
fn code_function_ranges(data: &[u8]) -> LinkResult<Vec<(u64, u64)>> {
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;
    let mut ranges = Vec::with_capacity(count as usize);

    for index in 0..count {
        let size = decode_u32_leb128(data, &mut offset)? as usize;
        let start = offset as u64;
        offset = offset
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("truncated WASM code entry {}", index),
                )
            })?;
        ranges.push((start, offset as u64));
    }

    Ok(ranges)
}

/// Skip a constant expression (`i32.const N end` is the only shape data
/// segments use); anything else is rejected rather than mis-skipped.
fn skip_const_expr(data: &[u8], offset: &mut usize) -> LinkResult<()> {
    loop {
        let opcode = read_u8(data, offset)?;
        match opcode {
            0x0B => return Ok(()),
            0x41 | 0x42 => {
                let _imm = decode_i64_leb128(data, offset)?;
            }
            0x23 => {
                let _global = decode_u32_leb128(data, offset)?;
            }
            0x6A | 0x6B | 0x6C | 0x7D | 0x7E | 0x7F => {}
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "unsupported opcode 0x{:02x} in WASM data-segment offset expression",
                        other
                    ),
                ));
            }
        }
    }
}

/// Export section entries. The indices are object-local and the linker
/// renumbers them; only the names survive unchanged.
fn parse_exports(data: &[u8]) -> LinkResult<Vec<WasmObjectExport>> {
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;
    let mut exports = Vec::with_capacity(count as usize);

    for _ in 0..count {
        let name = read_name(data, &mut offset, "export name")?;
        let kind = match read_u8(data, &mut offset)? {
            0 => WasmExportKind::Function,
            1 => WasmExportKind::Table,
            2 => WasmExportKind::Memory,
            3 => WasmExportKind::Global,
            4 => WasmExportKind::Tag,
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("unknown WASM export kind {} for `{}`", other, name),
                ));
            }
        };
        let index = decode_u32_leb128(data, &mut offset)?;
        exports.push(WasmObjectExport { name, kind, index });
    }

    Ok(exports)
}

/// Payload ranges of each data segment within the raw DATA section, plus the
/// segment index each payload belongs to.
///
/// The declared memory offset is parsed but not returned: the linker places
/// data itself, and a `global.get`-based offset (PIC objects) is a relocation
/// target whose value is meaningless before linking.
fn parse_data_segments(data: &[u8]) -> LinkResult<Vec<WasmDataSegmentInfo>> {
    let mut offset = 0;
    let count = decode_u32_leb128(data, &mut offset)?;
    let mut segments = Vec::with_capacity(count as usize);

    for index in 0..count {
        let segment_flags = decode_u32_leb128(data, &mut offset)?;
        match segment_flags {
            0 => skip_const_expr(data, &mut offset)?,
            1 | 3 => {}
            2 => {
                let _memory_index = decode_u32_leb128(data, &mut offset)?;
                skip_const_expr(data, &mut offset)?;
            }
            other => {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("unknown WASM data segment flags {}", other),
                ));
            }
        }
        let size = decode_u32_leb128(data, &mut offset)? as usize;
        let start = offset as u64;
        offset = offset
            .checked_add(size)
            .filter(|end| *end <= data.len())
            .ok_or_else(|| {
                LinkError::new(
                    ErrorCode::InvalidObject,
                    format!("truncated WASM data segment {}", index),
                )
            })?;
        segments.push(WasmDataSegmentInfo {
            payload_offset: start,
            payload_len: size as u64,
        });
    }

    Ok(segments)
}

fn parse_relocations(
    data: &[u8],
    symbols: &[WasmSymbol],
    sections: &mut [Section],
) -> LinkResult<()> {
    let mut offset = 0;
    let target_index = decode_u32_leb128(data, &mut offset)? as usize;
    let count = decode_u32_leb128(data, &mut offset)?;

    if target_index >= sections.len() {
        return Err(LinkError::new(
            ErrorCode::InvalidObject,
            format!(
                "WASM relocation section targets section {}, but the file has {} sections",
                target_index,
                sections.len()
            ),
        ));
    }

    for _ in 0..count {
        let ty = read_u8(data, &mut offset)?;
        let reloc_offset = decode_u32_leb128(data, &mut offset)? as u64;
        let symbol_index = decode_u32_leb128(data, &mut offset)? as usize;

        let kind = reloc::relocation_kind(ty).ok_or_else(|| {
            LinkError::new(
                ErrorCode::UnsupportedRelocation,
                format!("WASM relocation type {} is not implemented", ty),
            )
            .with_suggestion(
                "Only function/table index, type index, global index and 32-bit memory \
                 address relocations are supported today.",
            )
        })?;
        let addend = if reloc::has_addend(ty) {
            decode_i64_leb128(data, &mut offset)?
        } else {
            0
        };

        let symbol = symbols.get(symbol_index).ok_or_else(|| {
            LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "WASM relocation references symbol {}, but the symbol table has {} entries",
                    symbol_index,
                    symbols.len()
                ),
            )
        })?;
        if symbol.name.is_empty() {
            return Err(LinkError::new(
                ErrorCode::UnsupportedRelocation,
                "WASM relocation targets a section symbol, which is not supported",
            ));
        }

        let mut relocation = Relocation::new(reloc_offset, symbol.name.clone(), kind, addend);
        relocation.symbol_index = Some(symbol_index);
        sections[target_index].relocations.push(relocation);
    }

    Ok(())
}

/// Section name, kind and permission flags for a raw wasm section.
///
/// Metadata sections are *not* `Rodata`: they are not part of the produced
/// image, so treating them as allocatable content would place type/import/
/// export bytes into the address space as if they were data.
fn section_identity(section: &RawSection<'_>) -> (String, SectionKind, u32) {
    let (name, kind, sec_flags) = match section.id {
        WASM_SEC_CODE => (
            ".text".to_string(),
            SectionKind::Text,
            flags::READ | flags::EXEC | flags::ALLOC,
        ),
        WASM_SEC_DATA => (
            ".data".to_string(),
            SectionKind::Data,
            flags::READ | flags::WRITE | flags::ALLOC,
        ),
        WASM_SEC_CUSTOM => {
            let name = section.name.as_deref().unwrap_or("<unnamed>");
            if name == "linking" {
                (".linking".to_string(), SectionKind::Custom, 0)
            } else if name.starts_with("reloc.") {
                (format!(".{}", name), SectionKind::Reloc, 0)
            } else {
                (format!(".{}", name), SectionKind::Custom, 0)
            }
        }
        other => (
            format!(".wasm.{}", section_name(other)),
            SectionKind::Custom,
            0,
        ),
    };
    (name, kind, sec_flags)
}

fn section_name(id: u8) -> &'static str {
    match id {
        WASM_SEC_TYPE => "type",
        WASM_SEC_IMPORT => "import",
        WASM_SEC_FUNCTION => "function",
        WASM_SEC_TABLE => "table",
        WASM_SEC_MEMORY => "memory",
        WASM_SEC_GLOBAL => "global",
        WASM_SEC_EXPORT => "export",
        WASM_SEC_START => "start",
        WASM_SEC_ELEMENT => "element",
        WASM_SEC_DATA_COUNT => "datacount",
        _ => "unknown",
    }
}

/// WASM Object Reader.
pub struct WasmReader;

impl WasmReader {
    pub fn read(bytes: &[u8], path: &Path, file_index: usize) -> LinkResult<ObjectFile> {
        if bytes.len() < 8 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "WASM file too small",
            ));
        }

        if bytes[0..4] != WASM_MAGIC {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "invalid WASM magic",
            ));
        }

        if bytes[4..8] != WASM_VERSION {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "unsupported WASM version {:02x?}: this linker accepts version 1 \
                     (module and relocatable object)",
                    &bytes[4..8]
                ),
            )
            .with_suggestion(
                "Re-emit the file with a wasm32 target that writes version 1 objects.",
            ));
        }

        let target = Target {
            arch: Arch::Wasm32,
            os: Os::Wasi,
            format: ObjectFormat::Wasm,
            abi: crate::target::Abi::Wasi,
            pointer_width: PointerWidth::U32,
            endianness: Endianness::Little,
            relocation_model: crate::target::RelocationModel::Static,
            page_size: 65536,
            image_base: 0,
            default_entry: "_start".to_string(),
            libc_flavor: crate::target::LibcFlavor::Gnu,
        };

        let raw = parse_sections(bytes)?;

        let imports = match raw.iter().find(|section| section.id == WASM_SEC_IMPORT) {
            Some(section) => parse_imports(section.data)?,
            None => Vec::new(),
        };
        let import_names = ImportNameSpaces::from_imports(&imports);

        let wasm_symbols = match raw.iter().find(|section| section.is_custom("linking")) {
            Some(section) => parse_linking(section.data, &import_names)?,
            None => Vec::new(),
        };

        let code_index = raw.iter().position(|section| section.id == WASM_SEC_CODE);
        let data_index = raw.iter().position(|section| section.id == WASM_SEC_DATA);
        let global_index = raw.iter().position(|section| section.id == WASM_SEC_GLOBAL);
        let table_index = raw.iter().position(|section| section.id == WASM_SEC_TABLE);

        let function_ranges = match code_index {
            Some(index) => code_function_ranges(raw[index].data)?,
            None => Vec::new(),
        };
        let segment_offsets = match data_index {
            Some(index) => parse_data_segments(raw[index].data)?,
            None => Vec::new(),
        };
        let exports = match raw.iter().find(|section| section.id == WASM_SEC_EXPORT) {
            Some(section) => parse_exports(section.data)?,
            None => Vec::new(),
        };
        let body_offsets: Vec<u64> = function_ranges.iter().map(|(start, _)| *start).collect();
        let body_ends: Vec<u64> = function_ranges.iter().map(|(_, end)| *end).collect();

        let types = match raw.iter().find(|section| section.id == WASM_SEC_TYPE) {
            Some(section) => parse_types(section.data)?,
            None => Vec::new(),
        };
        let function_types = match raw.iter().find(|section| section.id == WASM_SEC_FUNCTION) {
            Some(section) => parse_function_types(section.data)?,
            None => Vec::new(),
        };
        if function_types.len() != function_ranges.len() {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!(
                    "WASM object declares {} function types but has {} code bodies",
                    function_types.len(),
                    function_ranges.len()
                ),
            ));
        }

        let mut sections: Vec<Section> = raw
            .iter()
            .map(|section| {
                let (name, kind, sec_flags) = section_identity(section);
                Section {
                    name,
                    kind,
                    flags: sec_flags,
                    alignment: 1,
                    virtual_address: 0,
                    file_offset: 0,
                    size: section.data.len() as u64,
                    data: section.data.to_vec(),
                    relocations: Vec::new(),
                    comdat_group: None,
                    file_index: Some(file_index),
                    is_live: true,
                    is_folded: false,
                    folded_into: None,
                }
            })
            .collect();

        for section in raw.iter().filter(|section| {
            section
                .name
                .as_deref()
                .is_some_and(|name| name.starts_with("reloc."))
        }) {
            parse_relocations(section.data, &wasm_symbols, &mut sections)?;
        }

        let segment_payloads: Vec<u64> = segment_offsets.iter().map(|s| s.payload_offset).collect();

        let mut symbols = Vec::with_capacity(wasm_symbols.len());
        for wasm_symbol in &wasm_symbols {
            symbols.push(linker_symbol(
                wasm_symbol,
                &sections,
                &body_offsets,
                &segment_payloads,
                import_names.funcs.len(),
                code_index,
                data_index,
                global_index,
                table_index,
                file_index,
            )?);
        }

        let mut object = ObjectFile::new(path.to_path_buf(), target, file_index);
        for section in sections {
            object.add_section(section);
        }
        for symbol in symbols {
            object.add_symbol(symbol);
        }
        object.wasm = Some(WasmObjectInfo {
            types,
            function_types,
            imports,
            body_offsets,
            body_ends,
            data_segments: segment_offsets,
            exports,
        });

        object.validate()?;
        Ok(object)
    }
}

#[allow(clippy::too_many_arguments)]
fn linker_symbol(
    wasm_symbol: &WasmSymbol,
    sections: &[Section],
    function_offsets: &[u64],
    segment_offsets: &[u64],
    imported_funcs: usize,
    code_index: Option<usize>,
    data_index: Option<usize>,
    global_index: Option<usize>,
    table_index: Option<usize>,
    file_index: usize,
) -> LinkResult<Symbol> {
    let undefined = wasm_symbol.is_undefined();

    let (sym_type, section_index, value, size) = match wasm_symbol.kind {
        SYMTAB_FUNCTION => {
            if undefined {
                // Imports keep their element index in `value` so the
                // relocation stage can find which import it is (the name may
                // be a WASM_SYM_EXPLICIT_NAME remap of the import field).
                (SymbolType::Function, None, wasm_symbol.index as u64, 0)
            } else {
                // Function symbols index the whole function index space, whose
                // first entries are the imported functions.
                let defined = (wasm_symbol.index as usize).checked_sub(imported_funcs);
                let value = defined
                    .and_then(|index| function_offsets.get(index).copied())
                    .ok_or_else(|| {
                        LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "WASM function symbol `{}` names function {}, but the module \
                                 imports {} functions and the code section defines {}",
                                wasm_symbol.name,
                                wasm_symbol.index,
                                imported_funcs,
                                function_offsets.len()
                            ),
                        )
                    })?;
                (SymbolType::Function, code_index, value, 0)
            }
        }
        SYMTAB_DATA => {
            if undefined {
                (SymbolType::Object, None, 0, 0)
            } else {
                let segment_start =
                    *segment_offsets
                        .get(wasm_symbol.index as usize)
                        .ok_or_else(|| {
                            LinkError::new(
                                ErrorCode::InvalidObject,
                                format!(
                                    "WASM data symbol `{}` names segment {}, but the data section \
                                 has {}",
                                    wasm_symbol.name,
                                    wasm_symbol.index,
                                    segment_offsets.len()
                                ),
                            )
                        })?;
                (
                    SymbolType::Object,
                    data_index,
                    segment_start + wasm_symbol.data_offset,
                    wasm_symbol.data_size,
                )
            }
        }
        SYMTAB_GLOBAL => (
            SymbolType::Object,
            if undefined { None } else { global_index },
            if undefined {
                wasm_symbol.index as u64
            } else {
                0
            },
            0,
        ),
        SYMTAB_TABLE => (
            SymbolType::Object,
            if undefined { None } else { table_index },
            if undefined {
                wasm_symbol.index as u64
            } else {
                0
            },
            0,
        ),
        SYMTAB_EVENT => (SymbolType::Unknown, None, 0, 0),
        SYMTAB_SECTION => {
            let section_index = wasm_symbol.index as usize;
            if section_index >= sections.len() {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    format!(
                        "WASM section symbol names section {}, but the file has {} sections",
                        section_index,
                        sections.len()
                    ),
                ));
            }
            (SymbolType::Section, Some(section_index), 0, 0)
        }
        other => {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("unknown WASM `linking` symbol kind {}", other),
            ));
        }
    };

    let name = if wasm_symbol.name.is_empty() && sym_type == SymbolType::Section {
        sections[wasm_symbol.index as usize].name.clone()
    } else {
        wasm_symbol.name.clone()
    };

    let binding = if wasm_symbol.flags & SYM_BINDING_LOCAL != 0 {
        SymbolBinding::Local
    } else if wasm_symbol.flags & SYM_BINDING_WEAK != 0 {
        SymbolBinding::Weak
    } else {
        SymbolBinding::Global
    };

    let visibility = if wasm_symbol.flags & SYM_VISIBILITY_HIDDEN != 0 {
        SymbolVisibility::Hidden
    } else {
        SymbolVisibility::Default
    };

    Ok(Symbol {
        name,
        binding,
        visibility,
        sym_type,
        section_index,
        value,
        size,
        is_defined: !undefined,
        // Data symbols cannot be imports: the import section has no data entry,
        // so an undefined data symbol is a cross-object reference that must be
        // resolved by a definition.
        is_imported: undefined && wasm_symbol.kind != SYMTAB_DATA,
        // Objects do not carry an export section; lld only learns exports from
        // the symbol flag, so that is what the reader trusts.
        is_exported: !undefined && wasm_symbol.flags & SYM_EXPORTED != 0,
        file_index: Some(file_index),
        alias_of: None,
        comdat_group: None,
        version: None,
    })
}
