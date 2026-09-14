// Static linker for AOT compilation
// Generic, language-agnostic linking

use super::symbols::SymbolTable;
use std::path::{Path, PathBuf};

/// Relocation type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelocationType {
    /// Absolute address (64-bit)
    Absolute64,
    /// PC-relative (32-bit)
    PCRel32,
    /// GOT relative
    GOTRel,
}

/// Relocation entry
#[derive(Debug, Clone)]
pub struct Relocation {
    /// Offset in section
    pub offset: usize,
    /// Relocation type
    pub rel_type: RelocationType,
    /// Symbol name
    pub symbol: String,
    /// Addend
    pub addend: i64,
}

/// Object file representation
#[derive(Debug, Clone)]
pub struct ObjectFile {
    /// File path
    pub path: PathBuf,
    /// Code section
    pub code: Vec<u8>,
    /// Data section
    pub data: Vec<u8>,
    /// Symbol table
    pub symbols: SymbolTable,
    /// Relocations
    pub relocations: Vec<Relocation>,
}

impl ObjectFile {
    /// Create new object file
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            code: Vec::new(),
            data: Vec::new(),
            symbols: SymbolTable::new(),
            relocations: Vec::new(),
        }
    }

    /// Add code
    pub fn add_code(&mut self, code: Vec<u8>) {
        self.code = code;
    }

    /// Add data
    pub fn add_data(&mut self, data: Vec<u8>) {
        self.data = data;
    }

    /// Add relocation
    pub fn add_relocation(&mut self, reloc: Relocation) {
        self.relocations.push(reloc);
    }
}

/// Static linker
#[derive(Debug)]
pub struct Linker {
    /// Object files to link
    objects: Vec<ObjectFile>,
    /// Global symbol table
    global_symbols: SymbolTable,
    /// Entry point symbol
    entry_point: Option<String>,
}

impl Linker {
    /// Create new linker
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            global_symbols: SymbolTable::new(),
            entry_point: None,
        }
    }

    /// Add object file
    pub fn add_object(&mut self, obj: ObjectFile) {
        self.objects.push(obj);
    }

    /// Set entry point
    pub fn set_entry_point(&mut self, symbol: String) {
        self.entry_point = Some(symbol);
    }

    /// Build global symbol table
    fn build_symbol_table(&mut self) -> Result<(), String> {
        // Collect all symbols
        for obj in &self.objects {
            self.global_symbols.merge(&obj.symbols);
        }

        // Resolve external symbols
        for obj in &mut self.objects {
            obj.symbols.resolve_externals(&self.global_symbols)?;
        }

        Ok(())
    }

    /// Apply relocations
    fn apply_relocations(&mut self, code: &mut Vec<u8>) -> Result<(), String> {
        let mut offset = 0;

        for obj in &self.objects {
            for reloc in &obj.relocations {
                let symbol = self
                    .global_symbols
                    .get_symbol(&reloc.symbol)
                    .ok_or_else(|| format!("Unresolved symbol: {}", reloc.symbol))?;

                let target_addr = symbol
                    .address
                    .ok_or_else(|| format!("Symbol has no address: {}", reloc.symbol))?;

                let patch_offset = offset + reloc.offset;

                match reloc.rel_type {
                    RelocationType::Absolute64 => {
                        let value = (target_addr as i64 + reloc.addend) as u64;
                        let bytes = value.to_le_bytes();
                        for (i, &byte) in bytes.iter().enumerate() {
                            if patch_offset + i < code.len() {
                                code[patch_offset + i] = byte;
                            }
                        }
                    }
                    RelocationType::PCRel32 => {
                        let pc = patch_offset + 4; // After instruction
                        let value = (target_addr as i64 - pc as i64 + reloc.addend) as i32;
                        let bytes = value.to_le_bytes();
                        for (i, &byte) in bytes.iter().enumerate() {
                            if patch_offset + i < code.len() {
                                code[patch_offset + i] = byte;
                            }
                        }
                    }
                    RelocationType::GOTRel => {
                        // Simplified GOT handling
                        let value = (target_addr as i64 + reloc.addend) as i32;
                        let bytes = value.to_le_bytes();
                        for (i, &byte) in bytes.iter().enumerate() {
                            if patch_offset + i < code.len() {
                                code[patch_offset + i] = byte;
                            }
                        }
                    }
                }
            }

            offset += obj.code.len();
        }

        Ok(())
    }

    /// Link objects into executable
    pub fn link(&mut self, _output: &Path) -> Result<Vec<u8>, String> {
        // Build global symbol table
        self.build_symbol_table()?;

        // Assign addresses to symbols
        let mut offset = 0;
        for obj in &mut self.objects {
            for symbol in obj.symbols.symbols() {
                if let Some(sym) = self.global_symbols.get_symbol_mut(&symbol.mangled_name) {
                    sym.set_address(offset);
                }
            }
            offset += obj.code.len();
        }

        // Concatenate code sections
        let mut code = Vec::new();
        for obj in &self.objects {
            code.extend_from_slice(&obj.code);
        }

        // Apply relocations
        self.apply_relocations(&mut code)?;

        // Verify entry point exists
        if let Some(entry) = &self.entry_point {
            if self.global_symbols.get_symbol(entry).is_none() {
                return Err(format!("Entry point not found: {}", entry));
            }
        }

        Ok(code)
    }

    /// Link as library (no entry point required)
    pub fn link_library(&mut self, _output: &Path) -> Result<Vec<u8>, String> {
        // Build global symbol table
        self.build_symbol_table()?;

        // Concatenate code sections
        let mut code = Vec::new();
        for obj in &self.objects {
            code.extend_from_slice(&obj.code);
        }

        // Apply relocations
        self.apply_relocations(&mut code)?;

        Ok(code)
    }
}

impl Default for Linker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_creation() {
        let linker = Linker::new();
        assert_eq!(linker.objects.len(), 0);
    }

    #[test]
    fn test_object_file() {
        let mut obj = ObjectFile::new(PathBuf::from("test.o"));
        obj.add_code(vec![0x90, 0x90, 0x90]); // NOPs
        assert_eq!(obj.code.len(), 3);
    }
}
