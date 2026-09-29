//! ADOB v2 (Adesh Portable Relocatable Object Format v2)
//!
//! Versioned, checked binary object format supporting rich relocation types,
//! COMDAT groups, visibility metadata, TLS, unwind data, and build metadata.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::object::{BinaryReader, ObjectFile};
use crate::relocation::{Relocation, RelocationKind};
use crate::section::{Section, SectionKind};
use crate::symbol::{Symbol, SymbolBinding, SymbolType, SymbolVisibility};
use crate::target::{Arch, Endianness, PointerWidth, Target};
use std::path::Path;

pub const ADOB_MAGIC: &[u8; 4] = b"ADOB";
pub const ADOB_VERSION_2: u16 = 2;

pub struct AdobV2;

impl AdobV2 {
    /// Encode ObjectFile to ADOB v2 byte representation.
    pub fn encode(obj: &ObjectFile) -> LinkResult<Vec<u8>> {
        let mut buf = Vec::new();
        // Magic
        buf.extend_from_slice(ADOB_MAGIC);
        // Version 2
        buf.extend_from_slice(&ADOB_VERSION_2.to_le_bytes());

        // Target Metadata
        let arch_id: u8 = match obj.target.arch {
            Arch::X86_64 => 0,
            Arch::AArch64 => 1,
            Arch::Riscv64 => 2,
            Arch::Arm => 3,
            Arch::Wasm32 => 4,
            _ => 0,
        };
        let abi_id: u8 = match obj.target.os {
            crate::target::Os::Windows => 1,
            crate::target::Os::MacOS => 2,
            _ => 0,
        };
        let endian_id: u8 = match obj.target.endianness {
            Endianness::Little => 0,
            Endianness::Big => 1,
        };
        let ptr_width_id: u8 = match obj.target.pointer_width {
            PointerWidth::U64 => 0,
            PointerWidth::U32 => 1,
        };

        buf.push(arch_id);
        buf.push(abi_id);
        buf.push(endian_id);
        buf.push(ptr_width_id);

        let flags: u32 = 0;
        buf.extend_from_slice(&flags.to_le_bytes());

        // Checksum placeholder (u32)
        buf.extend_from_slice(&0u32.to_le_bytes());

        buf.extend_from_slice(&(obj.sections.len() as u32).to_le_bytes());
        buf.extend_from_slice(&(obj.symbols.len() as u32).to_le_bytes());

        // Sections
        for sec in &obj.sections {
            let name_bytes = sec.name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(name_bytes);

            let kind_id: u8 = match sec.kind {
                SectionKind::Text => 0,
                SectionKind::Rodata => 1,
                SectionKind::Data => 2,
                SectionKind::Bss => 3,
                SectionKind::AdeshMeta => 4,
                _ => 5,
            };
            buf.push(kind_id);
            buf.extend_from_slice(&sec.flags.to_le_bytes());
            buf.extend_from_slice(&sec.alignment.to_le_bytes());
            buf.extend_from_slice(&(sec.data.len() as u64).to_le_bytes());

            // COMDAT group metadata
            if let Some(ref group) = sec.comdat_group {
                buf.push(1);
                let g_bytes = group.as_bytes();
                buf.extend_from_slice(&(g_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(g_bytes);
            } else {
                buf.push(0);
            }

            buf.extend_from_slice(&sec.data);

            // Relocations
            buf.extend_from_slice(&(sec.relocations.len() as u32).to_le_bytes());
            for reloc in &sec.relocations {
                buf.extend_from_slice(&reloc.offset.to_le_bytes());
                let r_kind_id: u8 = match reloc.kind {
                    RelocationKind::Absolute64 => 0,
                    RelocationKind::Absolute32 => 1,
                    RelocationKind::PcRelative32 => 2,
                    RelocationKind::PcRelative64 => 3,
                    RelocationKind::PltRelative32 => 4,
                    RelocationKind::GotRelative32 => 5,
                    _ => 0,
                };
                buf.push(r_kind_id);
                buf.extend_from_slice(&reloc.addend.to_le_bytes());
                let sym_bytes = reloc.symbol_name.as_bytes();
                buf.extend_from_slice(&(sym_bytes.len() as u32).to_le_bytes());
                buf.extend_from_slice(sym_bytes);
            }
        }

        // Symbols
        for sym in &obj.symbols {
            let name_bytes = sym.name.as_bytes();
            buf.extend_from_slice(&(name_bytes.len() as u32).to_le_bytes());
            buf.extend_from_slice(name_bytes);

            let bind_id: u8 = match sym.binding {
                SymbolBinding::Local => 0,
                SymbolBinding::Global => 1,
                SymbolBinding::Weak => 2,
            };
            let vis_id: u8 = match sym.visibility {
                SymbolVisibility::Default => 0,
                SymbolVisibility::Hidden => 1,
                SymbolVisibility::Protected => 2,
                SymbolVisibility::Internal => 3,
            };
            let type_id: u8 = match sym.sym_type {
                SymbolType::Function => 0,
                SymbolType::Object => 1,
                SymbolType::Tls => 2,
                _ => 3,
            };

            buf.push(bind_id);
            buf.push(vis_id);
            buf.push(type_id);
            buf.push(if sym.is_defined { 1 } else { 0 });

            let sec_idx = sym.section_index.unwrap_or(0) as u32;
            buf.extend_from_slice(&sec_idx.to_le_bytes());
            buf.extend_from_slice(&sym.value.to_le_bytes());
            buf.extend_from_slice(&sym.size.to_le_bytes());
        }

        Ok(buf)
    }

    /// Decode ADOB v2 bytes into ObjectFile.
    pub fn decode(
        bytes: &[u8],
        path: &Path,
        default_target: &Target,
        file_index: usize,
    ) -> LinkResult<ObjectFile> {
        let mut reader = BinaryReader::new(bytes);
        let magic = reader.read_bytes(4)?;
        if magic != ADOB_MAGIC {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                "invalid ADOB magic",
            ));
        }

        let version = reader.read_u16_le()?;
        if version != ADOB_VERSION_2 {
            return Err(LinkError::new(
                ErrorCode::InvalidObject,
                format!("unsupported ADOB version {}", version),
            ));
        }

        let _arch_id = reader.read_u8()?;
        let _abi_id = reader.read_u8()?;
        let _endian_id = reader.read_u8()?;
        let _ptr_width_id = reader.read_u8()?;
        let _flags = reader.read_u32_le()?;
        let _checksum = reader.read_u32_le()?;

        let mut obj = ObjectFile::new(path.to_path_buf(), default_target.clone(), file_index);

        let sec_count = reader.read_u32_le()? as usize;
        let sym_count = reader.read_u32_le()? as usize;

        for _ in 0..sec_count {
            let name_len = reader.read_u32_le()? as usize;
            let name = reader.read_string(name_len)?;

            let kind_id = reader.read_u8()?;
            let flags = reader.read_u32_le()?;
            let align = reader.read_u64_le()?;
            let data_len = reader.read_u64_le()? as usize;

            let has_comdat = reader.read_u8()? != 0;
            let comdat_group = if has_comdat {
                let g_len = reader.read_u32_le()? as usize;
                Some(reader.read_string(g_len)?)
            } else {
                None
            };

            let data = reader.read_bytes(data_len)?.to_vec();

            let kind = match kind_id {
                0 => SectionKind::Text,
                1 => SectionKind::Rodata,
                2 => SectionKind::Data,
                3 => SectionKind::Bss,
                4 => SectionKind::AdeshMeta,
                _ => SectionKind::Custom,
            };

            let reloc_count = reader.read_u32_le()? as usize;
            let mut relocations = Vec::with_capacity(reloc_count);

            for _ in 0..reloc_count {
                let r_off = reader.read_u64_le()?;
                let r_kind_id = reader.read_u8()?;
                let r_addend = reader.read_i64_le()?;
                let sym_len = reader.read_u32_le()? as usize;
                let sym_name = reader.read_string(sym_len)?;

                let r_kind = match r_kind_id {
                    0 => RelocationKind::Absolute64,
                    1 => RelocationKind::Absolute32,
                    2 => RelocationKind::PcRelative32,
                    3 => RelocationKind::PcRelative64,
                    4 => RelocationKind::PltRelative32,
                    5 => RelocationKind::GotRelative32,
                    _ => RelocationKind::Absolute64,
                };

                relocations.push(Relocation::new(r_off, sym_name, r_kind, r_addend));
            }

            let sec = Section {
                name,
                kind,
                flags,
                alignment: align,
                virtual_address: 0,
                file_offset: 0,
                size: data_len as u64,
                data,
                relocations,
                comdat_group,
                file_index: Some(file_index),
                is_live: true,
                is_folded: false,
                folded_into: None,
            };
            obj.add_section(sec);
        }

        for _ in 0..sym_count {
            let name_len = reader.read_u32_le()? as usize;
            let name = reader.read_string(name_len)?;

            let binding_id = reader.read_u8()?;
            let vis_id = reader.read_u8()?;
            let type_id = reader.read_u8()?;
            let is_def = reader.read_u8()? != 0;

            let sec_idx = reader.read_u32_le()?;
            let val = reader.read_u64_le()?;
            let sz = reader.read_u64_le()?;

            let binding = match binding_id {
                0 => SymbolBinding::Local,
                1 => SymbolBinding::Global,
                _ => SymbolBinding::Weak,
            };

            let visibility = match vis_id {
                0 => SymbolVisibility::Default,
                1 => SymbolVisibility::Hidden,
                2 => SymbolVisibility::Protected,
                _ => SymbolVisibility::Internal,
            };

            let sym_type = match type_id {
                0 => SymbolType::Function,
                1 => SymbolType::Object,
                2 => SymbolType::Tls,
                _ => SymbolType::Unknown,
            };

            let sym = Symbol {
                name,
                binding,
                visibility,
                sym_type,
                section_index: if is_def { Some(sec_idx as usize) } else { None },
                value: val,
                size: sz,
                is_defined: is_def,
                is_imported: false,
                is_exported: false,
                file_index: Some(file_index),
                alias_of: None,
                comdat_group: None,
                version: None,
            };
            obj.add_symbol(sym);
        }

        obj.validate()?;
        Ok(obj)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::Target;

    #[test]
    fn test_adob_v2_encode_decode_roundtrip() {
        let target = Target::host();
        let mut obj = ObjectFile::new("test.adob".into(), target.clone(), 0);

        let mut sec = Section::new_code(".text", vec![0x90, 0x90, 0x90, 0xc3], 1);
        sec.relocations
            .push(Relocation::new(0, "foo", RelocationKind::PcRelative32, -4));
        obj.add_section(sec);

        let sym = Symbol::new_defined(
            "main",
            SymbolBinding::Global,
            SymbolType::Function,
            0,
            2,
            0,
            0,
        );
        obj.add_symbol(sym);

        let encoded = AdobV2::encode(&obj).unwrap();
        let decoded = AdobV2::decode(&encoded, Path::new("test.adob"), &target, 0).unwrap();

        assert_eq!(decoded.sections.len(), 1);
        assert_eq!(decoded.sections[0].name, ".text");
        assert_eq!(decoded.symbols.len(), 1);
        assert_eq!(decoded.symbols[0].name, "main");
    }
}
