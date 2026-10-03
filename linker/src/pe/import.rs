use std::collections::HashMap;

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
    pub symbol_iat_rvas: HashMap<String, u32>,
    /// The `.idata` section RVA this table was built for. All internal RVAs
    /// (names, ILT, IAT, descriptor pointers) are relative to the image base
    /// assuming the section lands exactly at this RVA.
    pub idata_rva: u32,
}

/// Returns `true` if a symbol name looks like a genuine Windows API / CRT export.
/// Rust mangled names (`ZN...`), adesh-internal names, and similar must be excluded.
pub fn is_valid_windows_api_symbol(sym: &str) -> bool {
    // Must not be empty
    if sym.is_empty() {
        return false;
    }
    // Rust-mangled symbols (ZN4core..., ZN3std..., etc.)
    if sym.starts_with("ZN") {
        return false;
    }
    // Adesh / Rust internals
    if sym.starts_with("__rust")
        || sym.starts_with("rust_")
        || sym.starts_with("anon.")
        || sym.starts_with("_ZN")
        || sym.starts_with("adesh_")
        || sym.starts_with("aot_")
        || sym.contains("$u7b$")   // Rust closure/const name-mangling
        || sym.contains("$LT$")
        || sym.contains("$GT$")
        || sym.contains("..")
        || sym.starts_with("??")
        || sym.starts_with('$')
        || sym.starts_with('.')
    {
        return false;
    }
    true
}

/// Classify a symbol to its corresponding Windows system/CRT DLL.
pub fn classify_windows_dll(sym_name: &str) -> &'static str {
    let clean = sym_name
        .strip_prefix("__imp_")
        .or_else(|| sym_name.strip_prefix("_imp_"))
        .unwrap_or(sym_name)
        .trim_start_matches('_');

    // Winsock2 symbols
    if clean.starts_with("WSA")
        || matches!(
            clean,
            "socket"
                | "connect"
                | "bind"
                | "listen"
                | "accept"
                | "send"
                | "recv"
                | "sendto"
                | "recvfrom"
                | "closesocket"
                | "shutdown"
                | "getaddrinfo"
                | "freeaddrinfo"
                | "getnameinfo"
                | "getpeername"
                | "getsockname"
                | "select"
                | "ioctlsocket"
                | "setsockopt"
                | "getsockopt"
                | "htons"
                | "ntohs"
                | "htonl"
                | "ntohl"
                | "inet_ntop"
                | "inet_pton"
        )
    {
        "ws2_32.dll"
    } else if clean.starts_with("Reg")
        || clean.starts_with("SystemFunction")
        || clean.starts_with("Crypt")
        || clean.starts_with("OpenProcessToken")
        || clean.starts_with("GetTokenInformation")
        || clean == "ReleaseMutex"
        || clean == "UnlockFile"
        || clean == "LockFileEx"
        || clean == "ProcessPrng"
    {
        "advapi32.dll"
    } else if clean.starts_with("MessageBox")
        || clean.starts_with("GetDesktopWindow")
        || clean.starts_with("ShowWindow")
        || clean.starts_with("PeekMessage")
        || clean.starts_with("DispatchMessage")
        || clean == "lstrlenW"
    {
        "user32.dll"
    } else if clean.starts_with("BCrypt") {
        "bcrypt.dll"
    } else if clean.starts_with("Nt") || clean.starts_with("Zw") || clean.starts_with("RtlNtStatus")
    {
        "ntdll.dll"
    } else if clean == "WaitOnAddress"
        || clean == "WakeByAddressAll"
        || clean == "WakeByAddressSingle"
    {
        // WaitOnAddress lives in KERNEL32.dll on Windows 8+ (not synchronization.dll)
        "KERNEL32.dll"
    } else if clean == "CompareStringOrdinal"
        || clean == "UpdateProcThreadAttribute"
        || clean == "CopyFileExW"
    {
        "KERNEL32.dll"
    } else if clean == "ExitProcess"
        || clean.starts_with("Get")
        || clean.starts_with("Set")
        || clean.starts_with("Create")
        || clean.starts_with("Close")
        || clean.starts_with("Read")
        || clean.starts_with("Write")
        || clean.starts_with("Delete")
        || clean.starts_with("Move")
        || clean.starts_with("Find")
        || clean.starts_with("Virtual")
        || clean.starts_with("Heap")
        || clean.starts_with("Local")
        || clean.starts_with("Global")
        || clean.starts_with("Load")
        || clean.starts_with("Free")
        || clean.starts_with("Sleep")
        || clean.starts_with("Switch")
        || clean.starts_with("Query")
        || clean.starts_with("Rtl")
        || clean.starts_with("Tls")
        || clean.starts_with("Add")
        || clean.starts_with("Remove")
        || clean.starts_with("Duplicate")
        || clean.starts_with("Flush")
        || clean.starts_with("WaitFor")
        || clean.starts_with("Terminate")
        || clean.starts_with("FormatMessage")
        || clean.starts_with("WideChar")
        || clean.starts_with("MultiByte")
        || clean.starts_with("SystemTime")
        || clean.starts_with("FileTime")
        || clean.starts_with("DeviceIoControl")
        || clean.starts_with("CancelIo")
        || clean.starts_with("Initialize")
        || clean.starts_with("Enter")
        || clean.starts_with("Leave")
    {
        "KERNEL32.dll"
    } else {
        "msvcrt.dll"
    }
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
    // A membership set keeps the per-symbol dedup O(1) instead of rescanning
    // the accumulated vector for every import (previously O(n^2)).
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for imp in imports {
        if !seen.insert((imp.dll_name.clone(), imp.symbol_name.clone())) {
            continue;
        }
        by_dll
            .entry(imp.dll_name.clone())
            .or_default()
            .push(imp.symbol_name.clone());
    }

    let mut idata = Vec::new();
    let num_dlls = by_dll.len();
    let desc_table_size = (num_dlls + 1) * 20; // 20 bytes per IMAGE_IMPORT_DESCRIPTOR + null descriptor

    idata.resize(desc_table_size, 0);

    // Two-pass layout:
    //   Pass 1 (per DLL): DLL name strings, hint/name entries, and the ILT.
    //   Pass 2: all IAT arrays emitted contiguously at the end of the section.
    //
    // Keeping every IAT contiguous is what the PE loader expects from the
    // IMAGE_DIRECTORY_ENTRY_IAT data directory (a single [start, end) range),
    // and it matches the layout produced by MSVC link.exe.

    struct DllLayout {
        dll_name_rva: u32,
        ilt_rva: u32,
        hint_rvas: Vec<u32>,
        iat_rva: u32,
    }

    let mut layouts: Vec<DllLayout> = Vec::new();
    let mut symbol_iat_rvas = HashMap::new();

    // Pass 1: names, hints, ILTs.
    for (dll_name, symbols) in &by_dll {
        let dll_name_rva = idata_rva + (idata.len() as u32);
        idata.extend_from_slice(dll_name.as_bytes());
        idata.push(0);
        if (idata.len() & 1) != 0 {
            idata.push(0);
        }

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

        // ILT (Import Lookup Table) - 8 bytes per entry + null
        let ilt_rva = idata_rva + (idata.len() as u32);
        for &hrva in &hint_rvas {
            idata.extend_from_slice(&(hrva as u64).to_le_bytes());
        }
        idata.extend_from_slice(&0u64.to_le_bytes()); // Null terminator

        layouts.push(DllLayout {
            dll_name_rva,
            ilt_rva,
            hint_rvas,
            iat_rva: 0,
        });
    }

    // Pass 2: contiguous IAT block for all DLLs.
    // Align the IAT block to 8 bytes for cleanliness.
    while (idata.len() & 7) != 0 {
        idata.push(0);
    }
    let first_iat_rva = idata_rva + (idata.len() as u32);
    let iat_block_start = idata.len();

    for (dll_idx, symbols) in by_dll.values().enumerate() {
        let iat_rva = idata_rva + (idata.len() as u32);
        layouts[dll_idx].iat_rva = iat_rva;
        for (i, &hrva) in layouts[dll_idx].hint_rvas.iter().enumerate() {
            let sym_iat_rva = iat_rva + (i as u32) * 8;
            symbol_iat_rvas.insert(symbols[i].clone(), sym_iat_rva);
            idata.extend_from_slice(&(hrva as u64).to_le_bytes());
        }
        idata.extend_from_slice(&0u64.to_le_bytes()); // Null terminator
    }
    let total_iat_size = (idata.len() - iat_block_start) as u32;

    // Write IMAGE_IMPORT_DESCRIPTORs.
    for (desc_idx, layout) in layouts.iter().enumerate() {
        let desc_off = desc_idx * 20;
        idata[desc_off..desc_off + 4].copy_from_slice(&layout.ilt_rva.to_le_bytes()); // OriginalFirstThunk (ILT)
        idata[desc_off + 4..desc_off + 8].copy_from_slice(&0u32.to_le_bytes()); // TimeDateStamp
        idata[desc_off + 8..desc_off + 12].copy_from_slice(&0u32.to_le_bytes()); // ForwarderChain
        idata[desc_off + 12..desc_off + 16].copy_from_slice(&layout.dll_name_rva.to_le_bytes()); // Name RVA
        idata[desc_off + 16..desc_off + 20].copy_from_slice(&layout.iat_rva.to_le_bytes()); // FirstThunk (IAT)
    }

    ImportTableResult {
        data: idata,
        import_descriptor_size: desc_table_size as u32,
        iat_rva: first_iat_rva,
        iat_size: total_iat_size,
        symbol_iat_rvas,
        idata_rva,
    }
}
