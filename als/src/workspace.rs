//! Workspace Index
//!
//! Manages semantic indices for open and workspace `.adesh` / `.vy` files.
//! Provides:
//! - Background/initial workspace folder scanning
//! - Content-hash caching to avoid redundant re-indexing
//! - Cross-file symbol, type, member, and implementation resolution
//! - Module path resolution & relative import path calculation for auto-imports
//! - Workspace-wide symbol search with relevance ranking

use adeshlang::parsing::ast::StmtKind;
use adeshlang::semantics::{
    index_source_in, SemanticIndex, SymbolEntry, TypeDefinition, TypeMember,
};
use lsp_types::Url;
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};

/// Directories skipped when scanning a workspace for Adesh files.
const IGNORED_DIRS: &[&str] = &[
    ".git",
    "target",
    "target-workspace-tests",
    "node_modules",
    "dist",
    "out",
    ".adesh_cache",
    ".adl",
    ".vscode",
    "__pycache__",
    ".pytest_cache",
];

/// Maximum workspace files automatically scanned at startup to keep indexing snappy.
const MAX_WORKSPACE_FILES: usize = 500;

/// Manages semantic indices for all known files in the workspace.
pub struct WorkspaceIndex {
    /// Map of file URI -> SemanticIndex
    indices: HashMap<Url, SemanticIndex>,
    /// Content hash per URI so unchanged documents aren't needlessly re-parsed
    content_hashes: HashMap<Url, u64>,
    /// Open document URIs (never dropped from disk-refresh unless closed)
    open_docs: std::collections::HashSet<Url>,
    /// Workspace root paths
    roots: Vec<PathBuf>,
}

impl WorkspaceIndex {
    pub fn new() -> Self {
        Self {
            indices: HashMap::new(),
            content_hashes: HashMap::new(),
            open_docs: std::collections::HashSet::new(),
            roots: Vec::new(),
        }
    }

    fn hash_content(content: &str) -> u64 {
        let mut h = DefaultHasher::new();
        content.hash(&mut h);
        h.finish()
    }

    /// Register a workspace root and scan `.adesh`/`.vy` files underneath it.
    pub fn add_workspace_root(&mut self, root: PathBuf) {
        if !self.roots.contains(&root) {
            self.roots.push(root.clone());
        }
        self.scan_directory(&root);
    }

    /// Remove a workspace root.
    pub fn remove_workspace_root(&mut self, root: &Path) {
        self.roots.retain(|r| r != root);
        let removed: Vec<_> = self
            .indices
            .keys()
            .filter(|uri| {
                !self.open_docs.contains(*uri)
                    && uri.to_file_path().is_ok_and(|path| {
                        path.starts_with(root) && !self.roots.iter().any(|r| path.starts_with(r))
                    })
            })
            .cloned()
            .collect();
        for uri in removed {
            self.remove_document(&uri);
        }
    }

    /// Return configured workspace roots.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Scan a directory recursively for `.adesh` / `.vy` files and index them.
    pub fn scan_directory(&mut self, root: &Path) {
        let mut stack = vec![root.to_path_buf()];
        let mut indexed = 0usize;

        while let Some(dir) = stack.pop() {
            if indexed >= MAX_WORKSPACE_FILES {
                break;
            }
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if indexed >= MAX_WORKSPACE_FILES {
                    break;
                }
                let path = entry.path();
                if entry.file_type().is_ok_and(|kind| kind.is_symlink()) {
                    continue;
                }
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if IGNORED_DIRS.contains(&name) || name.starts_with("target-") {
                            continue;
                        }
                    }
                    stack.push(path);
                } else if is_adesh_path(&path) {
                    if let Ok(uri) = Url::from_file_path(&path) {
                        if self.open_docs.contains(&uri) {
                            continue;
                        }
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            self.index_document(&uri, &content);
                            indexed += 1;
                        }
                    }
                }
            }
        }
    }

    /// Mark a document as open and index it if needed.
    pub fn open_document(&mut self, uri: &Url, content: &str) {
        self.open_docs.insert(uri.clone());
        self.index_document(uri, content);
    }

    /// Index a document (skips work if content hash is unchanged).
    pub fn index_document(&mut self, uri: &Url, content: &str) {
        let hash = Self::hash_content(content);
        if self.content_hashes.get(uri) == Some(&hash) && self.indices.contains_key(uri) {
            return;
        }
        let file_path = uri
            .to_file_path()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()));
        let index = index_source_in(content, file_path.as_deref());
        self.indices.insert(uri.clone(), index);
        self.content_hashes.insert(uri.clone(), hash);
    }

    /// Index a file directly from disk (e.g., on file-watcher change or on-demand import).
    pub fn index_file_from_disk(&mut self, uri: &Url) -> bool {
        if self.open_docs.contains(uri) {
            return true;
        }
        if let Ok(path) = uri.to_file_path() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                self.index_document(uri, &content);
                return true;
            }
        }
        false
    }

    /// Close a document. If it exists on disk inside a workspace root, keep its disk index;
    /// otherwise remove it from the index.
    pub fn close_document(&mut self, uri: &Url) {
        self.open_docs.remove(uri);
        if let Ok(path) = uri.to_file_path() {
            if path.exists() && self.roots.iter().any(|r| path.starts_with(r)) {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    self.index_document(uri, &content);
                    return;
                }
            }
        }
        self.remove_document(uri);
    }

    /// Remove a document from the index.
    pub fn remove_document(&mut self, uri: &Url) {
        self.open_docs.remove(uri);
        self.indices.remove(uri);
        self.content_hashes.remove(uri);
    }

    /// Get the semantic index for a document.
    pub fn get_index(&self, uri: &Url) -> Option<&SemanticIndex> {
        self.indices.get(uri)
    }

    /// Iterate over all `(uri, index)` pairs in the workspace.
    pub fn iter_indices(&self) -> impl Iterator<Item = (&Url, &SemanticIndex)> {
        self.indices.iter()
    }

    /// Find a symbol declaration across the entire workspace.
    pub fn find_declaration_workspace(&self, name: &str) -> Option<(&Url, &SymbolEntry)> {
        // Prefer exported declarations first
        for (uri, index) in &self.indices {
            if let Some(sym) = index.find_declaration(name) {
                if sym.exported || index.exports.contains(name) {
                    return Some((uri, sym));
                }
            }
        }
        for (uri, index) in &self.indices {
            if let Some(sym) = index.find_declaration(name) {
                return Some((uri, sym));
            }
        }
        None
    }

    /// Find all candidate files that declare/export `name` (for auto-imports), excluding `current_uri`.
    pub fn find_export_candidates(
        &self,
        name: &str,
        current_uri: &Url,
    ) -> Vec<(&Url, &SymbolEntry)> {
        let mut results = Vec::new();
        for (uri, index) in &self.indices {
            if uri == current_uri {
                continue;
            }
            for sym in &index.symbols {
                if sym.name == name && sym.parent_type.is_none() {
                    if sym.exported
                        || index.exports.contains(name)
                        || sym.kind.is_type()
                        || sym.kind == adeshlang::semantics::SemanticSymbolKind::Function
                    {
                        results.push((uri, sym));
                        break;
                    }
                }
            }
        }
        results
    }

    /// Find all exported/top-level symbols in other workspace files matching a prefix (for auto-import completions).
    pub fn find_auto_import_symbols(
        &self,
        current_uri: &Url,
        prefix: &str,
    ) -> Vec<(&Url, &SymbolEntry)> {
        let prefix_lower = prefix.to_lowercase();
        let mut out = Vec::new();
        for (uri, index) in &self.indices {
            if uri == current_uri {
                continue;
            }
            for sym in &index.symbols {
                if sym.parent_type.is_some() {
                    continue;
                }
                let eligible = sym.exported
                    || index.exports.contains(&sym.name)
                    || sym.kind.is_type()
                    || sym.kind == adeshlang::semantics::SemanticSymbolKind::Function;
                if !eligible {
                    continue;
                }
                if prefix_lower.is_empty() || sym.name.to_lowercase().starts_with(&prefix_lower) {
                    out.push((uri, sym));
                }
            }
        }
        out
    }

    /// Find the semantic index of the file that defines a type with this name.
    pub fn find_type_index(&self, name: &str) -> Option<&SemanticIndex> {
        self.indices
            .values()
            .find(|index| index.get_type(name).is_some())
    }

    /// Find a type definition along with its URI anywhere in the workspace.
    pub fn find_type_location(&self, name: &str) -> Option<(&Url, &TypeDefinition)> {
        for (uri, index) in &self.indices {
            if let Some(td) = index.get_type(name) {
                return Some((uri, td));
            }
        }
        None
    }

    /// Find a type definition anywhere in the workspace.
    pub fn find_type_definition(&self, name: &str) -> Option<&TypeDefinition> {
        self.find_type_index(name)
            .and_then(|index| index.get_type(name))
    }

    /// Find the exact declaration location `(uri, line_1based, col_1based, len)` of a member
    /// on `type_name` (including fields, methods, static methods, enum variants, and inherited members).
    pub fn find_member_location(
        &self,
        type_name: &str,
        member_name: &str,
    ) -> Option<(Url, usize, usize, usize)> {
        let mut visited = std::collections::HashSet::new();
        let mut current = Some(type_name.to_string());

        while let Some(ty_name) = current {
            if !visited.insert(ty_name.clone()) {
                break;
            }
            // Check symbol entries with parent_type == Some(ty_name) across workspace
            for (uri, index) in &self.indices {
                for sym in &index.symbols {
                    if sym.parent_type.as_deref() == Some(ty_name.as_str())
                        && sym.name == member_name
                        && sym.line > 0
                    {
                        return Some((uri.clone(), sym.line, sym.col, sym.name.len()));
                    }
                }
                // Check TypeDefinition methods/fields/variants directly
                if let Some(td) = index.get_type(&ty_name) {
                    for m in td.methods.iter().chain(td.static_methods.iter()) {
                        if m.name == member_name && m.line > 0 {
                            return Some((uri.clone(), m.line, m.col, m.name.len()));
                        }
                    }
                    // Fallback: locate member identifier token after type declaration line
                    let is_field_or_variant = td.fields.iter().any(|(f, _, _)| f == member_name)
                        || td.variants.iter().any(|(v, _)| v == member_name)
                        || td.static_properties.iter().any(|(p, _)| p == member_name);
                    if is_field_or_variant {
                        for tok in &index.tokens {
                            if tok.line >= td.line && tok.lexeme == member_name {
                                return Some((uri.clone(), tok.line, tok.col, tok.lexeme.len()));
                            }
                        }
                    }
                }
            }

            current = self
                .find_type_definition(&ty_name)
                .and_then(|td| td.extends.clone());
        }

        None
    }

    /// Find all implementations of an interface or subclasses/extensions of a class/struct across the workspace.
    /// Returns `(uri, line_1based, col_1based, name)`.
    pub fn find_implementations(&self, target_name: &str) -> Vec<(Url, usize, usize, String)> {
        let mut results = Vec::new();
        for (uri, index) in &self.indices {
            for td in index.types.values() {
                let implements_target = td.implements.iter().any(|i| i == target_name);
                let extends_target = td.extends.as_deref() == Some(target_name);
                if (implements_target || extends_target) && td.line > 0 {
                    results.push((uri.clone(), td.line, td.col, td.name.clone()));
                }
            }
            // Also check `extend on Target` / `extend on X : Target`
            for stmt in &index.statements {
                if let StmtKind::Extend(_, target_ty, interfaces, _, _) = &stmt.kind {
                    if target_ty == target_name || interfaces.iter().any(|i| i == target_name) {
                        results.push((
                            uri.clone(),
                            stmt.span.line,
                            stmt.span.col,
                            format!("extend on {}", target_ty),
                        ));
                    }
                }
            }
        }
        results
    }

    /// Find all overriding/implementing methods for `method_name` in subclasses or interface implementors of `owner_type`.
    pub fn find_method_implementations(
        &self,
        owner_type: &str,
        method_name: &str,
    ) -> Vec<(Url, usize, usize, usize)> {
        let mut results = Vec::new();
        let impls = self.find_implementations(owner_type);
        for (_, _, _, impl_name) in impls {
            if let Some(loc) = self.find_member_location(&impl_name, method_name) {
                results.push(loc);
            }
        }
        results
    }

    /// Get all members (fields, methods, variants) of a type defined anywhere in the workspace.
    pub fn get_type_members(&self, name: &str) -> Option<Vec<TypeMember>> {
        self.find_type_index(name)
            .map(|index| index.get_type_members(name))
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

    /// Search for workspace symbols matching a query string (with prefix/fuzzy relevance ranking).
    pub fn workspace_symbols(&self, query: &str) -> Vec<(Url, &SymbolEntry)> {
        let query_lower = query.to_lowercase();
        let mut scored: Vec<(u8, Url, &SymbolEntry)> = Vec::new();

        for (uri, index) in &self.indices {
            for sym in &index.symbols {
                if sym.kind == adeshlang::semantics::SemanticSymbolKind::Parameter
                    || sym.kind == adeshlang::semantics::SemanticSymbolKind::Import
                {
                    continue;
                }
                if query_lower.is_empty() {
                    if sym.parent_type.is_none() {
                        scored.push((2, uri.clone(), sym));
                    }
                    continue;
                }
                let name_lower = sym.name.to_lowercase();
                let rank = if name_lower == query_lower {
                    Some(0)
                } else if name_lower.starts_with(&query_lower) {
                    Some(1)
                } else if name_lower.contains(&query_lower) {
                    Some(2)
                } else if fuzzy_subsequence_match(&name_lower, &query_lower) {
                    Some(3)
                } else {
                    None
                };
                if let Some(r) = rank {
                    scored.push((r, uri.clone(), sym));
                }
            }
        }

        scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.2.name.cmp(&b.2.name)));
        scored.into_iter().map(|(_, u, s)| (u, s)).collect()
    }

    /// Resolve a module import path to a file URI.
    pub fn resolve_module(&self, base_uri: &Url, module_path: &str) -> Option<Url> {
        let clean = module_path.trim().trim_matches('"').trim_matches('\'');
        if clean.is_empty() {
            return None;
        }

        if let Ok(base_path) = base_uri.to_file_path() {
            if let Some(parent) = base_path.parent() {
                if let Some(found) = try_resolve_in_dir(parent, clean) {
                    return Url::from_file_path(found).ok();
                }
            }
        }

        // Try workspace roots and their `src/` / `lib/` subdirectories
        for root in &self.roots {
            if let Some(found) = try_resolve_in_dir(root, clean) {
                return Url::from_file_path(found).ok();
            }
            for sub in ["src", "lib", "src/stdlib"] {
                let dir = root.join(sub);
                if dir.is_dir() {
                    if let Some(found) = try_resolve_in_dir(&dir, clean) {
                        return Url::from_file_path(found).ok();
                    }
                }
            }
        }

        // Match against already-indexed workspace documents by stem
        let wanted = clean.trim_start_matches("./").trim_end_matches(".adesh");
        for uri in self.indices.keys() {
            if let Ok(p) = uri.to_file_path() {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if stem == wanted {
                        return Some(uri.clone());
                    }
                }
            }
        }

        None
    }

    /// Compute a clean relative import specifier from `from_uri` to `to_uri` (e.g., `"./models"`).
    pub fn relative_import_path(&self, from_uri: &Url, to_uri: &Url) -> Option<String> {
        let from_path = from_uri.to_file_path().ok()?;
        let to_path = to_uri.to_file_path().ok()?;
        let from_dir = from_path.parent()?;

        let rel = diff_paths(&to_path, from_dir)?;
        let mut rel_str = rel.to_string_lossy().replace('\\', "/");
        if let Some(stripped) = rel_str.strip_suffix(".adesh") {
            rel_str = stripped.to_string();
        } else if let Some(stripped) = rel_str.strip_suffix(".vy") {
            rel_str = stripped.to_string();
        }
        if !rel_str.starts_with('.') {
            rel_str = format!("./{}", rel_str);
        }
        Some(rel_str)
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

fn is_adesh_path(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("adesh" | "vy")
    )
}

fn try_resolve_in_dir(dir: &Path, module_path: &str) -> Option<PathBuf> {
    let direct = dir.join(module_path);
    if direct.is_file() {
        return Some(direct);
    }
    let with_adesh = dir.join(format!("{}.adesh", module_path));
    if with_adesh.is_file() {
        return Some(with_adesh);
    }
    let with_vy = dir.join(format!("{}.vy", module_path));
    if with_vy.is_file() {
        return Some(with_vy);
    }
    if direct.is_dir() {
        for candidate in ["index.adesh", "mod.adesh", "lib.adesh"] {
            let file = direct.join(candidate);
            if file.is_file() {
                return Some(file);
            }
        }
    }
    None
}

fn fuzzy_subsequence_match(haystack: &str, needle: &str) -> bool {
    let mut h_chars = haystack.chars();
    for nc in needle.chars() {
        if !h_chars.any(|hc| hc == nc) {
            return false;
        }
    }
    true
}

fn diff_paths(path: &Path, base: &Path) -> Option<PathBuf> {
    let ita = path.components().collect::<Vec<_>>();
    let itb = base.components().collect::<Vec<_>>();
    let mut i = 0;
    while i < ita.len() && i < itb.len() && ita[i] == itb[i] {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let mut comps = Vec::new();
    for comp in &itb[i..] {
        if matches!(comp, Component::Normal(_)) {
            comps.push(Component::ParentDir);
        }
    }
    for comp in &ita[i..] {
        comps.push(*comp);
    }
    Some(comps.iter().collect())
}
