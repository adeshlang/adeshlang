//! Byte Order Mark (BOM) Detection and Handling for AdeshLang.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BomKind {
    Utf8,
    Utf16Le,
    Utf16Be,
    Utf32Le,
    Utf32Be,
    None,
}

impl BomKind {
    pub fn name(&self) -> &'static str {
        match self {
            BomKind::Utf8 => "UTF-8",
            BomKind::Utf16Le => "UTF-16LE",
            BomKind::Utf16Be => "UTF-16BE",
            BomKind::Utf32Le => "UTF-32LE",
            BomKind::Utf32Be => "UTF-32BE",
            BomKind::None => "NONE",
        }
    }
}

/// Detect BOM header in byte slice. Returns BomKind.
pub fn detect_bom(bytes: &[u8]) -> BomKind {
    if bytes.len() >= 4 {
        if bytes[0..4] == [0xFF, 0xFE, 0x00, 0x00] {
            return BomKind::Utf32Le;
        } else if bytes[0..4] == [0x00, 0x00, 0xFE, 0xFF] {
            return BomKind::Utf32Be;
        }
    }
    if bytes.len() >= 3 {
        if bytes[0..3] == [0xEF, 0xBB, 0xBF] {
            return BomKind::Utf8;
        }
    }
    if bytes.len() >= 2 {
        if bytes[0..2] == [0xFF, 0xFE] {
            return BomKind::Utf16Le;
        } else if bytes[0..2] == [0xFE, 0xFF] {
            return BomKind::Utf16Be;
        }
    }
    BomKind::None
}

/// Strip BOM header from bytes if present. Returns (bom_kind, remaining_bytes).
pub fn remove_bom(bytes: &[u8]) -> (BomKind, &[u8]) {
    let bom = detect_bom(bytes);
    let offset = match bom {
        BomKind::Utf32Le | BomKind::Utf32Be => 4,
        BomKind::Utf8 => 3,
        BomKind::Utf16Le | BomKind::Utf16Be => 2,
        BomKind::None => 0,
    };
    (bom, &bytes[offset..])
}

/// Add BOM header to bytes for specified encoding.
pub fn add_bom(bytes: &[u8], encoding_name: &str) -> Vec<u8> {
    let mut result = Vec::new();
    match encoding_name.to_uppercase().as_str() {
        "UTF-8" | "UTF8" => result.extend_from_slice(&[0xEF, 0xBB, 0xBF]),
        "UTF-16LE" | "UTF16LE" => result.extend_from_slice(&[0xFF, 0xFE]),
        "UTF-16BE" | "UTF16BE" => result.extend_from_slice(&[0xFE, 0xFF]),
        "UTF-32LE" | "UTF32LE" => result.extend_from_slice(&[0xFF, 0xFE, 0x00, 0x00]),
        "UTF-32BE" | "UTF32BE" => result.extend_from_slice(&[0x00, 0x00, 0xFE, 0xFF]),
        _ => {}
    }
    result.extend_from_slice(bytes);
    result
}
