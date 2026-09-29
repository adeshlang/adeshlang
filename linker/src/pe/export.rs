//! PE Export Directory (.edata) generator.

/// An exported symbol entry in a PE dynamic link library.
#[derive(Debug, Clone)]
pub struct ExportSymbol {
    pub name: String,
    pub rva: u32,
    pub ordinal: u16,
}

/// Helper to build `.edata` section payload.
pub fn build_export_table(module_name: &str, exports: &[ExportSymbol], edata_rva: u32) -> Vec<u8> {
    if exports.is_empty() {
        return Vec::new();
    }

    let mut edata = Vec::new();
    let header_size = 40usize; // IMAGE_EXPORT_DIRECTORY
    edata.resize(header_size, 0);

    let num_exports = exports.len() as u32;

    // Layout Module Name
    let mod_name_rva = edata_rva + (edata.len() as u32);
    edata.extend_from_slice(module_name.as_bytes());
    edata.push(0);
    if (edata.len() & 1) != 0 {
        edata.push(0);
    }

    // Export Address Table (EAT) - 4 bytes per RVA
    let eat_rva = edata_rva + (edata.len() as u32);
    for exp in exports {
        edata.extend_from_slice(&exp.rva.to_le_bytes());
    }

    // Name strings and Name Pointer Table (ENT)
    let mut name_rvas = Vec::new();
    for exp in exports {
        let n_rva = edata_rva + (edata.len() as u32);
        name_rvas.push(n_rva);
        edata.extend_from_slice(exp.name.as_bytes());
        edata.push(0);
        if (edata.len() & 1) != 0 {
            edata.push(0);
        }
    }

    let ent_rva = edata_rva + (edata.len() as u32);
    for &nrva in &name_rvas {
        edata.extend_from_slice(&nrva.to_le_bytes());
    }

    // Ordinal Table (EOT) - 2 bytes per export
    let eot_rva = edata_rva + (edata.len() as u32);
    for (idx, _exp) in exports.iter().enumerate() {
        edata.extend_from_slice(&(idx as u16).to_le_bytes());
    }

    // Write IMAGE_EXPORT_DIRECTORY
    edata[12..16].copy_from_slice(&mod_name_rva.to_le_bytes()); // Name RVA
    edata[16..20].copy_from_slice(&1u32.to_le_bytes()); // Base Ordinal (1)
    edata[20..24].copy_from_slice(&num_exports.to_le_bytes()); // NumberOfFunctions
    edata[24..28].copy_from_slice(&num_exports.to_le_bytes()); // NumberOfNames
    edata[28..32].copy_from_slice(&eat_rva.to_le_bytes()); // AddressOfFunctions
    edata[32..36].copy_from_slice(&ent_rva.to_le_bytes()); // AddressOfNames
    edata[36..40].copy_from_slice(&eot_rva.to_le_bytes()); // AddressOfNameOrdinals

    edata
}
