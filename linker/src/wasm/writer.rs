//! WebAssembly Binary Writer.
//!
//! Emits a module from the link plan: the plan holds the final type, function,
//! data and export numbering, and the writer copies function bodies and data
//! payloads byte-for-byte out of their input objects. Nothing here decides
//! numbering, and nothing here re-derives a signature.

use std::fs;
use std::path::Path;

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::target::Target;
use crate::wasm::link_info::{MEMORY_EXPORT, WasmLinkInfo};
use crate::wasm::sections::encode_u32_leb128;

pub struct WasmWriter;

impl WasmWriter {
    pub fn write_binary(
        path: &Path,
        _target: &Target,
        objects: &[ObjectFile],
        info: &WasmLinkInfo,
    ) -> LinkResult<()> {
        let bytes = Self::encode_binary(objects, info)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    /// Encode a complete module: header, then every non-empty section in the
    /// order the spec requires.
    pub fn encode_binary(objects: &[ObjectFile], info: &WasmLinkInfo) -> LinkResult<Vec<u8>> {
        let mut module = Vec::new();
        // `\0asm` and version 1: the only version a module (as opposed to a
        // relocatable object) may declare.
        module.extend_from_slice(&[0x00, 0x61, 0x73, 0x6D, 0x01, 0x00, 0x00, 0x00]);

        if !info.types.is_empty() {
            let mut payload = Vec::new();
            encode_u32_leb128(info.types.len() as u32, &mut payload);
            for signature in &info.types {
                payload.push(0x60);
                encode_u32_leb128(signature.params.len() as u32, &mut payload);
                payload.extend_from_slice(&signature.params);
                encode_u32_leb128(signature.results.len() as u32, &mut payload);
                payload.extend_from_slice(&signature.results);
            }
            emit_section(&mut module, 1, payload);
        }

        if !info.imports.is_empty() {
            let mut payload = Vec::new();
            encode_u32_leb128(info.imports.len() as u32, &mut payload);
            for import in &info.imports {
                write_name(&mut payload, &import.module);
                write_name(&mut payload, &import.name);
                payload.push(0x00);
                encode_u32_leb128(import.type_index, &mut payload);
            }
            emit_section(&mut module, 2, payload);
        }

        if !info.functions.is_empty() {
            let mut payload = Vec::new();
            encode_u32_leb128(info.functions.len() as u32, &mut payload);
            for function in &info.functions {
                encode_u32_leb128(function.type_index, &mut payload);
            }
            emit_section(&mut module, 3, payload);
        }

        if info.table_min > 0 {
            let mut payload = Vec::new();
            encode_u32_leb128(1, &mut payload);
            // `funcref` element type, limits flag 0 (minimum only).
            payload.push(0x70);
            payload.push(0x00);
            encode_u32_leb128(info.table_min, &mut payload);
            emit_section(&mut module, 4, payload);
        }

        {
            let mut payload = Vec::new();
            encode_u32_leb128(1, &mut payload);
            // No maximum: the heap above the stack grows with `memory.grow`.
            payload.push(0x00);
            encode_u32_leb128(info.initial_pages, &mut payload);
            emit_section(&mut module, 5, payload);
        }

        {
            let mut payload = Vec::new();
            encode_u32_leb128(1, &mut payload);
            payload.push(crate::wasm::link_info::I32);
            payload.push(0x01);
            push_i32_const(&mut payload, info.stack_top as i64)?;
            payload.push(0x0B);
            emit_section(&mut module, 6, payload);
        }

        {
            let entry_count = info.exports.len() + 1;
            let mut payload = Vec::new();
            encode_u32_leb128(entry_count as u32, &mut payload);
            for export in &info.exports {
                if export.name == MEMORY_EXPORT {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "the link plan exports function `{}`, but `{}` is reserved for linear \
                             memory",
                            export.name, MEMORY_EXPORT
                        ),
                    )
                    .with_suggestion(
                        "rename the function export; the host finds memory under the name \
                         `memory`",
                    ));
                }
                write_name(&mut payload, &export.name);
                payload.push(0x00);
                encode_u32_leb128(export.index, &mut payload);
            }
            write_name(&mut payload, MEMORY_EXPORT);
            payload.push(0x02);
            encode_u32_leb128(0, &mut payload);
            emit_section(&mut module, 7, payload);
        }

        if !info.table_entries.is_empty() {
            let mut payload = Vec::new();
            encode_u32_leb128(1, &mut payload);
            // One active segment for table 0, starting at slot 1: slot 0 stays
            // uninitialised so calling a null function pointer traps.
            payload.push(0x00);
            payload.push(0x41);
            payload.push(0x01);
            payload.push(0x0B);
            encode_u32_leb128(info.table_entries.len() as u32, &mut payload);
            for entry in &info.table_entries {
                encode_u32_leb128(*entry, &mut payload);
            }
            emit_section(&mut module, 9, payload);
        }

        if info.data_count_required {
            let mut payload = Vec::new();
            encode_u32_leb128(info.data_segments.len() as u32, &mut payload);
            emit_section(&mut module, 12, payload);
        }

        if !info.functions.is_empty() {
            let mut payload = Vec::new();
            encode_u32_leb128(info.functions.len() as u32, &mut payload);
            for (index, function) in info.functions.iter().enumerate() {
                let object = &objects[function.object_index];
                let section = &object.sections[function.code_section_index];
                let start = usize::try_from(function.body_offset)
                    .ok()
                    .filter(|start| *start <= section.data.len());
                let end = usize::try_from(function.body_end)
                    .ok()
                    .filter(|end| *end <= section.data.len());
                let (Some(start), Some(end)) = (start, end) else {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "function body {} of `{}` spans bytes {}..{}, outside its {} byte \
                             code section",
                            index,
                            object.path.display(),
                            function.body_offset,
                            function.body_end,
                            section.data.len()
                        ),
                    ));
                };
                if start > end {
                    return Err(LinkError::new(
                        ErrorCode::InvalidObject,
                        format!(
                            "function body {} of `{}` ends before it starts",
                            index,
                            object.path.display()
                        ),
                    ));
                }
                let body = &section.data[start..end];
                encode_u32_leb128(body.len() as u32, &mut payload);
                payload.extend_from_slice(body);
            }
            emit_section(&mut module, 10, payload);
        }

        if !info.data_segments.is_empty() {
            let mut payload = Vec::new();
            encode_u32_leb128(info.data_segments.len() as u32, &mut payload);
            for (index, segment) in info.data_segments.iter().enumerate() {
                let object = &objects[segment.object_index];
                let section = &object.sections[segment.data_section_index];
                let start = usize::try_from(segment.payload_offset)
                    .ok()
                    .filter(|start| *start <= section.data.len());
                let len = usize::try_from(segment.payload_len).ok();
                let payload_bytes = match (start, len) {
                    (Some(start), Some(len))
                        if start
                            .checked_add(len)
                            .is_some_and(|end| end <= section.data.len()) =>
                    {
                        &section.data[start..start + len]
                    }
                    _ => {
                        return Err(LinkError::new(
                            ErrorCode::InvalidObject,
                            format!(
                                "data segment {} of `{}` spans bytes {}..+{}, outside its {} byte \
                                 data section",
                                index,
                                object.path.display(),
                                segment.payload_offset,
                                segment.payload_len,
                                section.data.len()
                            ),
                        ));
                    }
                };

                // Segment index order is preserved because code may refer to a
                // segment by index (`memory.init`), not only by offset.
                payload.push(0x00);
                push_i32_const(&mut payload, segment.memory_offset as i64)?;
                payload.push(0x0B);
                encode_u32_leb128(payload_bytes.len() as u32, &mut payload);
                payload.extend_from_slice(payload_bytes);
            }
            emit_section(&mut module, 11, payload);
        }

        Ok(module)
    }
}

fn emit_section(module: &mut Vec<u8>, id: u8, payload: Vec<u8>) {
    module.push(id);
    encode_u32_leb128(payload.len() as u32, module);
    module.extend_from_slice(&payload);
}

fn write_name(buf: &mut Vec<u8>, name: &str) {
    encode_u32_leb128(name.len() as u32, buf);
    buf.extend_from_slice(name.as_bytes());
}

/// `i32.const <value>`: the signed LEB the operand requires. A value that does
/// not fit in 32 bits means the plan overflowed the address space, which is a
/// link error rather than a wrapped address.
fn push_i32_const(buf: &mut Vec<u8>, value: i64) -> LinkResult<()> {
    if value < i32::MIN as i64 || value > i32::MAX as i64 {
        return Err(LinkError::new(
            ErrorCode::MemoryExhausted,
            format!(
                "linear-memory offset {} does not fit in a 32-bit WebAssembly address",
                value
            ),
        ));
    }
    buf.push(0x41);
    encode_i32_sleb128(value as i32, buf);
    Ok(())
}

fn encode_i32_sleb128(mut value: i32, buf: &mut Vec<u8>) {
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7;
        let done = (value == 0 && byte & 0x40 == 0) || (value == -1 && byte & 0x40 != 0);
        if done {
            buf.push(byte);
            return;
        }
        buf.push(byte | 0x80);
    }
}
