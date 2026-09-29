//! Adesh Symbol Mangling and Demangling Standard (ABI v1).
//!
//! Format: `_A<crate_len><crate><module_len><module><item_len><item>E<hash>`

/// Mangle an Adesh symbol path (e.g. `["std", "io", "println"]`).
pub fn mangle_symbol(
    package: &str,
    module_path: &[&str],
    item_name: &str,
    sig_hash: Option<u32>,
) -> String {
    let mut s = String::from("_A");
    s.push_str(&format!("{}{}", package.len(), package));
    for mod_part in module_path {
        s.push_str(&format!("{}{}", mod_part.len(), mod_part));
    }
    s.push_str(&format!("{}{}", item_name.len(), item_name));
    s.push('E');
    if let Some(h) = sig_hash {
        s.push_str(&format!("{:08x}", h));
    }
    s
}

/// Demangle an Adesh mangled symbol back to a human-readable path.
pub fn demangle_symbol(mangled: &str) -> Option<String> {
    if !mangled.starts_with("_A") {
        return None;
    }

    let rest = &mangled[2..];
    let mut parts = Vec::new();
    let mut i = 0;
    let bytes = rest.as_bytes();

    while i < bytes.len() {
        if bytes[i] == b'E' {
            break;
        }

        // Read decimal length
        let mut len_str = String::new();
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            len_str.push(bytes[i] as char);
            i += 1;
        }

        if len_str.is_empty() {
            return None;
        }

        let len: usize = len_str.parse().ok()?;
        if i + len > bytes.len() {
            return None;
        }

        let part = std::str::from_utf8(&bytes[i..i + len]).ok()?;
        parts.push(part);
        i += len;
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("::"))
    }
}
