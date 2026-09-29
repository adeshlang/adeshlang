//! Incremental Linking state management.

use crate::cache::LinkCache;
use crate::config::LinkConfig;
use crate::hash::Sha256;
use std::path::PathBuf;

/// State for incremental link validation.
#[derive(Debug, Clone)]
pub struct IncrementalState {
    pub config_hash: String,
    pub input_files: Vec<(PathBuf, String)>, // (path, sha256_hash)
}

impl IncrementalState {
    pub fn build(config: &LinkConfig, inputs: &[PathBuf]) -> Self {
        let mut cfg_hasher = Sha256::new();
        cfg_hasher.update(config.target.triple_string().as_bytes());
        cfg_hasher.update(config.effective_entry().as_bytes());
        cfg_hasher.update(if config.gc_sections { b"gc1" } else { b"gc0" });
        let config_hash = crate::hash::to_hex(&cfg_hasher.finalize());

        let mut input_files = Vec::new();
        for inp in inputs {
            if let Ok(bytes) = std::fs::read(inp) {
                let h = LinkCache::compute_input_hash(&bytes);
                input_files.push((inp.clone(), h));
            }
        }

        Self {
            config_hash,
            input_files,
        }
    }
}
