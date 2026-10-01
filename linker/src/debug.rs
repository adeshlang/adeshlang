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

    /// Synthesizes minimal DWARF 5 `.debug_abbrev` table.
    pub fn synthesize_debug_abbrev() -> Section {
        let mut bytes = Vec::new();
        // Entry 1: DW_TAG_compile_unit (0x11), children: DW_CHILDREN_yes (0x01)
        bytes.push(1); // Abbrev code
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
        // End of attributes (0, 0)
        bytes.push(0);
        bytes.push(0);

        // Terminating null byte for abbreviation table
        bytes.push(0);

        Section::new_debug(".debug_abbrev", bytes, 1)
    }

    /// Synthesizes DWARF 5 `.debug_info` compilation unit header and root DIE.
    pub fn synthesize_debug_info(
        unit_name_str_offset: u32,
        producer_str_offset: u32,
        low_pc: u64,
        high_pc_size: u64,
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

        // Root DIE: Abbrev code 1
        bytes.push(1);
        // DW_AT_name (DW_FORM_strp: 4B offset)
        bytes.extend_from_slice(&unit_name_str_offset.to_le_bytes());
        // DW_AT_producer (DW_FORM_strp: 4B offset)
        bytes.extend_from_slice(&producer_str_offset.to_le_bytes());
        // DW_AT_low_pc (DW_FORM_addr: 8B)
        bytes.extend_from_slice(&low_pc.to_le_bytes());
        // DW_AT_high_pc (DW_FORM_data8: 8B)
        bytes.extend_from_slice(&high_pc_size.to_le_bytes());

        // Terminating null byte (end of children)
        bytes.push(0);

        // Update unit_length (length of following bytes)
        let length = (bytes.len() - 4) as u32;
        bytes[0..4].copy_from_slice(&length.to_le_bytes());

        Section::new_debug(".debug_info", bytes, 4)
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
