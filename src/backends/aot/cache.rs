//! Incremental Compilation Cache Engine for AdeshLang AOT Backend
//!
//! Provides fast hashing, artifact caching (.obj and binaries), and sub-millisecond
//! incremental compilation checks to eliminate redundant compilation and linking steps.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::cranelift::AotOptions;

/// Metadata stored alongside cached compilation outputs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CacheEntryMeta {
    pub hash: String,
    pub source_path: Option<String>,
    pub output_path: String,
    pub opt_level: u8,
    pub fast_compile: bool,
    pub output_format: String,
    pub target_triple: Option<String>,
    pub debug_info: bool,
    pub timestamp_secs: u64,
    pub output_file_size: u64,
}

/// Result of checking the compilation cache
#[derive(Debug, PartialEq, Eq)]
pub enum CacheLookupResult {
    /// Output file exists and is completely up to date with identical source & options.
    /// Compilation and linking can be completely skipped.
    FullHit {
        output_path: PathBuf,
        cache_hash: String,
    },
    /// The compiled object file is cached, so frontend (parsing/typechecking/IR/codegen)
    /// can be skipped and only linking is required.
    ObjectHit {
        cached_obj_path: PathBuf,
        cache_hash: String,
    },
    /// No cache match; full compilation is needed.
    Miss { cache_hash: String },
}

/// Compilation cache manager
#[derive(Debug, Clone)]
pub struct AotCompilationCache {
    cache_dir: PathBuf,
    enabled: bool,
}

impl AotCompilationCache {
    /// Create a cache manager for the default project or global cache directory
    pub fn new(custom_dir: Option<PathBuf>, enabled: bool) -> Self {
        let cache_dir = custom_dir.unwrap_or_else(Self::default_cache_dir);
        Self { cache_dir, enabled }
    }

    /// Compute the default cache directory (prefer `.adesh_cache/aot` in project/cwd)
    pub fn default_cache_dir() -> PathBuf {
        let current = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        current.join(".adesh_cache").join("aot")
    }

    /// Check if incremental caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Compute a unique hash signature for source code and compilation options
    pub fn compute_hash(src: &str, options: &AotOptions, extra_inputs: Option<&[&str]>) -> String {
        let mut hasher = DefaultHasher::new();

        // 1. Compiler version / build discriminator
        "adeshlang_aot_v0.3.0".hash(&mut hasher);
        if let Ok(exe) = std::env::current_exe() {
            if let Ok(m) = fs::metadata(&exe) {
                if let Ok(modified) = m.modified() {
                    if let Ok(d) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                        d.as_secs().hash(&mut hasher);
                    }
                }
            }
        }

        // 2. Source content
        src.hash(&mut hasher);

        // 3. Any extra inputs (e.g. imported modules)
        if let Some(extras) = extra_inputs {
            for item in extras {
                item.hash(&mut hasher);
            }
        }

        // 4. Compilation options
        options.opt_level.hash(&mut hasher);
        options.fast_compile.hash(&mut hasher);
        format!("{:?}", options.output_format).hash(&mut hasher);
        options.target_triple.hash(&mut hasher);
        options.debug_info.hash(&mut hasher);
        options.library_mode.hash(&mut hasher);
        options.enable_dead_code_elimination.hash(&mut hasher);
        options.enable_lto.hash(&mut hasher);
        options.include_dirs.hash(&mut hasher);
        options.lib_dirs.hash(&mut hasher);
        options.link_libs.hash(&mut hasher);
        options.extra_linker_args.hash(&mut hasher);

        // Key-value flags
        let mut sorted_flags: Vec<_> = options.flags.iter().collect();
        sorted_flags.sort_by_key(|(k, _)| *k);
        for (k, v) in sorted_flags {
            k.hash(&mut hasher);
            v.hash(&mut hasher);
        }

        format!("{:016x}", hasher.finish())
    }

    /// Ensure cache directory exists on disk
    fn ensure_cache_dir(&self) -> std::io::Result<()> {
        if !self.cache_dir.exists() {
            fs::create_dir_all(&self.cache_dir)?;
        }
        Ok(())
    }

    /// Path to cached object file for a hash
    pub fn cached_obj_path(&self, hash: &str) -> PathBuf {
        let ext = if cfg!(windows) { "obj" } else { "o" };
        self.cache_dir.join(format!("{}.{}", hash, ext))
    }

    /// Path to cached metadata JSON for a hash
    pub fn cached_meta_path(&self, hash: &str) -> PathBuf {
        self.cache_dir.join(format!("{}.meta.json", hash))
    }

    /// Lookup cache for given source, options, and desired output path
    pub fn query(
        &self,
        src: &str,
        options: &AotOptions,
        output_path: &Path,
        extra_inputs: Option<&[&str]>,
    ) -> CacheLookupResult {
        if !self.enabled {
            let hash = Self::compute_hash(src, options, extra_inputs);
            return CacheLookupResult::Miss { cache_hash: hash };
        }

        let hash = Self::compute_hash(src, options, extra_inputs);
        let meta_path = self.cached_meta_path(&hash);
        let cached_obj = self.cached_obj_path(&hash);

        // Check if metadata exists
        if let Ok(meta_content) = fs::read_to_string(&meta_path) {
            if let Ok(meta) = serde_json::from_str::<CacheEntryMeta>(&meta_content) {
                // Verify if output artifact exists and is not empty
                if output_path.exists() {
                    if let Ok(file_meta) = fs::metadata(output_path) {
                        if file_meta.len() > 0 && file_meta.len() == meta.output_file_size {
                            return CacheLookupResult::FullHit {
                                output_path: output_path.to_path_buf(),
                                cache_hash: hash,
                            };
                        }
                    }
                }

                // If output file is missing or size mismatch, but cached object exists:
                if cached_obj.exists() {
                    if let Ok(obj_meta) = fs::metadata(&cached_obj) {
                        if obj_meta.len() > 0 {
                            return CacheLookupResult::ObjectHit {
                                cached_obj_path: cached_obj,
                                cache_hash: hash,
                            };
                        }
                    }
                }
            }
        }

        // Even without metadata, check if cached object file alone exists
        if cached_obj.exists() {
            if let Ok(obj_meta) = fs::metadata(&cached_obj) {
                if obj_meta.len() > 0 {
                    return CacheLookupResult::ObjectHit {
                        cached_obj_path: cached_obj,
                        cache_hash: hash,
                    };
                }
            }
        }

        CacheLookupResult::Miss { cache_hash: hash }
    }

    /// Save compiled object file into the cache
    pub fn store_object(&self, hash: &str, obj_bytes: &[u8]) -> Result<PathBuf, String> {
        if !self.enabled {
            return Ok(self.cached_obj_path(hash));
        }

        self.ensure_cache_dir()
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;

        let cached_obj = self.cached_obj_path(hash);
        fs::write(&cached_obj, obj_bytes).map_err(|e| {
            format!(
                "Failed to write cached object file {}: {}",
                cached_obj.display(),
                e
            )
        })?;

        Ok(cached_obj)
    }

    /// Save metadata after successful build and linking
    pub fn store_success(
        &self,
        hash: &str,
        source_path: Option<&Path>,
        output_path: &Path,
        options: &AotOptions,
    ) -> Result<(), String> {
        if !self.enabled {
            return Ok(());
        }

        self.ensure_cache_dir()
            .map_err(|e| format!("Failed to create cache directory: {}", e))?;

        let output_size = fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let meta = CacheEntryMeta {
            hash: hash.to_string(),
            source_path: source_path.map(|p| p.to_string_lossy().to_string()),
            output_path: output_path.to_string_lossy().to_string(),
            opt_level: options.opt_level,
            fast_compile: options.fast_compile,
            output_format: format!("{:?}", options.output_format),
            target_triple: options.target_triple.clone(),
            debug_info: options.debug_info,
            timestamp_secs: now,
            output_file_size: output_size,
        };

        if let Ok(json_str) = serde_json::to_string_pretty(&meta) {
            let meta_path = self.cached_meta_path(hash);
            let _ = fs::write(&meta_path, json_str);
        }

        Ok(())
    }

    /// Clear all cached compilation artifacts
    pub fn clear(&self) -> Result<usize, String> {
        if !self.cache_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if fs::remove_file(&path).is_ok() {
                        count += 1;
                    }
                }
            }
        }

        Ok(count)
    }
}

impl Default for AotCompilationCache {
    fn default() -> Self {
        Self::new(None, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_consistency() {
        let src = "fn main() { return 42; }";
        let opt1 = AotOptions::default();
        let mut opt2 = AotOptions::default();

        let hash1 = AotCompilationCache::compute_hash(src, &opt1, None);
        let hash2 = AotCompilationCache::compute_hash(src, &opt1, None);
        assert_eq!(hash1, hash2);

        // Different opt level produces different hash
        opt2.opt_level = 0;
        let hash3 = AotCompilationCache::compute_hash(src, &opt2, None);
        assert_ne!(hash1, hash3);

        // Different source produces different hash
        let hash4 = AotCompilationCache::compute_hash("fn main() { return 0; }", &opt1, None);
        assert_ne!(hash1, hash4);
    }

    #[test]
    fn test_cache_query_miss_and_store() {
        let temp_dir = std::env::temp_dir().join("adesh_test_cache_1");
        let cache = AotCompilationCache::new(Some(temp_dir.clone()), true);

        let src = "let x = 100;";
        let opt = AotOptions::default();
        let out_path = temp_dir.join("test_out.exe");

        let res = cache.query(src, &opt, &out_path, None);
        match res {
            CacheLookupResult::Miss { cache_hash } => {
                let stored_obj = cache
                    .store_object(&cache_hash, b"dummy object bytes")
                    .unwrap();
                assert!(stored_obj.exists());
            }
            _ => panic!("Expected cache miss"),
        }

        // Now query again: should be ObjectHit
        let res2 = cache.query(src, &opt, &out_path, None);
        match res2 {
            CacheLookupResult::ObjectHit {
                cached_obj_path, ..
            } => {
                assert!(cached_obj_path.exists());
            }
            _ => panic!("Expected ObjectHit"),
        }

        let _ = cache.clear();
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
