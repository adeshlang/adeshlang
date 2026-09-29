//! Symbol model, bindings, visibilities, and symbol table representation.

use std::collections::HashMap;
use std::fmt;

/// Symbol linkage binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolBinding {
    Local,
    Global,
    Weak,
}

/// Symbol visibility specifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolVisibility {
    Default,
    Hidden,
    Protected,
    Internal,
}

/// Category of symbol content.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SymbolType {
    Function,
    Object,
    Tls,
    Section,
    File,
    Common,
    Unknown,
}

/// Unified Linker Symbol representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    pub name: String,
    pub binding: SymbolBinding,
    pub visibility: SymbolVisibility,
    pub sym_type: SymbolType,
    pub section_index: Option<usize>,
    pub value: u64,
    pub size: u64,
    pub is_defined: bool,
    pub is_imported: bool,
    pub is_exported: bool,
    pub file_index: Option<usize>,
    pub alias_of: Option<String>,
    pub comdat_group: Option<String>,
    pub version: Option<String>,
}

impl Symbol {
    pub fn new_defined(
        name: impl Into<String>,
        binding: SymbolBinding,
        sym_type: SymbolType,
        section_index: usize,
        value: u64,
        size: u64,
        file_index: usize,
    ) -> Self {
        Self {
            name: name.into(),
            binding,
            visibility: SymbolVisibility::Default,
            sym_type,
            section_index: Some(section_index),
            value,
            size,
            is_defined: true,
            is_imported: false,
            is_exported: false,
            file_index: Some(file_index),
            alias_of: None,
            comdat_group: None,
            version: None,
        }
    }

    pub fn new_undefined(name: impl Into<String>, file_index: usize) -> Self {
        Self {
            name: name.into(),
            binding: SymbolBinding::Global,
            visibility: SymbolVisibility::Default,
            sym_type: SymbolType::Unknown,
            section_index: None,
            value: 0,
            size: 0,
            is_defined: false,
            is_imported: false,
            is_exported: false,
            file_index: Some(file_index),
            alias_of: None,
            comdat_group: None,
            version: None,
        }
    }

    pub fn is_weak(&self) -> bool {
        self.binding == SymbolBinding::Weak
    }

    pub fn is_global(&self) -> bool {
        self.binding == SymbolBinding::Global
    }

    pub fn is_local(&self) -> bool {
        self.binding == SymbolBinding::Local
            || self.sym_type == SymbolType::File
            || self.sym_type == SymbolType::Section
            || self.name.starts_with('.')
    }
}

impl fmt::Display for Symbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let bind_str = match self.binding {
            SymbolBinding::Local => "LOC",
            SymbolBinding::Global => "GLO",
            SymbolBinding::Weak => "WEA",
        };
        let type_str = match self.sym_type {
            SymbolType::Function => "FUNC",
            SymbolType::Object => "OBJ ",
            SymbolType::Tls => "TLS ",
            SymbolType::Section => "SECT",
            SymbolType::File => "FILE",
            SymbolType::Common => "COMM",
            SymbolType::Unknown => "UNK ",
        };
        let state = if self.is_defined { "DEF" } else { "UND" };
        write!(
            f,
            "0x{:016x} {:5} {} {} {} {}",
            self.value, self.size, bind_str, type_str, state, self.name
        )
    }
}

/// In-memory Symbol Table.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    symbols: Vec<Symbol>,
    name_to_index: HashMap<String, usize>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            symbols: Vec::new(),
            name_to_index: HashMap::new(),
        }
    }

    pub fn insert(&mut self, symbol: Symbol) -> usize {
        let index = self.symbols.len();
        if symbol.binding != SymbolBinding::Local {
            self.name_to_index.insert(symbol.name.clone(), index);
        }
        self.symbols.push(symbol);
        index
    }

    pub fn find_by_name(&self, name: &str) -> Option<&Symbol> {
        self.name_to_index.get(name).map(|&idx| &self.symbols[idx])
    }

    pub fn find_by_name_mut(&mut self, name: &str) -> Option<&mut Symbol> {
        if let Some(&idx) = self.name_to_index.get(name) {
            Some(&mut self.symbols[idx])
        } else {
            None
        }
    }

    pub fn get(&self, index: usize) -> Option<&Symbol> {
        self.symbols.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut Symbol> {
        self.symbols.get_mut(index)
    }

    pub fn len(&self) -> usize {
        self.symbols.len()
    }

    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Symbol> {
        self.symbols.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Symbol> {
        self.symbols.iter_mut()
    }
}
