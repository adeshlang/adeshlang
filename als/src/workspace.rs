//! Workspace Index
//!
//! Manages semantic indices for all open files in the workspace. Provides
//! cross-file symbol resolution, module/import tracking, and workspace-wide
//! symbol search capabilities.

use std::collections::HashMap;
use lsp_types::Url;
use adeshlang::semantics::{index_source_in, SemanticIndex, SymbolEntry, TypeDefinition, TypeMember};

/// Manages semantic indices for all open files in the workspace.
pub struct WorkspaceIndex {
    /// Map of file URI → SemanticIndex
    indices: HashMap<Url, SemanticIndex>,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
        }
    }

    /// Index a document (called on open or change).
    pub fn index_document(&mut self, uri: &Url, content: &str) {
        let file_path = uri.to_file_path().ok().and_then(|p| p.to_str().map(|s| s.to_string()));
        let index = index_source_in(content, file_path.as_deref());
        self.indices.insert(uri.clone(), index);
    }

    /// Remove a document from the index (called on close).
    pub fn remove_document(&mut self, uri: &Url) {
        self.indices.remove(uri);
    }

    /// Get the semantic index for a document.
    pub fn get_index(&self, uri: &Url) -> Option<&SemanticIndex> {
        self.indices.get(uri)
    }

    /// Find a symbol declaration across the entire workspace.
    pub fn find_declaration_workspace(&self, name: &str) -> Option<(&Url, &SymbolEntry)> {
        for (uri, index) in &self.indices {
            if let Some(sym) = index.find_declaration(name) {
                return Some((uri, sym));
            }
        }
        None
    }

    /// Find the semantic index of the file that defines a type with this name.
    pub fn find_type_index(&self, name: &str) -> Option<&SemanticIndex> {
        self.indices.values().find(|index| index.get_type(name).is_some())
    }

    /// Find a type definition anywhere in the workspace.
    pub fn find_type_definition(&self, name: &str) -> Option<&TypeDefinition> {
        self.find_type_index(name).and_then(|index| index.get_type(name))
    }

    /// Get all members (fields, methods, variants) of a type defined anywhere
    /// in the workspace.
    pub fn get_type_members(&self, name: &str) -> Option<Vec<TypeMember>> {
        self.find_type_index(name).map(|index| index.get_type_members(name))
    }

    /// Find all references to a symbol across the workspace.
    pub fn find_references_workspace(&self, name: &str) -> Vec<(Url, usize, usize, usize)> {
        let mut results = Vec::new();
        for (uri, index) in &self.indices {
            for (line, col, end_col) in index.find_references_by_name(name) {
                results.push((uri.clone(), line, col, end_col));
            }
        }
        results
    }

    /// Search for workspace symbols matching a query string.
    pub fn workspace_symbols(&self, query: &str) -> Vec<(Url, &SymbolEntry)> {
        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (uri, index) in &self.indices {
            for sym in &index.symbols {
                // Only include top-level declarations in workspace search
                if sym.parent_type.is_some() {
                    continue;
                }
                if sym.name.to_lowercase().contains(&query_lower) {
                    results.push((uri.clone(), sym));
                }
            }
        }

        // Sort by name for consistent ordering
        results.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        results
    }

    /// Resolve a module import path to a file URI.
    pub fn resolve_module(&self, base_uri: &Url, module_path: &str) -> Option<Url> {
        // Try relative path resolution
        if let Ok(base_path) = base_uri.to_file_path() {
            let parent = base_path.parent()?;
            let module_file = parent.join(format!("{}.adesh", module_path));
            if module_file.exists() {
                return Url::from_file_path(&module_file).ok();
            }
            // Try as directory with index.adesh
            let module_dir = parent.join(module_path);
            let index_file = module_dir.join("index.adesh");
            if index_file.exists() {
                return Url::from_file_path(&index_file).ok();
            }
            // Try lib.adesh
            let lib_file = module_dir.join("lib.adesh");
            if lib_file.exists() {
                return Url::from_file_path(&lib_file).ok();
            }
        }
        None
    }

    /// Get all indexed document URIs.
    pub fn documents(&self) -> impl Iterator<Item = &Url> {
        self.indices.keys()
    }

    /// Get the number of indexed documents.
    pub fn document_count(&self) -> usize {
        self.indices.len()
    }
}

impl Default for WorkspaceIndex {
    fn default() -> Self {
        Self::new()
    }
}
