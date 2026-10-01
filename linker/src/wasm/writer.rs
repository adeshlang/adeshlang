//! WebAssembly Binary Writer.

use crate::error::LinkResult;
use crate::section::MergedSection;
use crate::symbol::Symbol;
use crate::target::Target;
use crate::wasm::sections::*;
use std::fs;
use std::path::Path;

pub struct WasmWriter;

impl WasmWriter {
    pub fn write_binary(
        path: &Path,
        _target: &Target,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
    ) -> LinkResult<()> {
        let bytes = Self::encode_binary(merged_sections, symbols)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn encode_binary(
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
    ) -> LinkResult<Vec<u8>> {
        let mut output = Vec::new();

        // 1. WASM Magic & Version
        output.extend_from_slice(&WASM_MAGIC);
        output.extend_from_slice(&WASM_VERSION);

        // 2. Type Section (ID 1): Type 0 = () -> (i64)
        let mut type_payload = Vec::new();
        encode_u32_leb128(1, &mut type_payload); // 1 type entry
        type_payload.push(0x60); // func type
        type_payload.push(0x00); // 0 params
        type_payload.push(0x01); // 1 result
        type_payload.push(0x7E); // i64 result
        emit_section(WASM_SEC_TYPE, &type_payload, &mut output);

        // 3. Function Section (ID 3): 1 function with type index 0
        let mut func_payload = Vec::new();
        encode_u32_leb128(1, &mut func_payload); // 1 function
        encode_u32_leb128(0, &mut func_payload); // type 0
        emit_section(WASM_SEC_FUNCTION, &func_payload, &mut output);

        // 4. Memory Section (ID 5): 1 memory, limits: min 1 page, max 256 pages
        let mut mem_payload = Vec::new();
        encode_u32_leb128(1, &mut mem_payload); // 1 memory
        mem_payload.push(0x01); // has max
        encode_u32_leb128(1, &mut mem_payload); // min = 1
        encode_u32_leb128(256, &mut mem_payload); // max = 256
        emit_section(WASM_SEC_MEMORY, &mem_payload, &mut output);

        // 5. Global Section (ID 6): __stack_pointer (i32, mut) initial 1048576 (1MB)
        let mut global_payload = Vec::new();
        encode_u32_leb128(1, &mut global_payload); // 1 global
        global_payload.push(0x7F); // i32
        global_payload.push(0x01); // mut
        global_payload.push(0x41); // i32.const
        encode_u32_leb128(1048576, &mut global_payload);
        global_payload.push(0x0B); // end
        emit_section(WASM_SEC_GLOBAL, &global_payload, &mut output);

        // 6. Export Section (ID 7): export "memory" (mem 0), export "_start" (func 0)
        let mut exp_payload = Vec::new();
        let mut exports = vec![
            ("memory", 0x02u8, 0u32), // memory 0
            ("_start", 0x00u8, 0u32), // function 0
        ];
        for sym in symbols {
            if sym.is_exported && sym.name != "_start" {
                exports.push((&sym.name, 0x00, 0));
            }
        }

        encode_u32_leb128(exports.len() as u32, &mut exp_payload);
        for (name, kind, index) in exports {
            let name_bytes = name.as_bytes();
            encode_u32_leb128(name_bytes.len() as u32, &mut exp_payload);
            exp_payload.extend_from_slice(name_bytes);
            exp_payload.push(kind);
            encode_u32_leb128(index, &mut exp_payload);
        }
        emit_section(WASM_SEC_EXPORT, &exp_payload, &mut output);

        // 7. Code Section (ID 10): 1 function body (contains its own locals header & opcodes)
        let mut code_payload = Vec::new();
        encode_u32_leb128(1, &mut code_payload); // 1 function body

        let mut func_body = Vec::new();

        // Find code payload from merged sections if any
        let mut has_code = false;
        for sec in merged_sections {
            if sec.is_executable() && !sec.data.is_empty() {
                func_body.extend_from_slice(&sec.data);
                has_code = true;
                break;
            }
        }
        if !has_code {
            func_body.push(0x00); // 0 locals
            func_body.push(0x42); // i64.const 0
            func_body.push(0x00);
            func_body.push(0x0B); // end opcode
        } else if func_body.last() != Some(&0x0B) {
            func_body.push(0x0B);
        }

        encode_u32_leb128(func_body.len() as u32, &mut code_payload);
        code_payload.extend_from_slice(&func_body);
        emit_section(WASM_SEC_CODE, &code_payload, &mut output);

        // 8. Data Section (ID 11): static data if any
        let mut total_data = Vec::new();
        for sec in merged_sections {
            if !sec.is_executable()
                && !sec.data.is_empty()
                && sec.kind != crate::section::SectionKind::Bss
            {
                total_data.extend_from_slice(&sec.data);
            }
        }

        if !total_data.is_empty() {
            let mut data_payload = Vec::new();
            encode_u32_leb128(1, &mut data_payload); // 1 data segment
            data_payload.push(0x00); // active segment, memory index 0
            data_payload.push(0x41); // i32.const offset
            encode_u32_leb128(1024, &mut data_payload); // placed at offset 1024
            data_payload.push(0x0B); // end
            encode_u32_leb128(total_data.len() as u32, &mut data_payload);
            data_payload.extend_from_slice(&total_data);
            emit_section(WASM_SEC_DATA, &data_payload, &mut output);
        }

        Ok(output)
    }
}

fn emit_section(sec_id: u8, payload: &[u8], output: &mut Vec<u8>) {
    output.push(sec_id);
    encode_u32_leb128(payload.len() as u32, output);
    output.extend_from_slice(payload);
}
