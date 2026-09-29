//! PE Import Directory (.idata) generator.

/// An imported symbol from a specific DLL.
#[derive(Debug, Clone)]
pub struct ImportSymbol {
    pub dll_name: String,
    pub symbol_name: String,
    pub ordinal: Option<u16>,
}

/// Result of building a PE import directory.
#[derive(Debug, Clone, Default)]
pub struct ImportTableResult {
    pub data: Vec<u8>,
    pub import_descriptor_size: u32,
    pub iat_rva: u32,
    pub iat_size: u32,
}

/// Helper to generate `.idata` section data for PE binaries.
pub fn build_import_table(
    imports: &[ImportSymbol],
    _image_base: u64,
    idata_rva: u32,
) -> ImportTableResult {
    if imports.is_empty() {
        return ImportTableResult::default();
    }

    // Group imports by DLL
    use std::collections::BTreeMap;
    let mut by_dll: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for imp in imports {
        by_dll
            .entry(imp.dll_name.clone())
            .or_default()
            .push(imp.symbol_name.clone());
    }

    let mut idata = Vec::new();
    let num_dlls = by_dll.len();
    let desc_table_size = (num_dlls + 1) * 20; // 20 bytes per IMAGE_IMPORT_DESCRIPTOR + null descriptor

    idata.resize(desc_table_size, 0);

    // Layout names and ILT/IAT arrays
    let mut current_offset = desc_table_size;
    let mut desc_idx = 0;
    let mut first_iat_rva = 0u32;
    let mut total_iat_size = 0u32;

    for (dll_name, symbols) in &by_dll {
        // DLL name offset
        let dll_name_rva = idata_rva + (current_offset as u32);
        idata.extend_from_slice(dll_name.as_bytes());
        idata.push(0);
        if (idata.len() & 1) != 0 {
            idata.push(0);
        }

        // Hint/Name table offsets for each symbol
        let mut hint_rvas = Vec::new();
        for sym in symbols {
            let hint_rva = idata_rva + (idata.len() as u32);
            hint_rvas.push(hint_rva);
            idata.extend_from_slice(&0u16.to_le_bytes()); // Hint (0)
            idata.extend_from_slice(sym.as_bytes());
            idata.push(0);
            if (idata.len() & 1) != 0 {
                idata.push(0);
            }
        }
        current_offset = idata.len();

        // ILT (Import Lookup Table) - 8 bytes per entry + null
        let ilt_rva = idata_rva + (current_offset as u32);
        for &hrva in &hint_rvas {
            idata.extend_from_slice(&(hrva as u64).to_le_bytes());
        }
        idata.extend_from_slice(&0u64.to_le_bytes()); // Null terminator

        // IAT (Import Address Table) - 8 bytes per entry + null
        let iat_rva = idata_rva + (idata.len() as u32);
        if first_iat_rva == 0 {
            first_iat_rva = iat_rva;
        }
        let iat_start_off = idata.len();
        for &hrva in &hint_rvas {
            idata.extend_from_slice(&(hrva as u64).to_le_bytes());
        }
        idata.extend_from_slice(&0u64.to_le_bytes()); // Null terminator
        total_iat_size += (idata.len() - iat_start_off) as u32;
        current_offset = idata.len();

        // Write IMAGE_IMPORT_DESCRIPTOR
        let desc_off = desc_idx * 20;
        idata[desc_off..desc_off + 4].copy_from_slice(&ilt_rva.to_le_bytes()); // OriginalFirstThunk (ILT)
        idata[desc_off + 4..desc_off + 8].copy_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
        idata[desc_off + 8..desc_off + 12].copy_from_slice(&0u32.to_le_bytes()); // ForwarderChain
        idata[desc_off + 12..desc_off + 16].copy_from_slice(&dll_name_rva.to_le_bytes()); // Name RVA
        idata[desc_off + 16..desc_off + 20].copy_from_slice(&iat_rva.to_le_bytes()); // FirstThunk (IAT)

        desc_idx += 1;
    }

    ImportTableResult {
        data: idata,
        import_descriptor_size: desc_table_size as u32,
        iat_rva: first_iat_rva,
        iat_size: total_iat_size,
    }
}
