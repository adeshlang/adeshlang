//! PE Export Directory (.edata) and Import Library (.lib) Generation.

use crate::symbol::Symbol;

/// Information about an exported symbol.
#[derive(Debug, Clone)]
pub struct ExportSymbol {
    pub name: String,
    pub rva: u32,
    pub ordinal: u16,
}

/// The generated export table and its metadata.
#[derive(Debug, Clone)]
pub struct ExportTableResult {
    /// The raw bytes of the `.edata` section.
    pub data: Vec<u8>,
    /// The total size of `.edata`.
    pub edata_size: u32,
    /// The list of exports included.
    pub exports: Vec<ExportSymbol>,
}

/// Builds the `.edata` section contents according to the PE32+ specification.
pub fn build_export_table(
    dll_name: &str,
    symbols: &[Symbol],
    image_base: u64,
    edata_rva: u32,
) -> Option<ExportTableResult> {
    // Collect candidate symbols for export.
    let mut exports: Vec<ExportSymbol> = symbols
        .iter()
        .filter(|s| {
            if !s.is_defined || s.is_local() || s.name.is_empty() {
                return false;
            }
            if s.is_exported {
                return true;
            }
            // By default for shared libraries, export public symbols not starting with linker internals
            !s.name.starts_with("__")
                && s.name != "main"
                && s.name != "mainCRTStartup"
                && s.name != "_start"
                && s.name != "_fltused"
        })
        .enumerate()
        .map(|(idx, s)| {
            let rva = if s.value >= image_base {
                (s.value - image_base) as u32
            } else {
                s.value as u32
            };
            ExportSymbol {
                name: s.name.clone(),
                rva,
                ordinal: (idx + 1) as u16,
            }
        })
        .collect();

    if exports.is_empty() {
        return None;
    }

    // Sort alphabetically by name (strictly mandated by PE specification for binary search)
    exports.sort_by(|a, b| a.name.cmp(&b.name));
    for (i, exp) in exports.iter_mut().enumerate() {
        exp.ordinal = (i + 1) as u16;
    }

    let count = exports.len() as u32;
    let header_size = 40u32; // sizeof(IMAGE_EXPORT_DIRECTORY)
    let eat_size = count * 4;
    let enpt_size = count * 4;
    let eot_size = count * 2;
    let eot_padded_size = (eot_size + 3) & !3; // 4-byte align before strings

    let eat_offset = header_size;
    let enpt_offset = eat_offset + eat_size;
    let eot_offset = enpt_offset + enpt_size;
    let strings_offset = eot_offset + eot_padded_size;

    let mut data = vec![0u8; strings_offset as usize];

    // Append DLL name string
    let dll_name_rva = edata_rva + data.len() as u32;
    data.extend_from_slice(dll_name.as_bytes());
    data.push(0); // null terminator

    // Append export names and record their RVAs
    let mut name_rvas = Vec::with_capacity(count as usize);
    for exp in &exports {
        let name_rva = edata_rva + data.len() as u32;
        name_rvas.push(name_rva);
        data.extend_from_slice(exp.name.as_bytes());
        data.push(0);
    }

    // Pad total length to 4 bytes
    while data.len() % 4 != 0 {
        data.push(0);
    }

    let eat_rva = edata_rva + eat_offset;
    let enpt_rva = edata_rva + enpt_offset;
    let eot_rva = edata_rva + eot_offset;

    // IMAGE_EXPORT_DIRECTORY (40 bytes):
    // 0..4: Characteristics = 0
    // 4..8: TimeDateStamp = 0x60000000
    data[4..8].copy_from_slice(&0x60000000u32.to_le_bytes());
    // 8..10: MajorVersion = 0
    // 10..12: MinorVersion = 0
    // 12..16: Name RVA
    data[12..16].copy_from_slice(&dll_name_rva.to_le_bytes());
    // 16..20: Base = 1
    data[16..20].copy_from_slice(&1u32.to_le_bytes());
    // 20..24: NumberOfFunctions = count
    data[20..24].copy_from_slice(&count.to_le_bytes());
    // 24..28: NumberOfNames = count
    data[24..28].copy_from_slice(&count.to_le_bytes());
    // 28..32: AddressOfFunctions = eat_rva
    data[28..32].copy_from_slice(&eat_rva.to_le_bytes());
    // 32..36: AddressOfNames = enpt_rva
    data[32..36].copy_from_slice(&enpt_rva.to_le_bytes());
    // 36..40: AddressOfNameOrdinals = eot_rva
    data[36..40].copy_from_slice(&eot_rva.to_le_bytes());

    // Populate EAT (Export Address Table)
    for (i, exp) in exports.iter().enumerate() {
        let off = (eat_offset + (i as u32 * 4)) as usize;
        data[off..off + 4].copy_from_slice(&exp.rva.to_le_bytes());
    }

    // Populate ENPT (Export Name Pointer Table)
    for (i, &name_rva) in name_rvas.iter().enumerate() {
        let off = (enpt_offset + (i as u32 * 4)) as usize;
        data[off..off + 4].copy_from_slice(&name_rva.to_le_bytes());
    }

    // Populate EOT (Export Ordinal Table: 0-based index relative to Base)
    for (i, _exp) in exports.iter().enumerate() {
        let off = (eot_offset + (i as u32 * 2)) as usize;
        let ord_idx = i as u16;
        data[off..off + 2].copy_from_slice(&ord_idx.to_le_bytes());
    }

    let edata_size = data.len() as u32;
    Some(ExportTableResult {
        data,
        edata_size,
        exports,
    })
}

/// Creates a COFF Short Import Header object member for a symbol.
pub fn create_short_import_object(
    symbol_name: &str,
    dll_name: &str,
    machine: u16,
    ordinal: u16,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0u16.to_le_bytes()); // Sig1: IMAGE_FILE_MACHINE_UNKNOWN (0)
    bytes.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Sig2: IMPORT_OBJECT_HDR_SIG (0xFFFF)
    bytes.extend_from_slice(&0u16.to_le_bytes()); // Version: 0
    bytes.extend_from_slice(&machine.to_le_bytes()); // Machine
    bytes.extend_from_slice(&0x60000000u32.to_le_bytes()); // TimeDateStamp
    let data_len = (symbol_name.len() + 1 + dll_name.len() + 1) as u32;
    bytes.extend_from_slice(&data_len.to_le_bytes()); // SizeOfData
    bytes.extend_from_slice(&ordinal.to_le_bytes()); // Ordinal / Hint
    bytes.extend_from_slice(&0u16.to_le_bytes()); // Type=0 (code), NameType=0 (name)

    bytes.extend_from_slice(symbol_name.as_bytes());
    bytes.push(0);
    bytes.extend_from_slice(dll_name.as_bytes());
    bytes.push(0);

    bytes
}

/// Creates a standard MSVC/GNU compatible COFF import library (`.lib`) containing short import objects.
pub fn create_import_library(dll_name: &str, exports: &[ExportSymbol], machine: u16) -> Vec<u8> {
    let mut archive = crate::archive::Archive::new();
    for exp in exports {
        let member_data = create_short_import_object(&exp.name, dll_name, machine, exp.ordinal);
        let member_name = format!("{}.obj", exp.name);
        archive.add_file(member_name, member_data);
    }
    archive.encode_gnu()
}
