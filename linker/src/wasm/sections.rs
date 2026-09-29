//! WebAssembly section identifiers, LEB128 encoders/decoders, and reader.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::ObjectFile;
use crate::section::{Section, SectionKind, flags};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, ObjectFormat, Os, PointerWidth, Target};
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

        if &bytes[0..4] != &WASM_MAGIC {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "invalid WASM magic",
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
        };

        let mut obj = ObjectFile::new(path.to_path_buf(), target, file_index);
        let mut offset = 8;

        while offset < bytes.len() {
            let sec_id = bytes[offset];
            offset += 1;
            let sec_len = decode_u32_leb128(bytes, &mut offset)? as usize;

            if offset + sec_len > bytes.len() {
                return Err(LinkError::new(
                    ErrorCode::InvalidObject,
                    "truncated WASM section",
                ));
            }

            let sec_data = &bytes[offset..offset + sec_len];
            let (sec_name, sec_kind) = match sec_id {
                WASM_SEC_CUSTOM => ("custom", SectionKind::Custom),
                WASM_SEC_TYPE => ("type", SectionKind::Rodata),
                WASM_SEC_IMPORT => ("import", SectionKind::Rodata),
                WASM_SEC_FUNCTION => ("function", SectionKind::Rodata),
                WASM_SEC_TABLE => ("table", SectionKind::Rodata),
                WASM_SEC_MEMORY => ("memory", SectionKind::Rodata),
                WASM_SEC_GLOBAL => ("global", SectionKind::Data),
                WASM_SEC_EXPORT => ("export", SectionKind::Rodata),
                WASM_SEC_CODE => ("code", SectionKind::Text),
                WASM_SEC_DATA => ("data", SectionKind::Data),
                _ => ("unknown", SectionKind::Custom),
            };

            let sec = Section {
                name: format!(".wasm.{}", sec_name),
                kind: sec_kind,
                flags: flags::READ
                    | flags::ALLOC
                    | if sec_kind == SectionKind::Text {
                        flags::EXEC
                    } else {
                        0
                    },
                alignment: 1,
                virtual_address: 0,
                file_offset: offset as u64,
                size: sec_len as u64,
                data: sec_data.to_vec(),
                relocations: Vec::new(),
                comdat_group: None,
                file_index: Some(file_index),
                is_live: true,
                is_folded: false,
                folded_into: None,
            };

            obj.add_section(sec);
            offset += sec_len;
        }

        // Add a default entry symbol
        obj.add_symbol(Symbol {
            name: "_start".to_string(),
            binding: SymbolBinding::Global,
            visibility: SymbolVisibility::Default,
            sym_type: SymbolType::Function,
            section_index: Some(0),
            value: 0,
            size: 0,
            is_defined: true,
            is_imported: false,
            is_exported: true,
            file_index: Some(file_index),
            alias_of: None,
            comdat_group: None,
            version: None,
        });

        obj.validate()?;
        Ok(obj)
    }
}
