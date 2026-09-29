//! Mach-O 64-bit Executable Writer.

use crate::error::LinkResult;
use crate::macho::header::*;
use crate::section::{align_to, MergedSection, SectionKind};
use crate::symbol::Symbol;
use crate::target::{Arch, Target};
use std::fs;
use std::path::Path;

pub struct MachOWriter;

impl MachOWriter {
    pub fn write_executable(
        path: &Path,
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
    ) -> LinkResult<()> {
        let bytes = Self::encode_executable(target, entry_va, merged_sections, symbols)?;
        fs::write(path, bytes)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(meta) = fs::metadata(path) {
                let mut perms = meta.permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(path, perms);
            }
        }

        Ok(())
    }

    pub fn encode_executable(
        target: &Target,
        entry_va: u64,
        merged_sections: &[MergedSection],
        symbols: &[Symbol],
    ) -> LinkResult<Vec<u8>> {
        let cputype = if target.arch == Arch::Arm || target.arch == Arch::AArch64 {
            CPU_TYPE_ARM64
        } else {
            CPU_TYPE_X86_64
        };

        let pagezero_size = 0x100000000u64; // 4GB pagezero
        let text_vmaddr = pagezero_size;

        let mut output = Vec::new();

        // 1. Calculate Load Commands Size
        // Commands: LC_SEGMENT_64 (__PAGEZERO), LC_SEGMENT_64 (__TEXT with sections), LC_SEGMENT_64 (__DATA), LC_SEGMENT_64 (__LINKEDIT), LC_MAIN, LC_SYMTAB, LC_LOAD_DYLINKER
        let ncmds = 7u32;
        let mut text_sections = Vec::new();
        let mut data_sections = Vec::new();

        for sec in merged_sections {
            if sec.is_executable() || (!sec.is_writable() && sec.kind != SectionKind::Bss) {
                text_sections.push(sec.clone());
            } else {
                data_sections.push(sec.clone());
            }
        }

        let cmd_pagezero_sz = 72usize;
        let cmd_text_sz = 72 + text_sections.len() * 80;
        let cmd_data_sz = 72 + data_sections.len() * 80;
        let cmd_linkedit_sz = 72usize;
        let cmd_main_sz = 24usize;
        let cmd_symtab_sz = 24usize;
        let dylinker_str = b"/usr/lib/dyld\0\0\0"; // padded to 16 bytes
        let cmd_dylinker_sz = 12 + dylinker_str.len();

        let sizeofcmds = (cmd_pagezero_sz + cmd_text_sz + cmd_data_sz + cmd_linkedit_sz + cmd_main_sz + cmd_symtab_sz + cmd_dylinker_sz) as u32;
        let headers_size = 32 + (sizeofcmds as usize);
        let text_file_offset_start = 0u64; // __TEXT starts at file offset 0 including headers

        output.resize(headers_size, 0);

        // 2. Layout __TEXT segment payload
        let mut current_file_offset = headers_size as u64;
        let mut current_vmaddr = text_vmaddr + current_file_offset;

        let mut text_sec_records = Vec::new();
        for sec in &text_sections {
            let aligned_off = align_to(current_file_offset, sec.alignment);
            let aligned_va = align_to(current_vmaddr, sec.alignment);
            if aligned_off > current_file_offset {
                let pad = (aligned_off - current_file_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_file_offset = aligned_off;
                current_vmaddr = aligned_va;
            }

            text_sec_records.push((sec.name.clone(), aligned_va, sec.data.len() as u64, aligned_off as u32, sec.alignment));
            output.extend_from_slice(&sec.data);
            current_file_offset += sec.data.len() as u64;
            current_vmaddr += sec.data.len() as u64;
        }

        let text_filesize = current_file_offset;
        let text_vmsize = align_to(current_vmaddr - text_vmaddr, target.page_size);

        // 3. Layout __DATA segment payload
        let data_file_offset_start = align_to(current_file_offset, target.page_size);
        let data_vmaddr_start = text_vmaddr + text_vmsize;

        if data_file_offset_start > current_file_offset {
            let pad = (data_file_offset_start - current_file_offset) as usize;
            output.resize(output.len() + pad, 0);
            current_file_offset = data_file_offset_start;
            current_vmaddr = data_vmaddr_start;
        }

        let mut data_sec_records = Vec::new();
        for sec in &data_sections {
            let aligned_off = align_to(current_file_offset, sec.alignment);
            let aligned_va = align_to(current_vmaddr, sec.alignment);
            if aligned_off > current_file_offset {
                let pad = (aligned_off - current_file_offset) as usize;
                output.resize(output.len() + pad, 0);
                current_file_offset = aligned_off;
                current_vmaddr = aligned_va;
            }

            data_sec_records.push((sec.name.clone(), aligned_va, sec.size, if sec.kind == SectionKind::Bss { 0 } else { aligned_off as u32 }, sec.alignment));
            if sec.kind != SectionKind::Bss {
                output.extend_from_slice(&sec.data);
                current_file_offset += sec.data.len() as u64;
            }
            current_vmaddr += sec.size;
        }

        let data_filesize = current_file_offset - data_file_offset_start;
        let data_vmsize = align_to(current_vmaddr - data_vmaddr_start, target.page_size);

        // 4. Layout __LINKEDIT (Symbols and Strings)
        let linkedit_file_offset = align_to(current_file_offset, target.page_size);
        let linkedit_vmaddr = data_vmaddr_start + data_vmsize;

        if linkedit_file_offset > current_file_offset {
            let pad = (linkedit_file_offset - current_file_offset) as usize;
            output.resize(output.len() + pad, 0);
        }

        let mut strtab = vec![0u8, 0u8, 0u8, 0u8]; // start with 4-byte padding
        let mut nlists = Vec::new();

        for sym in symbols {
            if !sym.name.is_empty() {
                let n_strx = strtab.len() as u32;
                strtab.extend_from_slice(sym.name.as_bytes());
                strtab.push(0);

                let n_type = if sym.is_defined { 0x0E } else { 0x01 }; // N_SECT | N_EXT vs N_UNDF | N_EXT
                let n_sect = if sym.is_defined { 1 } else { 0 };

                nlists.push(Nlist64 {
                    n_strx,
                    n_type,
                    n_sect,
                    n_desc: 0,
                    n_value: sym.value,
                });
            }
        }

        let symoff = output.len() as u32;
        for nl in &nlists {
            output.extend_from_slice(&nl.n_strx.to_le_bytes());
            output.push(nl.n_type);
            output.push(nl.n_sect);
            output.extend_from_slice(&nl.n_desc.to_le_bytes());
            output.extend_from_slice(&nl.n_value.to_le_bytes());
        }

        let stroff = output.len() as u32;
        let strsize = strtab.len() as u32;
        output.extend_from_slice(&strtab);

        let linkedit_filesize = (output.len() as u64) - linkedit_file_offset;
        let linkedit_vmsize = align_to(linkedit_filesize, target.page_size);

        // 5. Fill Mach-O Header at offset 0 (32 bytes)
        let hdr = MachHeader64 {
            magic: MH_MAGIC_64,
            cputype,
            cpusubtype: CPU_SUBTYPE_LIB64,
            filetype: MH_EXECUTE,
            ncmds,
            sizeofcmds,
            flags: MH_NOUNDEFS | MH_DYLDLINK | MH_TWOLEVEL | MH_PIE,
            reserved: 0,
        };

        let mut hdr_buf = Vec::with_capacity(32);
        hdr_buf.extend_from_slice(&hdr.magic.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.cputype.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.cpusubtype.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.filetype.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.ncmds.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.sizeofcmds.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.flags.to_le_bytes());
        hdr_buf.extend_from_slice(&hdr.reserved.to_le_bytes());
        output[0..32].copy_from_slice(&hdr_buf);

        // 6. Write Load Commands
        let mut cmd_buf = Vec::new();

        // LC_SEGMENT_64: __PAGEZERO
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_pagezero_sz as u32).to_le_bytes());
        let mut pz_name = [0u8; 16];
        pz_name[0..10].copy_from_slice(b"__PAGEZERO");
        cmd_buf.extend_from_slice(&pz_name);
        cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // vmaddr = 0
        cmd_buf.extend_from_slice(&pagezero_size.to_le_bytes()); // vmsize = 4GB
        cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // fileoff = 0
        cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // filesize = 0
        cmd_buf.extend_from_slice(&VM_PROT_NONE.to_le_bytes()); // maxprot = 0
        cmd_buf.extend_from_slice(&VM_PROT_NONE.to_le_bytes()); // initprot = 0
        cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // nsects = 0
        cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // flags = 0

        // LC_SEGMENT_64: __TEXT
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_text_sz as u32).to_le_bytes());
        let mut text_name = [0u8; 16];
        text_name[0..6].copy_from_slice(b"__TEXT");
        cmd_buf.extend_from_slice(&text_name);
        cmd_buf.extend_from_slice(&text_vmaddr.to_le_bytes());
        cmd_buf.extend_from_slice(&text_vmsize.to_le_bytes());
        cmd_buf.extend_from_slice(&text_file_offset_start.to_le_bytes());
        cmd_buf.extend_from_slice(&text_filesize.to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_EXECUTE).to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_EXECUTE).to_le_bytes());
        cmd_buf.extend_from_slice(&(text_sec_records.len() as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());

        for (name, va, sz, off, align) in &text_sec_records {
            let mut sname = [0u8; 16];
            let name_bytes = name.as_bytes();
            sname[0..name_bytes.len().min(16)].copy_from_slice(&name_bytes[0..name_bytes.len().min(16)]);
            cmd_buf.extend_from_slice(&sname);
            cmd_buf.extend_from_slice(&text_name);
            cmd_buf.extend_from_slice(&va.to_le_bytes());
            cmd_buf.extend_from_slice(&sz.to_le_bytes());
            cmd_buf.extend_from_slice(&off.to_le_bytes());
            cmd_buf.extend_from_slice(&((*align as f64).log2() as u32).to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // reloff
            cmd_buf.extend_from_slice(&0u32.to_le_bytes()); // nreloc
            cmd_buf.extend_from_slice(&0x80000400u32.to_le_bytes()); // flags: S_ATTR_SOME_INSTRUCTIONS | S_ATTR_PURE_INSTRUCTIONS
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
        }

        // LC_SEGMENT_64: __DATA
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_data_sz as u32).to_le_bytes());
        let mut data_name = [0u8; 16];
        data_name[0..6].copy_from_slice(b"__DATA");
        cmd_buf.extend_from_slice(&data_name);
        cmd_buf.extend_from_slice(&data_vmaddr_start.to_le_bytes());
        cmd_buf.extend_from_slice(&data_vmsize.to_le_bytes());
        cmd_buf.extend_from_slice(&data_file_offset_start.to_le_bytes());
        cmd_buf.extend_from_slice(&data_filesize.to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_WRITE).to_le_bytes());
        cmd_buf.extend_from_slice(&(VM_PROT_READ | VM_PROT_WRITE).to_le_bytes());
        cmd_buf.extend_from_slice(&(data_sec_records.len() as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());

        for (name, va, sz, off, align) in &data_sec_records {
            let mut sname = [0u8; 16];
            let name_bytes = name.as_bytes();
            sname[0..name_bytes.len().min(16)].copy_from_slice(&name_bytes[0..name_bytes.len().min(16)]);
            cmd_buf.extend_from_slice(&sname);
            cmd_buf.extend_from_slice(&data_name);
            cmd_buf.extend_from_slice(&va.to_le_bytes());
            cmd_buf.extend_from_slice(&sz.to_le_bytes());
            cmd_buf.extend_from_slice(&off.to_le_bytes());
            cmd_buf.extend_from_slice(&((*align as f64).log2() as u32).to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
            cmd_buf.extend_from_slice(&0u32.to_le_bytes());
        }

        // LC_SEGMENT_64: __LINKEDIT
        cmd_buf.extend_from_slice(&LC_SEGMENT_64.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_linkedit_sz as u32).to_le_bytes());
        let mut linkedit_name = [0u8; 16];
        linkedit_name[0..10].copy_from_slice(b"__LINKEDIT");
        cmd_buf.extend_from_slice(&linkedit_name);
        cmd_buf.extend_from_slice(&linkedit_vmaddr.to_le_bytes());
        cmd_buf.extend_from_slice(&linkedit_vmsize.to_le_bytes());
        cmd_buf.extend_from_slice(&linkedit_file_offset.to_le_bytes());
        cmd_buf.extend_from_slice(&linkedit_filesize.to_le_bytes());
        cmd_buf.extend_from_slice(&VM_PROT_READ.to_le_bytes());
        cmd_buf.extend_from_slice(&VM_PROT_READ.to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());
        cmd_buf.extend_from_slice(&0u32.to_le_bytes());

        // LC_MAIN
        let entryoff = if entry_va >= text_vmaddr {
            entry_va - text_vmaddr
        } else {
            headers_size as u64
        };
        cmd_buf.extend_from_slice(&LC_MAIN.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_main_sz as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&entryoff.to_le_bytes());
        cmd_buf.extend_from_slice(&0u64.to_le_bytes()); // stacksize default

        // LC_SYMTAB
        cmd_buf.extend_from_slice(&LC_SYMTAB.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_symtab_sz as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&symoff.to_le_bytes());
        cmd_buf.extend_from_slice(&(nlists.len() as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&stroff.to_le_bytes());
        cmd_buf.extend_from_slice(&strsize.to_le_bytes());

        // LC_LOAD_DYLINKER
        cmd_buf.extend_from_slice(&LC_LOAD_DYLINKER.to_le_bytes());
        cmd_buf.extend_from_slice(&(cmd_dylinker_sz as u32).to_le_bytes());
        cmd_buf.extend_from_slice(&12u32.to_le_bytes()); // offset to string
        cmd_buf.extend_from_slice(dylinker_str);

        output[32..32 + cmd_buf.len()].copy_from_slice(&cmd_buf);

        Ok(output)
    }
}
