//! UNIX `.ar` static archive parser and member extraction.

use crate::error::{ErrorCode, LinkError, LinkResult};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const AR_MAGIC: [u8; 8] = *b"!<arch>\n";

/// An entry in a static archive (`.a` / `.lib`).
#[derive(Debug, Clone)]
pub struct ArchiveMember {
    pub name: String,
    pub size: usize,
    pub data: Vec<u8>,
    pub obj: Option<crate::object::ObjectFile>,
}

/// Static Archive reader.
#[derive(Debug, Clone)]
pub struct Archive {
    pub path: PathBuf,
    pub members: Vec<ArchiveMember>,
    pub symbol_index: HashMap<String, Vec<usize>>, // symbol_name -> defining member indices
}

impl Archive {
    pub fn parse(bytes: &[u8], path: &Path) -> LinkResult<Self> {
        if bytes.len() < 8 || &bytes[0..8] != &AR_MAGIC {
            return Err(LinkError::new(
                ErrorCode::InvalidArchive,
                format!(
                    "file `{}` is not a valid static archive (missing AR magic)",
                    path.display()
                ),
            ));
        }

        let mut offset = 8;
        let mut members = Vec::new();
        let mut string_table = Vec::new();
        let mut symbol_index = HashMap::new();
        let mut member_header_offsets = Vec::new();
        let mut archive_symbol_tables: Vec<(String, Vec<u8>)> = Vec::new();

        while offset + 60 <= bytes.len() {
            let header_offset = offset;
            let header = &bytes[offset..offset + 60];
            let name_raw = std::str::from_utf8(&header[0..16]).unwrap_or("").trim_end();
            let size_str = std::str::from_utf8(&header[48..58]).unwrap_or("0").trim();
            let size: usize = size_str.parse().unwrap_or(0);

            offset += 60;
            if offset + size > bytes.len() {
                return Err(LinkError::new(
                    ErrorCode::InvalidArchive,
                    format!(
                        "archive member `{}` in `{}` is truncated",
                        name_raw,
                        path.display()
                    ),
                ));
            }

            let member_data = &bytes[offset..offset + size];
            offset += size;
            if (size & 1) != 0 && offset < bytes.len() {
                offset += 1; // 2-byte alignment padding
            }

            if name_raw == "//" {
                // GNU string table
                string_table = member_data.to_vec();
            } else if name_raw == "/"
                || name_raw == "/SYM64/"
                || name_raw == "__.SYMDEF"
                || name_raw == "__.SYMDEF SORTED"
                || name_raw.starts_with("/ ")
            {
                // Keep archive linker indexes for a cheap symbol scan. The
                // member payloads are decoded only if the resolver extracts
                // them, rather than eagerly parsing every object at startup.
                if name_raw == "/" || name_raw == "/SYM64/" {
                    archive_symbol_tables.push((name_raw.to_string(), member_data.to_vec()));
                }
            } else {
                let name = if name_raw.starts_with('/') && !string_table.is_empty() {
                    // GNU extended filename: /123
                    if let Ok(st_off) = name_raw[1..].parse::<usize>() {
                        if st_off < string_table.len() {
                            let end = string_table[st_off..]
                                .iter()
                                .position(|&b| b == b'/' || b == 0 || b == b'\n')
                                .map(|p| st_off + p)
                                .unwrap_or(string_table.len());
                            String::from_utf8_lossy(&string_table[st_off..end]).to_string()
                        } else {
                            name_raw.to_string()
                        }
                    } else {
                        name_raw.to_string()
                    }
                } else if name_raw.starts_with("#1/") {
                    // BSD extended filename: #1/len
                    if let Ok(len) = name_raw[3..].parse::<usize>() {
                        if len <= member_data.len() {
                            String::from_utf8_lossy(&member_data[0..len])
                                .trim_matches('\0')
                                .to_string()
                        } else {
                            name_raw.to_string()
                        }
                    } else {
                        name_raw.to_string()
                    }
                } else {
                    name_raw.trim_end_matches('/').to_string()
                };

                member_header_offsets.push((header_offset as u64, members.len()));
                members.push(ArchiveMember {
                    name,
                    size,
                    data: member_data.to_vec(),
                    obj: None,
                });
            }
        }

        let offset_to_member: HashMap<u64, usize> = member_header_offsets
            .into_iter()
            .filter(|&(_, member_idx)| !is_coff_import_object(&members[member_idx].data))
            .collect();
        let mut has_usable_index = false;
        for (table_name, table_data) in &archive_symbol_tables {
            has_usable_index |= parse_archive_symbol_index(
                table_name,
                table_data,
                &offset_to_member,
                &mut symbol_index,
            );
        }

        // Archives created without a linker index are uncommon but valid.
        // Preserve support by scanning those members as a fallback; normal
        // GNU/COFF archives remain lazy and parse only selected members.
        if !has_usable_index {
            for (m_idx, m) in members.iter_mut().enumerate() {
                if let Ok(obj) = crate::object::reader::ObjectReader::read_from_memory(
                    &m.data,
                    std::path::Path::new(&m.name),
                    &crate::target::Target::host(),
                    0,
                ) {
                    for sym in &obj.symbols {
                        if sym.is_defined && !sym.is_local() {
                            let candidates: &mut Vec<usize> =
                                symbol_index.entry(sym.name.clone()).or_default();
                            if candidates.last() != Some(&m_idx) {
                                candidates.push(m_idx);
                            }
                        }
                    }
                    m.obj = Some(obj);
                }
            }
        }

        Ok(Self {
            path: path.to_path_buf(),
            members,
            symbol_index,
        })
    }

    /// Create an empty static archive.
    pub fn new() -> Self {
        Self {
            path: PathBuf::new(),
            members: Vec::new(),
            symbol_index: HashMap::new(),
        }
    }

    /// Add a member object or file to the archive.
    pub fn add_file(&mut self, name: impl Into<String>, data: Vec<u8>) {
        let name = name.into();
        let size = data.len();
        self.members.push(ArchiveMember {
            name,
            size,
            data,
            obj: None,
        });
    }

    /// Encode the archive into standard GNU AR format bytes.
    pub fn encode_gnu(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&AR_MAGIC);

        // Build string table for filenames > 15 characters
        let mut string_table = Vec::new();
        let mut long_names: Vec<(usize, usize)> = Vec::new(); // member_idx -> str_offset

        for (idx, m) in self.members.iter().enumerate() {
            if m.name.len() > 15 {
                let off = string_table.len();
                string_table.extend_from_slice(m.name.as_bytes());
                string_table.push(b'/');
                string_table.push(b'\n');
                long_names.push((idx, off));
            }
        }

        // If we have long names, emit GNU string table member `//`
        if !string_table.is_empty() {
            let mut hdr = [b' '; 60];
            hdr[0..2].copy_from_slice(b"//");
            let size_str = format!("{}", string_table.len());
            hdr[48..48 + size_str.len().min(10)].copy_from_slice(size_str.as_bytes());
            hdr[58..60].copy_from_slice(b"`\n");

            out.extend_from_slice(&hdr);
            out.extend_from_slice(&string_table);
            if (string_table.len() & 1) != 0 {
                out.push(b'\n');
            }
        }

        // Emit each member
        for (idx, m) in self.members.iter().enumerate() {
            let mut hdr = [b' '; 60];
            if m.name.len() > 15 {
                if let Some(&(_, off)) = long_names.iter().find(|(i, _)| *i == idx) {
                    let id = format!("/{}", off);
                    hdr[0..id.len().min(16)].copy_from_slice(id.as_bytes());
                } else {
                    // `m.name.len() > 15` here, but 15 is a byte index: slicing
                    // must land on a char boundary or it panics for a
                    // multi-byte filename.
                    let mut cut = 15.min(m.name.len());
                    while cut > 0 && !m.name.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    let truncated = format!("{}/", &m.name[..cut]);
                    hdr[0..truncated.len().min(16)].copy_from_slice(truncated.as_bytes());
                }
            } else {
                let id = format!("{}/", m.name);
                hdr[0..id.len().min(16)].copy_from_slice(id.as_bytes());
            }

            // Timestamp (0 for deterministic builds)
            hdr[16..17].copy_from_slice(b"0");
            // Owner / Group
            hdr[28..29].copy_from_slice(b"0");
            hdr[34..35].copy_from_slice(b"0");
            // Mode: 644 octal
            hdr[40..46].copy_from_slice(b"100644");
            // Size
            let size_str = format!("{}", m.data.len());
            hdr[48..48 + size_str.len().min(10)].copy_from_slice(size_str.as_bytes());
            // Magic trailer
            hdr[58..60].copy_from_slice(b"`\n");

            out.extend_from_slice(&hdr);
            out.extend_from_slice(&m.data);
            if (m.data.len() & 1) != 0 {
                out.push(b'\n'); // 2-byte alignment padding
            }
        }

        out
    }
}

fn is_coff_import_object(data: &[u8]) -> bool {
    data.len() >= 4 && data[..2] == [0, 0] && data[2..4] == [0xff, 0xff]
}

/// Add symbol names from a GNU/BSD-style first linker member or a COFF
/// second-linker-member table. Returns true when the table structure was
/// valid, including a valid empty index.
fn parse_archive_symbol_index(
    table_name: &str,
    data: &[u8],
    offset_to_member: &HashMap<u64, usize>,
    symbols: &mut HashMap<String, Vec<usize>>,
) -> bool {
    fn read_u32_be(data: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_be_bytes(
            data.get(offset..offset + 4)?.try_into().ok()?,
        ))
    }
    fn read_u32_le(data: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            data.get(offset..offset + 4)?.try_into().ok()?,
        ))
    }
    fn read_u64_be(data: &[u8], offset: usize) -> Option<u64> {
        Some(u64::from_be_bytes(
            data.get(offset..offset + 8)?.try_into().ok()?,
        ))
    }
    fn add_name(
        name: &[u8],
        member_offset: u64,
        offset_to_member: &HashMap<u64, usize>,
        symbols: &mut HashMap<String, Vec<usize>>,
    ) {
        let Some(&member_idx) = offset_to_member.get(&member_offset) else {
            return;
        };
        let name = String::from_utf8_lossy(name).into_owned();
        if name.is_empty() {
            return;
        }
        let candidates = symbols.entry(name).or_default();
        if candidates.last() != Some(&member_idx) {
            candidates.push(member_idx);
        }
    }

    if table_name == "/SYM64/" {
        let Some(count) = read_u64_be(data, 0).map(|v| v as usize) else {
            return false;
        };
        let Some(names_start) = count.checked_mul(8).and_then(|n| n.checked_add(8)) else {
            return false;
        };
        if names_start > data.len() {
            return false;
        }
        let mut names = names_start;
        for i in 0..count {
            let Some(member_offset) = read_u64_be(data, 8 + i * 8) else {
                return false;
            };
            let Some(end_rel) = data[names..].iter().position(|&b| b == 0) else {
                return false;
            };
            add_name(
                &data[names..names + end_rel],
                member_offset,
                offset_to_member,
                symbols,
            );
            names += end_rel + 1;
        }
        return true;
    }

    // GNU and COFF first linker members both use a big-endian symbol count,
    // offset array, then NUL-terminated names.
    if let Some(count) = read_u32_be(data, 0).map(|v| v as usize) {
        if let Some(names_start) = count.checked_mul(4).and_then(|n| n.checked_add(4)) {
            if names_start <= data.len() {
                let mut names = names_start;
                let mut valid = true;
                for i in 0..count {
                    let Some(member_offset) = read_u32_be(data, 4 + i * 4) else {
                        valid = false;
                        break;
                    };
                    let Some(end_rel) = data[names..].iter().position(|&b| b == 0) else {
                        valid = false;
                        break;
                    };
                    add_name(
                        &data[names..names + end_rel],
                        u64::from(member_offset),
                        offset_to_member,
                        symbols,
                    );
                    names += end_rel + 1;
                }
                if valid {
                    return true;
                }
            }
        }
    }

    // MSVC's second linker member stores a member-offset array, then a symbol
    // count and 1-based u16 indices into that array, followed by symbol names.
    let Some(member_count) = read_u32_le(data, 0).map(|v| v as usize) else {
        return false;
    };
    let Some(symbol_count_at) = member_count.checked_mul(4).and_then(|n| n.checked_add(4)) else {
        return false;
    };
    let Some(symbol_count) = read_u32_le(data, symbol_count_at).map(|v| v as usize) else {
        return false;
    };
    let Some(names_start) = symbol_count
        .checked_mul(2)
        .and_then(|n| n.checked_add(symbol_count_at + 4))
    else {
        return false;
    };
    if names_start > data.len() {
        return false;
    }
    let mut names = names_start;
    for i in 0..symbol_count {
        let Some(member_ordinal) = data
            .get(symbol_count_at + 4 + i * 2..symbol_count_at + 6 + i * 2)
            .and_then(|b| <[u8; 2]>::try_from(b).ok())
            .map(u16::from_le_bytes)
        else {
            return false;
        };
        let Some(member_offset) = member_ordinal
            .checked_sub(1)
            .and_then(|idx| ((idx as usize) < member_count).then_some(idx as usize))
            .and_then(|idx| read_u32_le(data, 4 + idx * 4))
        else {
            return false;
        };
        let Some(end_rel) = data[names..].iter().position(|&b| b == 0) else {
            return false;
        };
        add_name(
            &data[names..names + end_rel],
            u64::from(member_offset),
            offset_to_member,
            symbols,
        );
        names += end_rel + 1;
    }
    true
}
