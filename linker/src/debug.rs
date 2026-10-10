//! Debug information handling, DWARF 5 synthesis, CodeView emission, and stripping.

use crate::object::ObjectFile;
use crate::section::Section;

pub struct DebugProcessor;

impl DebugProcessor {
    pub fn strip_debug_sections(objects: &mut [ObjectFile], strip_all: bool, strip_debug: bool) {
        if !strip_all && !strip_debug {
            return;
        }

        for obj in objects.iter_mut() {
            obj.sections.retain(|sec| {
                if strip_all {
                    !sec.name.starts_with(".debug") && !sec.name.starts_with(".comment")
                } else if strip_debug {
                    !sec.name.starts_with(".debug")
                } else {
                    true
                }
            });
        }
    }
}

/// DWARF 5 standard debug section generator.
pub struct Dwarf5Generator;

impl Dwarf5Generator {
    /// Synthesizes `.debug_str` section containing null-terminated string table.
    pub fn synthesize_debug_str(strings: &[&str]) -> Section {
        let mut bytes = Vec::new();
        for s in strings {
            bytes.extend_from_slice(s.as_bytes());
            bytes.push(0);
        }
        Section::new_debug(".debug_str", bytes, 1)
    }

    /// Synthesizes DWARF 5 `.debug_abbrev` table supporting compile units and subprograms.
    pub fn synthesize_debug_abbrev() -> Section {
        let mut bytes = Vec::new();
        // Entry 1: DW_TAG_compile_unit (0x11), children: DW_CHILDREN_yes (0x01)
        bytes.push(1); // Abbrev code 1
        bytes.push(0x11); // DW_TAG_compile_unit
        bytes.push(0x01); // DW_CHILDREN_yes
        // Attribute: DW_AT_name (0x03), form: DW_FORM_strp (0x0e)
        bytes.push(0x03);
        bytes.push(0x0e);
        // Attribute: DW_AT_producer (0x25), form: DW_FORM_strp (0x0e)
        bytes.push(0x25);
        bytes.push(0x0e);
        // Attribute: DW_AT_low_pc (0x11), form: DW_FORM_addr (0x01)
        bytes.push(0x11);
        bytes.push(0x01);
        // Attribute: DW_AT_high_pc (0x12), form: DW_FORM_data8 (0x0b)
        bytes.push(0x12);
        bytes.push(0x0b);
        // Attribute: DW_AT_stmt_list (0x10), form: DW_FORM_sec_offset (0x17)
        bytes.push(0x10);
        bytes.push(0x17);
        // End of attributes (0, 0)
        bytes.push(0);
        bytes.push(0);

        // Entry 2: DW_TAG_subprogram (0x2e), children: DW_CHILDREN_no (0x00)
        bytes.push(2); // Abbrev code 2
        bytes.push(0x2e); // DW_TAG_subprogram
        bytes.push(0x00); // DW_CHILDREN_no
        // Attribute: DW_AT_name (0x03), form: DW_FORM_strp (0x0e)
        bytes.push(0x03);
        bytes.push(0x0e);
        // Attribute: DW_AT_low_pc (0x11), form: DW_FORM_addr (0x01)
        bytes.push(0x11);
        bytes.push(0x01);
        // Attribute: DW_AT_high_pc (0x12), form: DW_FORM_data8 (0x0b)
        bytes.push(0x12);
        bytes.push(0x0b);
        // End of attributes (0, 0)
        bytes.push(0);
        bytes.push(0);

        // Terminating null byte for abbreviation table
        bytes.push(0);

        Section::new_debug(".debug_abbrev", bytes, 1)
    }

    /// Synthesizes DWARF 5 `.debug_info` compilation unit header, root DIE, and subprogram DIEs.
    pub fn synthesize_debug_info(
        unit_name_str_offset: u32,
        producer_str_offset: u32,
        low_pc: u64,
        high_pc_size: u64,
    ) -> Section {
        Self::synthesize_debug_info_with_functions(
            unit_name_str_offset,
            producer_str_offset,
            low_pc,
            high_pc_size,
            0,
            &[],
        )
    }

    /// Synthesizes DWARF 5 `.debug_info` with compilation unit and multiple function subprograms.
    pub fn synthesize_debug_info_with_functions(
        unit_name_str_offset: u32,
        producer_str_offset: u32,
        low_pc: u64,
        high_pc_size: u64,
        stmt_list_offset: u32,
        functions: &[(u32, u64, u64)], // (name_str_offset, low_pc, size)
    ) -> Section {
        let mut bytes = Vec::new();
        // Placeholder for unit_length (4B)
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // DWARF version: 5 (2B)
        bytes.extend_from_slice(&5u16.to_le_bytes());
        // Unit type: DW_UT_compile (1) (1B)
        bytes.push(1);
        // Address size: 8 bytes (1B)
        bytes.push(8);
        // debug_abbrev_offset: 0 (4B)
        bytes.extend_from_slice(&0u32.to_le_bytes());

        // Root DIE: Abbrev code 1 (DW_TAG_compile_unit)
        bytes.push(1);
        // DW_AT_name (DW_FORM_strp: 4B offset)
        bytes.extend_from_slice(&unit_name_str_offset.to_le_bytes());
        // DW_AT_producer (DW_FORM_strp: 4B offset)
        bytes.extend_from_slice(&producer_str_offset.to_le_bytes());
        // DW_AT_low_pc (DW_FORM_addr: 8B)
        bytes.extend_from_slice(&low_pc.to_le_bytes());
        // DW_AT_high_pc (DW_FORM_data8: 8B)
        bytes.extend_from_slice(&high_pc_size.to_le_bytes());
        // DW_AT_stmt_list (DW_FORM_sec_offset: 4B)
        bytes.extend_from_slice(&stmt_list_offset.to_le_bytes());

        // Child subprogram DIEs (Abbrev code 2)
        for &(fn_name_off, fn_low_pc, fn_size) in functions {
            bytes.push(2); // Abbrev code 2
            bytes.extend_from_slice(&fn_name_off.to_le_bytes());
            bytes.extend_from_slice(&fn_low_pc.to_le_bytes());
            bytes.extend_from_slice(&fn_size.to_le_bytes());
        }

        // Terminating null byte (end of compile_unit children)
        bytes.push(0);

        // Update unit_length (length of following bytes)
        let length = (bytes.len() - 4) as u32;
        bytes[0..4].copy_from_slice(&length.to_le_bytes());

        Section::new_debug(".debug_info", bytes, 4)
    }

    /// Synthesizes standard DWARF 5 `.debug_line` state machine program.
    pub fn synthesize_debug_line(
        directories: &[&str],
        files: &[(&str, u32)],
        mut line_entries: Vec<(u64, u32)>,
    ) -> Section {
        let mut bytes = Vec::new();

        // 1. unit_length placeholder (4B)
        bytes.extend_from_slice(&0u32.to_le_bytes());
        // 2. version: 5 (2B)
        bytes.extend_from_slice(&5u16.to_le_bytes());
        // 3. address_size: 8 (1B)
        bytes.push(8);
        // 4. segment_selector_size: 0 (1B)
        bytes.push(0);

        // 5. header_length placeholder (4B)
        let header_len_offset = bytes.len();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let header_start = bytes.len();

        // 6. Line table configuration parameters
        bytes.push(1); // minimum_instruction_length
        bytes.push(1); // maximum_operations_per_instruction
        bytes.push(1); // default_is_stmt
        bytes.push((-5i8) as u8); // line_base
        bytes.push(14); // line_range
        bytes.push(13); // opcode_base (standard opcodes 1..12)

        // 7. Standard opcode lengths (12 standard opcodes)
        let std_opcode_lens = [0u8, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0, 1];
        bytes.extend_from_slice(&std_opcode_lens);

        // 8. Directory table format (DWARF 5)
        bytes.push(1); // directory_entry_format_count
        encode_uleb128(0x01, &mut bytes); // DW_LNCT_path
        encode_uleb128(0x08, &mut bytes); // DW_FORM_string

        encode_uleb128(directories.len() as u64, &mut bytes); // directories count
        for dir in directories {
            bytes.extend_from_slice(dir.as_bytes());
            bytes.push(0);
        }

        // 9. File name table format (DWARF 5)
        bytes.push(2); // file_name_entry_format_count
        encode_uleb128(0x01, &mut bytes); // DW_LNCT_path
        encode_uleb128(0x08, &mut bytes); // DW_FORM_string
        encode_uleb128(0x02, &mut bytes); // DW_LNCT_directory_index
        encode_uleb128(0x0f, &mut bytes); // DW_FORM_udata

        encode_uleb128(files.len() as u64, &mut bytes); // files count
        for (file_name, dir_idx) in files {
            bytes.extend_from_slice(file_name.as_bytes());
            bytes.push(0);
            encode_uleb128(*dir_idx as u64, &mut bytes);
        }

        // Patch header_length
        let header_len = (bytes.len() - header_start) as u32;
        bytes[header_len_offset..header_len_offset + 4].copy_from_slice(&header_len.to_le_bytes());

        // 10. Line number program opcode stream
        line_entries.sort_by_key(|&(va, _)| va);

        let mut current_va = 0u64;
        let mut current_line = 1i64;

        for (va, line) in line_entries {
            if current_va == 0 {
                // DW_LNE_set_address (extended opcode: 0x00, len: 9, opcode: 0x02)
                bytes.push(0);
                encode_uleb128(9, &mut bytes);
                bytes.push(0x02);
                bytes.extend_from_slice(&va.to_le_bytes());
                current_va = va;
            } else if va > current_va {
                let pc_delta = va - current_va;
                // DW_LNS_advance_pc
                bytes.push(0x02);
                encode_uleb128(pc_delta, &mut bytes);
                current_va = va;
            }

            let line_delta = (line as i64) - current_line;
            if line_delta != 0 {
                // DW_LNS_advance_line
                bytes.push(0x03);
                encode_sleb128(line_delta, &mut bytes);
                current_line = line as i64;
            }

            // DW_LNS_copy
            bytes.push(0x01);
        }

        // DW_LNE_end_sequence (extended opcode: 0x00, len: 1, opcode: 0x01)
        bytes.push(0);
        encode_uleb128(1, &mut bytes);
        bytes.push(0x01);

        // Patch unit_length
        let unit_len = (bytes.len() - 4) as u32;
        bytes[0..4].copy_from_slice(&unit_len.to_le_bytes());

        Section::new_debug(".debug_line", bytes, 8)
    }
}

fn encode_uleb128(mut val: u64, out: &mut Vec<u8>) {
    loop {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        if val != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if val == 0 {
            break;
        }
    }
}

fn encode_sleb128(mut val: i64, out: &mut Vec<u8>) {
    let mut more = true;
    while more {
        let mut byte = (val & 0x7F) as u8;
        val >>= 7;
        let sign_bit = (byte & 0x40) != 0;
        if (val == 0 && !sign_bit) || (val == -1 && sign_bit) {
            more = false;
        } else {
            byte |= 0x80;
        }
        out.push(byte);
    }
}

/// Microsoft CodeView / PDB Debug Section Generator for Windows PE.
pub struct CodeViewGenerator;

impl CodeViewGenerator {
    /// Synthesizes CodeView `.debug$S` (symbol stream) and `.debug$T` (type stream).
    pub fn synthesize_codeview_symbols(module_name: &str, compiler_ver: &str) -> Section {
        let mut bytes = Vec::new();
        // CodeView signature: CV_SIGNATURE_C13 (4)
        bytes.extend_from_slice(&4u32.to_le_bytes());

        // S_OBJNAME symbol record
        let objname_sym_len = (module_name.len() + 9) as u16;
        bytes.extend_from_slice(&objname_sym_len.to_le_bytes());
        bytes.extend_from_slice(&0x1101u16.to_le_bytes()); // S_OBJNAME
        bytes.extend_from_slice(&0u32.to_le_bytes()); // signature
        bytes.extend_from_slice(module_name.as_bytes());
        bytes.push(0); // null terminator

        // S_COMPILE3 symbol record
        let comp_bytes = compiler_ver.as_bytes();
        let comp_sym_len = (comp_bytes.len() + 35) as u16;
        bytes.extend_from_slice(&comp_sym_len.to_le_bytes());
        bytes.extend_from_slice(&0x113cu16.to_le_bytes()); // S_COMPILE3
        bytes.extend_from_slice(&0x000000D0u32.to_le_bytes()); // flags (64-bit x64, optimized)
        bytes.extend_from_slice(&0x00D0u16.to_le_bytes()); // machine: AMD64
        bytes.extend_from_slice(&1u16.to_le_bytes()); // frontend major
        bytes.extend_from_slice(&0u16.to_le_bytes()); // frontend minor
        bytes.extend_from_slice(&0u16.to_le_bytes()); // frontend build
        bytes.extend_from_slice(&1u16.to_le_bytes()); // backend major
        bytes.extend_from_slice(&0u16.to_le_bytes()); // backend minor
        bytes.extend_from_slice(&0u16.to_le_bytes()); // backend build
        bytes.extend_from_slice(comp_bytes);
        bytes.push(0);

        Section::new_debug(".debug$S", bytes, 4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dwarf5_debug_line_synthesis() {
        let directories = vec!["/src", "/src/math"];
        let files = vec![("main.adesh", 0), ("calc.adesh", 1)];
        let lines = vec![(0x1000, 10), (0x1015, 12), (0x1030, 25)];

        let sec = Dwarf5Generator::synthesize_debug_line(&directories, &files, lines);
        assert_eq!(sec.name, ".debug_line");
        assert!(sec.data.len() > 30);

        // Version check (offset 4..6 is version 5)
        let version = u16::from_le_bytes([sec.data[4], sec.data[5]]);
        assert_eq!(version, 5);

        // Address size is 8
        assert_eq!(sec.data[6], 8);
    }

    #[test]
    fn test_dwarf5_debug_info_synthesis() {
        let sec = Dwarf5Generator::synthesize_debug_info(0, 16, 0x1000, 0x500);
        assert_eq!(sec.name, ".debug_info");
        let version = u16::from_le_bytes([sec.data[4], sec.data[5]]);
        assert_eq!(version, 5);
    }

    #[test]
    fn test_dwarf5_debug_info_with_functions_synthesis() {
        let abbrev_sec = Dwarf5Generator::synthesize_debug_abbrev();
        assert_eq!(abbrev_sec.name, ".debug_abbrev");
        assert!(abbrev_sec.data.len() > 10);

        let funcs = vec![(32, 0x1000, 0x80), (48, 0x1080, 0x120)];
        let info_sec =
            Dwarf5Generator::synthesize_debug_info_with_functions(0, 16, 0x1000, 0x1A0, 0, &funcs);
        assert_eq!(info_sec.name, ".debug_info");
        let version = u16::from_le_bytes([info_sec.data[4], info_sec.data[5]]);
        assert_eq!(version, 5);
    }
}
