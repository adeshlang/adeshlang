//! Symbol string table and fast lookup indices for object files.

use crate::symbol::Symbol;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct ObjectSymbolIndex {
    defined_symbols: HashMap<String, usize>,
    undefined_symbols: Vec<String>,
}

impl ObjectSymbolIndex {
    pub fn build(symbols: &[Symbol]) -> Self {
        let mut defined = HashMap::new();
        let mut undefined = Vec::new();

        for (idx, sym) in symbols.iter().enumerate() {
            if sym.is_defined {
                defined.insert(sym.name.clone(), idx);
            } else {
                undefined.push(sym.name.clone());
            }
        }

        Self {
            defined_symbols: defined,
            undefined_symbols: undefined,
        }
    }

    pub fn find_defined(&self, name: &str) -> Option<usize> {
        self.defined_symbols.get(name).copied()
    }

    pub fn undefined(&self) -> &[String] {
        &self.undefined_symbols
    }
}
