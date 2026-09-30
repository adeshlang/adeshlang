//! PE32+ (64-bit Windows) Executable and DLL Writer.

use crate::error::{ErrorCode, LinkError, LinkResult};
use crate::pe::header::*;
use crate::pe::import::{ImportSymbol, ImportTableResult, build_import_table};
use crate::pe::reloc::build_base_reloc_table;
use crate::section::{MergedSection, align_to};
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
        Self::write_executable_with_layout(
            path,
            target,
            entry_va,
            merged_sections,
            symbols,
            imports,
            None,
            &[],
        )
    }

    /// Write a PE executable, using the layout engine's import table and base
    /// relocation records when available.
    ///
    /// `import_info` MUST be the same `ImportTableResult` whose bytes already
    /// live in the `.idata` merged section and whose `symbol_iat_rvas` were
    /// used to patch import thunks and `__imp_*` references. Rebuilding the
    /// table here from a differently ordered `imports` list would shift ILT /
    /// IAT offsets and send every thunk through the wrong slot (this was a
    /// real bug: thunks jumped into the unpatched ILT and crashed).
    #[allow(clippy::too_many_arguments)]
    pub fn write_executable_with_layout(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
        imports: &[ImportSymbol],
        import_info: Option<&ImportTableResult>,
        base_relocs: &[u32],
    ) -> LinkResult<()> {
        let bytes = Self::encode_executable(
            target,
            entry_va,
            merged_sections,
            symbols,
            imports,
            import_info,
            base_relocs,
            false,
        )?;
        fs::write(path, bytes)?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn encode_executable(
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        _symbols: &[Symbol],
        imports: &[ImportSymbol],
        import_info: Option<&ImportTableResult>,
        base_relocs: &[u32],
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
        //
        // When the layout engine assigned virtual addresses (the normal link
        // path), those VAs are authoritative: every relocation and thunk was
        // patched against them, so the writer must place sections at exactly
        // those RVAs instead of re-deriving its own layout.
        let has_layout_vas = merged_sections.iter().any(|s| s.virtual_address != 0);

        struct OutSection {
            name: String,
            flags: u32,
            data: Vec<u8>,
            size: u64,
            /// RVA dictated by the layout engine, if any.
            layout_rva: Option<u32>,
        }

        let mut pe_sections: Vec<OutSection> = Vec::new();
        let mut idata_idx = None;
        for (idx, sec) in merged_sections.iter().enumerate() {
            if sec.name == ".idata" {
                idata_idx = Some(idx);
            }
            let layout_rva = if has_layout_vas && sec.virtual_address != 0 {
                if sec.virtual_address < image_base {
                    return Err(LinkError::new(
                        ErrorCode::InvalidSection,
                        format!(
                            "section `{}` virtual address 0x{:x} is below the image base 0x{:x}",
                            sec.name, sec.virtual_address, image_base
                        ),
                    ));
                }
                Some((sec.virtual_address - image_base) as u32)
            } else {
                None
            };
            pe_sections.push(OutSection {
                name: sec.name.clone(),
                flags: sec.flags,
                data: sec.data.clone(),
                size: sec.size,
                layout_rva,
            });
        }

        // Compute header size first (needed to validate/place section RVAs).
        let num_sections_base = pe_sections.len();
        let extra_sections = 2; // potential .idata + .reloc
        let opt_hdr_size = 240u16; // PE32+ Optional Header size
        let headers_unaligned_max = (lfanew as usize)
            + 4
            + 20
            + (opt_hdr_size as usize)
            + ((num_sections_base + extra_sections) * 40);
        let headers_size = align_to(headers_unaligned_max as u64, file_alignment as u64) as u32;

        // Standalone path only: synthesize .idata when the layout engine did
        // not provide one (e.g. the gen_windows_exe helper). The real link
        // path always carries a prebuilt .idata + ImportTableResult.
        let mut generated_imp: Option<ImportTableResult> = None;
        if idata_idx.is_none() && !imports.is_empty() {
            let next_rva = pe_sections
                .iter()
                .map(|s| {
                    let rva = s.layout_rva.unwrap_or(0);
                    rva + align_to(s.size.max(s.data.len() as u64), section_alignment as u64) as u32
                })
                .max()
                .unwrap_or(align_to(headers_size as u64, section_alignment as u64) as u32);
            let idata_rva = align_to(next_rva as u64, section_alignment as u64) as u32;
            let res = build_import_table(imports, image_base, idata_rva);
            let data_len = res.data.len() as u64;
            pe_sections.push(OutSection {
                name: ".idata".to_string(),
                flags: crate::section::flags::READ
                    | crate::section::flags::WRITE
                    | crate::section::flags::ALLOC,
                data: res.data.clone(),
                size: data_len,
                layout_rva: Some(idata_rva),
            });
            idata_idx = Some(pe_sections.len() - 1);
            generated_imp = Some(res);
        }

        let imp_res: Option<&ImportTableResult> = import_info.or(generated_imp.as_ref());

        // If the layout engine built the import table, the .idata section must
        // land at exactly the RVA the table was built for.
        if let (Some(info), Some(id_idx)) = (import_info, idata_idx) {
            let actual = pe_sections[id_idx].layout_rva.unwrap_or(0);
            if actual != info.idata_rva {
                return Err(LinkError::new(
                    ErrorCode::InvalidSection,
                    format!(
                        "internal layout inconsistency: .idata section RVA 0x{:x} does not match \
                         the RVA 0x{:x} the import table was built for; import thunks would be \
                         corrupted, refusing to emit a broken image",
                        actual, info.idata_rva
                    ),
                ));
            }
        }

        // Add .reloc section (real base relocations when ASLR rebases the image).
        let reloc_data = build_base_reloc_table(base_relocs, true);
        let next_rva = pe_sections
            .iter()
            .map(|s| {
                let rva = s.layout_rva.unwrap_or(0);
                rva + align_to(s.size.max(s.data.len() as u64), section_alignment as u64) as u32
            })
            .max()
            .unwrap_or(align_to(headers_size as u64, section_alignment as u64) as u32);
        let reloc_rva = align_to(next_rva as u64, section_alignment as u64) as u32;
        let reloc_size_val = reloc_data.len() as u64;
        let reloc_idx = pe_sections.len();
        pe_sections.push(OutSection {
            name: ".reloc".to_string(),
            flags: crate::section::flags::READ
                | crate::section::flags::ALLOC
                | crate::section::flags::DISCARD,
            data: reloc_data,
            size: reloc_size_val,
            layout_rva: Some(reloc_rva),
        });

        let num_sections = pe_sections.len() as u16;

        // 3. Assign final RVAs and file offsets
        let mut current_seq_rva = align_to(headers_size as u64, section_alignment as u64) as u32;
        let mut current_file_offset = headers_size;

        let mut section_headers = Vec::with_capacity(num_sections as usize);
        let mut size_of_code = 0u32;
        let mut size_of_init_data = 0u32;
        let mut base_of_code = 0u32;
        let mut prev_rva = 0u32;

        output.resize(headers_size as usize, 0);

        for sec in &pe_sections {
            let virt_size = sec.size.max(sec.data.len() as u64) as u32;
            let raw_size = align_to(sec.data.len() as u64, file_alignment as u64) as u32;

            let rva = match sec.layout_rva {
                Some(r) => r,
                None => {
                    let r = current_seq_rva;
                    current_seq_rva =
                        align_to((r + virt_size) as u64, section_alignment as u64) as u32;
                    r
                }
            };

            if rva < headers_size
                || (!section_headers.is_empty() && rva < prev_rva)
                || (!section_headers.is_empty() && rva == prev_rva && virt_size > 0)
            {
                return Err(LinkError::new(
                    ErrorCode::InvalidSection,
                    format!(
                        "section `{}` RVA 0x{:x} overlaps a preceding section or the PE headers \
                         (headers end at 0x{:x}, previous RVA 0x{:x})",
                        sec.name, rva, headers_size, prev_rva
                    ),
                ));
            }

            let mut name_buf = [0u8; 8];
            let name_bytes = sec.name.as_bytes();
            let copy_len = name_bytes.len().min(8);
            name_buf[0..copy_len].copy_from_slice(&name_bytes[0..copy_len]);

            let characteristics = if sec.name == ".reloc" {
                IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_DISCARDABLE
            } else if sec.name == ".idata" {
                IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE
            } else {
                let mut c = 0u32;
                if (sec.flags & crate::section::flags::READ) != 0 {
                    c |= IMAGE_SCN_MEM_READ;
                }
                if (sec.flags & crate::section::flags::WRITE) != 0 {
                    c |= IMAGE_SCN_MEM_WRITE;
                }
                if (sec.flags & crate::section::flags::EXEC) != 0 {
                    c |= IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_CNT_CODE;
                    if base_of_code == 0 {
                        base_of_code = rva;
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
                virtual_address: rva,
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
                output.extend_from_slice(&sec.data);
                let padded_len = (current_file_offset + raw_size) as usize;
                if output.len() < padded_len {
                    output.resize(padded_len, 0);
                }
                current_file_offset += raw_size;
            }

            prev_rva = rva;
            current_seq_rva =
                align_to((rva + virt_size) as u64, section_alignment as u64) as u32;
        }

        let size_of_image = current_seq_rva;

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
        // ASLR (DYNAMIC_BASE + HIGH_ENTROPY_VA) is sound here because the
        // layout engine recorded every 64-bit absolute address in .reloc.
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
        if let (Some(info), Some(id_idx)) = (imp_res, idata_idx) {
            let idata_rva = section_headers[id_idx].virtual_address;
            let imp_dir_off = 112 + IMAGE_DIRECTORY_ENTRY_IMPORT * 8;
            opt_buf[imp_dir_off..imp_dir_off + 4].copy_from_slice(&idata_rva.to_le_bytes());
            opt_buf[imp_dir_off + 4..imp_dir_off + 8]
                .copy_from_slice(&info.import_descriptor_size.to_le_bytes());

            if info.iat_rva > 0 {
                let iat_dir_off = 112 + IMAGE_DIRECTORY_ENTRY_IAT * 8;
                opt_buf[iat_dir_off..iat_dir_off + 4]
                    .copy_from_slice(&info.iat_rva.to_le_bytes());
                opt_buf[iat_dir_off + 4..iat_dir_off + 8]
                    .copy_from_slice(&info.iat_size.to_le_bytes());
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
