use crate::hash::{to_hex, Sha256};
use std::fs;
use std::path::PathBuf;

/// Persistent link cache store.
pub struct LinkCache {
    pub root_dir: PathBuf,
}

impl LinkCache {
    pub fn new(custom_dir: Option<PathBuf>) -> Self {
        let root = custom_dir.unwrap_or_else(|| PathBuf::from(".adesh").join("link-cache"));
        Self { root_dir: root }
    }

    pub fn compute_input_hash(file_bytes: &[u8]) -> String {
        let digest = Sha256::digest(file_bytes);
        to_hex(&digest)
    }

    pub fn ensure_cache_dir(&self) -> std::io::Result<()> {
        if !self.root_dir.exists() {
            fs::create_dir_all(&self.root_dir)?;
        }
        Ok(())
    }

    pub fn get_cached_file(&self, key: &str) -> Option<Vec<u8>> {
        let path = self.root_dir.join(key);
        fs::read(path).ok()
    }

    pub fn put_cached_file(&self, key: &str, data: &[u8]) -> std::io::Result<()> {
        self.ensure_cache_dir()?;
        let path = self.root_dir.join(key);
        fs::write(path, data)
    }
}
