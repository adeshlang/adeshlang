//! Archive symbol indexing and lazy member lookup.

use crate::archive::ar::Archive;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ArchiveIndex {
    pub symbol_to_member: HashMap<String, usize>,
}

impl ArchiveIndex {
    pub fn build(archive: &Archive) -> Self {
        let mut index = HashMap::new();
        // In case the archive had an embedded symbol table or we index member symbols directly
        for (m_idx, member) in archive.members.iter().enumerate() {
            // Attempt to parse member as object and record its defined symbols
            if let Ok(obj) = crate::object::reader::ObjectReader::read_from_memory(
                &member.data,
                std::path::Path::new(&member.name),
                &crate::target::Target::host(),
                0,
            ) {
                for sym in &obj.symbols {
                    if sym.is_defined && sym.is_global() {
                        index.entry(sym.name.clone()).or_insert(m_idx);
                    }
                }
            }
        }

        Self {
            symbol_to_member: index,
        }
    }

    pub fn lookup(&self, symbol_name: &str) -> Option<usize> {
        self.symbol_to_member.get(symbol_name).copied()
    }
}
