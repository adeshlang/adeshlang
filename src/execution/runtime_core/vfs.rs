//! In-memory module source store (virtual filesystem for embedders).
//!
//! The WebAssembly target has no filesystem: every `std::fs` path probe
//! returns false and every read fails with "operation not supported". The
//! embedder (web editor, tests) registers module sources here before
//! execution so `import "..." as x;` statements resolve against a virtual
//! workspace instead of a disk.
//!
//! Resolution is intentionally tolerant about path shapes: keys are matched
//! after normalizing `\` to `/` and trimming a leading `./`, and bare
//! imports (no extension) try the language's module extensions.

use rustc_hash::FxHashMap as HashMap;
use std::cell::RefCell;
use std::path::Path;

thread_local! {
    static SOURCES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::default());
}

/// Module file extensions tried, in order, for extension-less imports.
const MODULE_EXTS: [&str; 3] = ["adesh", "adl", "ind"];

fn normalize(key: &str) -> String {
    key.replace('\\', "/")
        .trim_start_matches("./")
        .to_string()
}

/// Register (or replace) the source for a module path.
pub fn register(path: &str, source: String) {
    SOURCES.with(|s| {
        s.borrow_mut().insert(path.to_string(), source);
    });
}

/// Remove all registered module sources.
pub fn clear() {
    SOURCES.with(|s| s.borrow_mut().clear());
}

/// Number of registered module sources (for embedder sanity checks).
pub fn len() -> usize {
    SOURCES.with(|s| s.borrow().len())
}

/// Look up a registered source by exact (normalized) key. Returns the
/// registered key so read sites can fetch the source without a second scan.
fn lookup_key(key: &str) -> Option<String> {
    let norm = normalize(key);
    SOURCES.with(|s| {
        let map = s.borrow();
        for k in map.keys() {
            if normalize(k) == norm {
                return Some(k.clone());
            }
        }
        None
    })
}

/// Resolve an import against the virtual store the same way
/// `resolve_path` resolves against the disk: relative to the current
/// module, trying `.adesh`/`.adl`/`.ind` for extension-less imports.
/// Returns the registered key on success.
pub fn resolve_virtual(import: &str, module_id: &str) -> Option<String> {
    let base = Path::new(module_id).parent().unwrap_or(Path::new("."));
    let p = Path::new(import);

    let mut candidates: Vec<String> = Vec::new();
    let joined = base.join(p);
    let joined_str = joined.to_string_lossy().to_string();
    candidates.push(joined_str.clone());
    candidates.push(import.to_string());

    let had_ext = p.extension().and_then(|e| e.to_str()).is_some();
    if !had_ext {
        for ext in MODULE_EXTS {
            candidates.push(joined.with_extension(ext).to_string_lossy().to_string());
            candidates.push(format!("{import}.{ext}"));
        }
    }

    for c in &candidates {
        if let Some(k) = lookup_key(c) {
            return Some(k);
        }
    }

    // Last resort: match by file name against registered keys, so an
    // import of "math" resolves to a registered "libs/math.adesh".
    let file_name = p.file_name().and_then(|n| n.to_str()).unwrap_or(import);
    let stems: Vec<String> = if had_ext {
        vec![file_name.to_string()]
    } else {
        MODULE_EXTS
            .iter()
            .map(|e| format!("{file_name}.{e}"))
            .collect()
    };
    SOURCES.with(|s| {
        let map = s.borrow();
        for k in map.keys() {
            let kn = normalize(k);
            for stem in &stems {
                if kn == *stem || kn.ends_with(&format!("/{stem}")) {
                    return Some(k.clone());
                }
            }
        }
        None
    })
}

/// Read a module source: virtual store first, then the real filesystem.
///
/// Drop-in replacement for `std::fs::read_to_string` at import sites so
/// native behavior is unchanged while wasm resolves registered modules.
pub fn read_source(path: &str) -> Result<String, String> {
    if let Some(k) = lookup_key(path) {
        return SOURCES.with(|s| Ok(s.borrow().get(&k).cloned().unwrap_or_default()));
    }
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_relative_import_with_extension() {
        clear();
        register("helper.adesh", "fn help() {}".to_string());
        let key = resolve_virtual("./helper.adesh", "main.adesh");
        assert_eq!(key.as_deref(), Some("helper.adesh"));
        assert_eq!(read_source(&key.unwrap()).unwrap(), "fn help() {}");
        clear();
    }

    #[test]
    fn resolves_bare_import_by_extension() {
        clear();
        register("math.adesh", "fn add(a, b) { a + b }".to_string());
        let key = resolve_virtual("./math", "main.adesh");
        assert_eq!(key.as_deref(), Some("math.adesh"));
        clear();
    }

    #[test]
    fn resolves_by_filename_suffix() {
        clear();
        register("libs/math.adesh", "fn add(a, b) { a + b }".to_string());
        let key = resolve_virtual("./math", "main.adesh");
        assert_eq!(key.as_deref(), Some("libs/math.adesh"));
        clear();
    }

    #[test]
    fn misses_gracefully() {
        clear();
        assert_eq!(resolve_virtual("./nope", "main.adesh"), None);
        assert!(read_source("./nope.adesh").is_err());
    }
}
