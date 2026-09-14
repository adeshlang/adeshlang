// Symbol table and resolution for AOT compilation
// Generic, language-agnostic symbol management

use std::collections::HashMap;

/// Symbol visibility
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolVisibility {
    /// Public symbol, exported from module
    Public,
    /// Private symbol, local to module
    Private,
    /// External symbol, imported from another module
    External,
}

/// Symbol information
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Symbol name
    pub name: String,
    /// Mangled name for uniqueness
    pub mangled_name: String,
    /// Module name
    pub module: String,
    /// Symbol visibility
    pub visibility: SymbolVisibility,
    /// Address (offset in object file)
    pub address: Option<usize>,
    /// Size in bytes
    pub size: Option<usize>,
    /// Is function (vs data)
    pub is_function: bool,
}

impl Symbol {
    /// Create new symbol
    pub fn new(name: String, module: String, visibility: SymbolVisibility) -> Self {
        let mangled_name = mangle_symbol(&module, &name);
        Self {
            name,
            mangled_name,
            module,
            visibility,
            address: None,
            size: None,
            is_function: true,
        }
    }

    /// Create function symbol
    pub fn function(name: String, module: String, visibility: SymbolVisibility) -> Self {
        let mut sym = Self::new(name, module, visibility);
        sym.is_function = true;
        sym
    }

    /// Create data symbol
    pub fn data(name: String, module: String, visibility: SymbolVisibility) -> Self {
        let mut sym = Self::new(name, module, visibility);
        sym.is_function = false;
        sym
    }

    /// Set address
    pub fn set_address(&mut self, addr: usize) {
        self.address = Some(addr);
    }

    /// Set size
    pub fn set_size(&mut self, size: usize) {
        self.size = Some(size);
    }
}

/// Symbol table
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    /// Symbols by mangled name
    symbols: HashMap<String, Symbol>,
    /// Exports (public symbols)
    exports: Vec<String>,
    /// Imports (external symbols)
    imports: Vec<String>,
}

impl SymbolTable {
    /// Create new symbol table
    pub fn new() -> Self {
        Self::default()
    }

    /// Add symbol
    pub fn add_symbol(&mut self, symbol: Symbol) {
        let mangled = symbol.mangled_name.clone();

        // Track exports and imports
        match symbol.visibility {
            SymbolVisibility::Public => {
                if !self.exports.contains(&mangled) {
                    self.exports.push(mangled.clone());
                }
            }
            SymbolVisibility::External => {
                if !self.imports.contains(&mangled) {
                    self.imports.push(mangled.clone());
                }
            }
            SymbolVisibility::Private => {}
        }

        self.symbols.insert(mangled, symbol);
    }

    /// Get symbol by mangled name
    pub fn get_symbol(&self, mangled_name: &str) -> Option<&Symbol> {
        self.symbols.get(mangled_name)
    }

    /// Get mutable symbol
    pub fn get_symbol_mut(&mut self, mangled_name: &str) -> Option<&mut Symbol> {
        self.symbols.get_mut(mangled_name)
    }

    /// Lookup symbol by module and name
    pub fn lookup(&self, module: &str, name: &str) -> Option<&Symbol> {
        let mangled = mangle_symbol(module, name);
        self.get_symbol(&mangled)
    }

    /// Get all symbols
    pub fn symbols(&self) -> impl Iterator<Item = &Symbol> {
        self.symbols.values()
    }

    /// Get exports
    pub fn exports(&self) -> &[String] {
        &self.exports
    }

    /// Get imports
    pub fn imports(&self) -> &[String] {
        &self.imports
    }

    /// Resolve external symbols from other table
    pub fn resolve_externals(&mut self, other: &SymbolTable) -> Result<(), String> {
        let imports = self.imports.clone();

        for import in &imports {
            if let Some(symbol) = other.get_symbol(import) {
                if symbol.visibility == SymbolVisibility::Public {
                    // Update external symbol with address
                    if let Some(local) = self.get_symbol_mut(import) {
                        local.address = symbol.address;
                        local.size = symbol.size;
                    }
                } else {
                    return Err(format!("Symbol '{}' is not public", import));
                }
            } else {
                return Err(format!("Unresolved external symbol: {}", import));
            }
        }
        Ok(())
    }

    /// Merge symbols from another table
    pub fn merge(&mut self, other: &SymbolTable) {
        for (mangled, symbol) in &other.symbols {
            self.symbols.insert(mangled.clone(), symbol.clone());
        }
        for export in &other.exports {
            if !self.exports.contains(export) {
                self.exports.push(export.clone());
            }
        }
    }
}

/// Mangle symbol name for uniqueness
/// Format: module::name
pub fn mangle_symbol(module: &str, name: &str) -> String {
    format!("{}::{}", module, name)
}

/// Demangle symbol name
pub fn demangle_symbol(mangled: &str) -> Option<(&str, &str)> {
    mangled.split_once("::")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_symbol_creation() {
        let sym = Symbol::function(
            "add".to_string(),
            "math".to_string(),
            SymbolVisibility::Public,
        );
        assert_eq!(sym.name, "add");
        assert_eq!(sym.module, "math");
        assert_eq!(sym.mangled_name, "math::add");
        assert!(sym.is_function);
    }

    #[test]
    fn test_symbol_table() {
        let mut table = SymbolTable::new();
        let sym = Symbol::function(
            "add".to_string(),
            "math".to_string(),
            SymbolVisibility::Public,
        );
        table.add_symbol(sym);

        assert!(table.lookup("math", "add").is_some());
        assert_eq!(table.exports().len(), 1);
    }

    #[test]
    fn test_mangle_demangle() {
        let mangled = mangle_symbol("math", "add");
        assert_eq!(mangled, "math::add");

        let (module, name) = demangle_symbol(&mangled).unwrap();
        assert_eq!(module, "math");
        assert_eq!(name, "add");
    }
}
