//! PE Base Relocations table generator (.reloc).

pub const IMAGE_REL_BASED_ABSOLUTE: u16 = 0;
pub const IMAGE_REL_BASED_HIGHLOW: u16 = 3;
pub const IMAGE_REL_BASED_DIR64: u16 = 10;

/// Construct a `.reloc` base relocations section for Windows ASLR.
pub fn build_base_reloc_table(reloc_rvas: &[u32], is_64bit: bool) -> Vec<u8> {
    if reloc_rvas.is_empty() {
        let mut empty_reloc = Vec::with_capacity(8);
        empty_reloc.extend_from_slice(&0u32.to_le_bytes()); // Page RVA: 0
        empty_reloc.extend_from_slice(&8u32.to_le_bytes()); // Block Size: 8 bytes
        return empty_reloc;
    }

    use std::collections::BTreeMap;
    let mut pages: BTreeMap<u32, Vec<u16>> = BTreeMap::new();

    let reloc_type = if is_64bit { IMAGE_REL_BASED_DIR64 } else { IMAGE_REL_BASED_HIGHLOW };

    for &rva in reloc_rvas {
        let page_rva = rva & !0xFFF;
        let offset_in_page = (rva & 0xFFF) as u16;
        let entry = (reloc_type << 12) | offset_in_page;
        pages.entry(page_rva).or_default().push(entry);
    }

    let mut reloc_sec = Vec::new();

    for (page_rva, mut entries) in pages {
        // Pad to 4-byte boundary if needed
        if (entries.len() & 1) != 0 {
            entries.push(IMAGE_REL_BASED_ABSOLUTE);
        }

        let block_size = (8 + entries.len() * 2) as u32;
        reloc_sec.extend_from_slice(&page_rva.to_le_bytes());
        reloc_sec.extend_from_slice(&block_size.to_le_bytes());

        for entry in entries {
            reloc_sec.extend_from_slice(&entry.to_le_bytes());
        }
    }

    reloc_sec
}
