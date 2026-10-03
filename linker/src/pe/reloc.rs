//! PE Base Relocations table generator (.reloc).

pub const IMAGE_REL_BASED_ABSOLUTE: u16 = 0;
pub const IMAGE_REL_BASED_HIGHLOW: u16 = 3;
pub const IMAGE_REL_BASED_DIR64: u16 = 10;

/// Construct a `.reloc` base relocations section for Windows ASLR.
///
/// `dir64_rvas` holds the RVAs of 64-bit absolute fields (emitted as
/// IMAGE_REL_BASED_DIR64) and `highlow_rvas` the RVAs of 32-bit absolute fields
/// (emitted as IMAGE_REL_BASED_HIGHLOW, which is what a 32-bit x86 image
/// needs). The entry type is chosen per field, not per image.
pub fn build_base_reloc_table(dir64_rvas: &[u32], highlow_rvas: &[u32]) -> Vec<u8> {
    use std::collections::BTreeMap;
    let mut pages: BTreeMap<u32, Vec<u16>> = BTreeMap::new();

    for &rva in dir64_rvas {
        let entry = (IMAGE_REL_BASED_DIR64 << 12) | (rva & 0xFFF) as u16;
        pages.entry(rva & !0xFFF).or_default().push(entry);
    }
    for &rva in highlow_rvas {
        let entry = (IMAGE_REL_BASED_HIGHLOW << 12) | (rva & 0xFFF) as u16;
        pages.entry(rva & !0xFFF).or_default().push(entry);
    }

    if pages.is_empty() {
        let mut empty_reloc = Vec::with_capacity(8);
        empty_reloc.extend_from_slice(&0u32.to_le_bytes()); // Page RVA: 0
        empty_reloc.extend_from_slice(&8u32.to_le_bytes()); // Block Size: 8 bytes
        return empty_reloc;
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
