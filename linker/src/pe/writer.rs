//! PE32+ (64-bit Windows) Executable and DLL Writer.

use crate::error::LinkResult;
use crate::pe::header::*;
use crate::pe::import::{ImportSymbol, build_import_table};
use crate::pe::reloc::build_base_reloc_table;
use crate::section::{MergedSection, SectionKind, align_to};
use crate::symbol::Symbol;
use crate::target::{Arch, Target};
use std::fs;
use std::path::Path;

pub struct PeWriter;

impl PeWriter {
    pub fn write_executable(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        imports: &[ImportSymbol],
    ) -> LinkResult<()> {
        let bytes =
            Self::encode_executable(target, entry_va, merged_sections, symbols, imports, false)?;
        fs::write(path, bytes)?;
        Ok(())
    }

    pub fn encode_executable(
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        _symbols: &[Symbol],
        imports: &[ImportSymbol],
        is_dll: bool,
    ) -> LinkResult<Vec<u8>> {
        let machine = match target.arch {
            Arch::AArch64 => IMAGE_FILE_MACHINE_ARM64,
            Arch::X86 => IMAGE_FILE_MACHINE_I386,
            _ => IMAGE_FILE_MACHINE_AMD64,
        };

        let file_alignment = 0x200u32;
        let section_alignment = 0x1000u32;
        let image_base = target.image_base;

        let mut output = Vec::new();

        // 1. DOS Header & DOS Stub (128 bytes standard MSVC/LLVM layout)
        let lfanew = 0x80u32;
        let dos_hdr: [u8; 128] = [
            0x4d, 0x5a, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xff, 0xff,
            0x00, 0x00, 0xb8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x0e, 0x1f, 0xba, 0x0e, 0x00, 0xb4,
            0x09, 0xcd, 0x21, 0xb8, 0x01, 0x4c, 0xcd, 0x21, 0x54, 0x68, 0x69, 0x73, 0x20, 0x70,
            0x72, 0x6f, 0x67, 0x72, 0x61, 0x6d, 0x20, 0x63, 0x61, 0x6e, 0x6e, 0x6f, 0x74, 0x20,
            0x62, 0x65, 0x20, 0x72, 0x75, 0x6e, 0x20, 0x69, 0x6e, 0x20, 0x44, 0x4f, 0x53, 0x20,
            0x6d, 0x6f, 0x64, 0x65, 0x2e, 0x0d, 0x0d, 0x0a, 0x24, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ];
        output.extend_from_slice(&dos_hdr);

        // 2. Prepare Sections
        let mut pe_sections = Vec::new();
        for sec in merged_sections {
            pe_sections.push((
                sec.name.clone(),
                sec.kind,
                sec.flags,
                sec.data.clone(),
                sec.size,
            ));
        }

        // Add .idata section if imports exist
        let mut idata_idx = None;
        if !imports.is_empty() {
            let res = build_import_table(imports, image_base, 0);
            idata_idx = Some(pe_sections.len());
            let data_len = res.data.len() as u64;
            pe_sections.push((
                ".idata".to_string(),
                SectionKind::Data,
                crate::section::flags::READ
                    | crate::section::flags::WRITE
                    | crate::section::flags::ALLOC,
                res.data,
                data_len,
            ));
        }

        // Add .reloc section for ASLR
        let reloc_data = build_base_reloc_table(&[], true);
        let reloc_idx = pe_sections.len();
        pe_sections.push((
            ".reloc".to_string(),
            SectionKind::Rodata,
            crate::section::flags::READ
                | crate::section::flags::ALLOC
                | crate::section::flags::DISCARD,
            reloc_data.clone(),
            reloc_data.len() as u64,
        ));

        let num_sections = pe_sections.len() as u16;
        let opt_hdr_size = 240u16; // PE32+ Optional Header size
        let section_table_size = (num_sections as usize) * 40;
        let headers_unaligned =
            (lfanew as usize) + 4 + 20 + (opt_hdr_size as usize) + section_table_size;
        let headers_size = align_to(headers_unaligned as u64, file_alignment as u64) as u32;

        output.resize(headers_size as usize, 0);

        // 3. Layout Sections in Memory and File
        let mut current_rva = align_to(headers_size as u64, section_alignment as u64) as u32;
        let mut current_file_offset = headers_size;

        let mut section_headers = Vec::with_capacity(num_sections as usize);
        let mut size_of_code = 0u32;
        let mut size_of_init_data = 0u32;
        let mut base_of_code = 0u32;

        for (name, _kind, flags, data, size) in &mut pe_sections {
            let virt_size = (*size).max(data.len() as u64) as u32;
            let raw_size = align_to(data.len() as u64, file_alignment as u64) as u32;

            let mut name_buf = [0u8; 8];
            let name_bytes = name.as_bytes();
            let copy_len = name_bytes.len().min(8);
            name_buf[0..copy_len].copy_from_slice(&name_bytes[0..copy_len]);

            let characteristics = if name == ".reloc" {
                IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_DISCARDABLE
            } else if name == ".idata" {
                IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE
            } else {
                let mut c = 0u32;
                if (*flags & crate::section::flags::READ) != 0 {
                    c |= IMAGE_SCN_MEM_READ;
                }
                if (*flags & crate::section::flags::WRITE) != 0 {
                    c |= IMAGE_SCN_MEM_WRITE;
                }
                if (*flags & crate::section::flags::EXEC) != 0 {
                    c |= IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_CNT_CODE;
                    if base_of_code == 0 {
                        base_of_code = current_rva;
                    }
                    size_of_code += raw_size;
                } else {
                    c |= IMAGE_SCN_CNT_INITIALIZED_DATA;
                    size_of_init_data += raw_size;
                }
                c
            };

            section_headers.push(SectionHeader {
                name: name_buf,
                virtual_size: virt_size,
                virtual_address: current_rva,
                size_of_raw_data: raw_size,
                pointer_to_raw_data: if raw_size > 0 { current_file_offset } else { 0 },
                pointer_to_relocations: 0,
                pointer_to_linenumbers: 0,
                number_of_relocations: 0,
                number_of_linenumbers: 0,
                characteristics,
            });

            if raw_size > 0 {
                let current_len = output.len();
                if (current_file_offset as usize) > current_len {
                    output.resize(current_file_offset as usize, 0);
                }
                output.extend_from_slice(data);
                let padded_len = (current_file_offset + raw_size) as usize;
                if output.len() < padded_len {
                    output.resize(padded_len, 0);
                }
                current_file_offset += raw_size;
            }

            current_rva =
                align_to((current_rva + virt_size) as u64, section_alignment as u64) as u32;
        }

        let size_of_image = current_rva;

        // Re-generate .idata with actual RVA if present
        let mut final_imp_res = None;
        if let Some(id_idx) = idata_idx {
            let idata_rva = section_headers[id_idx].virtual_address;
            let res = build_import_table(imports, image_base, idata_rva);
            let ptr = section_headers[id_idx].pointer_to_raw_data as usize;
            if ptr + res.data.len() <= output.len() {
                output[ptr..ptr + res.data.len()].copy_from_slice(&res.data);
            }
            final_imp_res = Some(res);
        }

        // 4. Fill in PE Header Signature at lfanew
        let pe_off = lfanew as usize;
        output[pe_off..pe_off + 4].copy_from_slice(&PE_SIGNATURE);

        // 5. Fill in COFF File Header (20 bytes)
        let mut characteristics = IMAGE_FILE_EXECUTABLE_IMAGE | IMAGE_FILE_LARGE_ADDRESS_AWARE;
        if is_dll {
            characteristics |= IMAGE_FILE_DLL;
        }
        let coff_hdr = CoffHeader {
            machine,
            number_of_sections: num_sections,
            time_date_stamp: 0x60000000,
            pointer_to_symbol_table: 0,
            number_of_symbols: 0,
            size_of_optional_header: opt_hdr_size,
            characteristics,
        };
        let mut coff_buf = Vec::with_capacity(20);
        coff_buf.extend_from_slice(&coff_hdr.machine.to_le_bytes());
        coff_buf.extend_from_slice(&coff_hdr.number_of_sections.to_le_bytes());
        coff_buf.extend_from_slice(&coff_hdr.time_date_stamp.to_le_bytes());
        coff_buf.extend_from_slice(&coff_hdr.pointer_to_symbol_table.to_le_bytes());
        coff_buf.extend_from_slice(&coff_hdr.number_of_symbols.to_le_bytes());
        coff_buf.extend_from_slice(&coff_hdr.size_of_optional_header.to_le_bytes());
        coff_buf.extend_from_slice(&coff_hdr.characteristics.to_le_bytes());
        output[pe_off + 4..pe_off + 24].copy_from_slice(&coff_buf);

        // 6. Fill in Optional Header (PE32+ 240 bytes)
        let opt_off = pe_off + 24;
        let mut opt_buf = vec![0u8; 240];
        opt_buf[0..2].copy_from_slice(&PE32PLUS_MAGIC.to_le_bytes()); // Magic: 0x020B
        opt_buf[2] = 14; // MajorLinkerVersion
        opt_buf[3] = 0; // MinorLinkerVersion
        opt_buf[4..8].copy_from_slice(&size_of_code.to_le_bytes());
        opt_buf[8..12].copy_from_slice(&size_of_init_data.to_le_bytes());
        opt_buf[12..16].copy_from_slice(&0u32.to_le_bytes()); // SizeOfUninitializedData

        let entry_rva = if entry_va >= image_base {
            (entry_va - image_base) as u32
        } else {
            section_headers
                .first()
                .map(|s| s.virtual_address)
                .unwrap_or(0x1000)
        };
        opt_buf[16..20].copy_from_slice(&entry_rva.to_le_bytes());
        opt_buf[20..24].copy_from_slice(&base_of_code.to_le_bytes());
        opt_buf[24..32].copy_from_slice(&image_base.to_le_bytes());
        opt_buf[32..36].copy_from_slice(&section_alignment.to_le_bytes());
        opt_buf[36..40].copy_from_slice(&file_alignment.to_le_bytes());
        opt_buf[40..42].copy_from_slice(&6u16.to_le_bytes()); // MajorOSVersion
        opt_buf[42..44].copy_from_slice(&0u16.to_le_bytes());
        opt_buf[44..46].copy_from_slice(&0u16.to_le_bytes()); // MajorImageVersion
        opt_buf[46..48].copy_from_slice(&0u16.to_le_bytes());
        opt_buf[48..50].copy_from_slice(&6u16.to_le_bytes()); // MajorSubsystemVersion
        opt_buf[50..52].copy_from_slice(&0u16.to_le_bytes());
        opt_buf[52..56].copy_from_slice(&0u32.to_le_bytes()); // Win32VersionValue
        opt_buf[56..60].copy_from_slice(&size_of_image.to_le_bytes());
        opt_buf[60..64].copy_from_slice(&headers_size.to_le_bytes());
        opt_buf[64..68].copy_from_slice(&0u32.to_le_bytes()); // CheckSum
        opt_buf[68..70].copy_from_slice(&IMAGE_SUBSYSTEM_WINDOWS_CUI.to_le_bytes());
        let dll_chars = IMAGE_DLLCHARACTERISTICS_HIGH_ENTROPY_VA
            | IMAGE_DLLCHARACTERISTICS_DYNAMIC_BASE
            | IMAGE_DLLCHARACTERISTICS_NX_COMPAT
            | IMAGE_DLLCHARACTERISTICS_TERMINAL_SERVER_AWARE;
        opt_buf[70..72].copy_from_slice(&dll_chars.to_le_bytes());
        opt_buf[72..80].copy_from_slice(&0x100000u64.to_le_bytes()); // SizeOfStackReserve (1MB)
        opt_buf[80..88].copy_from_slice(&0x1000u64.to_le_bytes()); // SizeOfStackCommit (4KB)
        opt_buf[88..96].copy_from_slice(&0x100000u64.to_le_bytes()); // SizeOfHeapReserve (1MB)
        opt_buf[96..104].copy_from_slice(&0x1000u64.to_le_bytes()); // SizeOfHeapCommit (4KB)
        opt_buf[104..108].copy_from_slice(&0u32.to_le_bytes()); // LoaderFlags
        opt_buf[108..112].copy_from_slice(&16u32.to_le_bytes()); // NumberOfRvaAndSizes

        // Data Directories (112..240)
        if let Some(ref imp_res) = final_imp_res {
            let id_idx = idata_idx.unwrap();
            let idata_rva = section_headers[id_idx].virtual_address;
            let imp_dir_off = 112 + IMAGE_DIRECTORY_ENTRY_IMPORT * 8;
            opt_buf[imp_dir_off..imp_dir_off + 4].copy_from_slice(&idata_rva.to_le_bytes());
            opt_buf[imp_dir_off + 4..imp_dir_off + 8]
                .copy_from_slice(&imp_res.import_descriptor_size.to_le_bytes());

            if imp_res.iat_rva > 0 {
                let iat_dir_off = 112 + IMAGE_DIRECTORY_ENTRY_IAT * 8;
                opt_buf[iat_dir_off..iat_dir_off + 4]
                    .copy_from_slice(&imp_res.iat_rva.to_le_bytes());
                opt_buf[iat_dir_off + 4..iat_dir_off + 8]
                    .copy_from_slice(&imp_res.iat_size.to_le_bytes());
            }
        }

        // Base Relocation directory
        let reloc_rva = section_headers[reloc_idx].virtual_address;
        let reloc_size = section_headers[reloc_idx].virtual_size;
        let reloc_dir_off = 112 + IMAGE_DIRECTORY_ENTRY_BASERELOC * 8;
        opt_buf[reloc_dir_off..reloc_dir_off + 4].copy_from_slice(&reloc_rva.to_le_bytes());
        opt_buf[reloc_dir_off + 4..reloc_dir_off + 8].copy_from_slice(&reloc_size.to_le_bytes());

        output[opt_off..opt_off + 240].copy_from_slice(&opt_buf);

        // 7. Write Section Table Entries
        let mut sec_table_off = opt_off + 240;
        for sh in &section_headers {
            output[sec_table_off..sec_table_off + 8].copy_from_slice(&sh.name);
            output[sec_table_off + 8..sec_table_off + 12]
                .copy_from_slice(&sh.virtual_size.to_le_bytes());
            output[sec_table_off + 12..sec_table_off + 16]
                .copy_from_slice(&sh.virtual_address.to_le_bytes());
            output[sec_table_off + 16..sec_table_off + 20]
                .copy_from_slice(&sh.size_of_raw_data.to_le_bytes());
            output[sec_table_off + 20..sec_table_off + 24]
                .copy_from_slice(&sh.pointer_to_raw_data.to_le_bytes());
            output[sec_table_off + 24..sec_table_off + 28]
                .copy_from_slice(&sh.pointer_to_relocations.to_le_bytes());
            output[sec_table_off + 28..sec_table_off + 32]
                .copy_from_slice(&sh.pointer_to_linenumbers.to_le_bytes());
            output[sec_table_off + 32..sec_table_off + 34]
                .copy_from_slice(&sh.number_of_relocations.to_le_bytes());
            output[sec_table_off + 34..sec_table_off + 36]
                .copy_from_slice(&sh.number_of_linenumbers.to_le_bytes());
            output[sec_table_off + 36..sec_table_off + 40]
                .copy_from_slice(&sh.characteristics.to_le_bytes());
            sec_table_off += 40;
        }

        Ok(output)
    }
}
