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

        while offset + 60 <= bytes.len() {
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
                || name_raw == "__.SYMDEF"
                || name_raw == "__.SYMDEF SORTED"
                || name_raw.starts_with("/ ")
            {
                // Symbol directory member (skipped in raw extraction; indexed below)
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

                members.push(ArchiveMember {
                    name,
                    size,
                    data: member_data.to_vec(),
                    obj: None,
                });
            }
        }

        // Build accurate symbol index across all members and cache decoded ObjectFiles
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
                    let truncated = format!("{}/", &m.name[..15]);
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
