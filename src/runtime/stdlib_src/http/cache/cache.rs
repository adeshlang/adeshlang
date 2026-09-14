use super::super::request::Request;
use super::super::response::Response;
use super::single_flight::SingleFlight;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct CachedResponse {
    pub response: Response,
    pub cached_at: u64,
    pub max_age_secs: u64,
    pub etag: Option<String>,
}

impl CachedResponse {
    pub fn is_fresh(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        now < self.cached_at + self.max_age_secs
    }
}

#[derive(Clone, Default)]
pub struct HttpCache {
    storage: Arc<Mutex<HashMap<String, CachedResponse>>>,
    pub single_flight: SingleFlight,
}

impl HttpCache {
    pub fn new() -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
            single_flight: SingleFlight::new(),
        }
    }

    pub fn get(&self, req: &Request) -> Option<CachedResponse> {
        let key = self.cache_key(req);
        let map = self.storage.lock().ok()?;
        map.get(&key).cloned()
    }

    pub fn put(&self, req: &Request, resp: &Response) {
        if !req.method.is_safe() || !resp.status.is_cacheable() {
            return;
        }

        let cc = resp.headers.get("cache-control").unwrap_or("");
        if cc.contains("no-store") {
            return;
        }

        // Non-cacheable methods (for example QUERY) require explicit cache directives.
        let explicit_caching = parse_max_age(cc).is_some()
            || cc.to_ascii_lowercase().contains("public")
            || resp.headers.get("expires").is_some();
        if !req.method.is_cacheable() && !explicit_caching {
            return;
        }

        let max_age = parse_max_age(cc).unwrap_or(300); // default 5 min
        let etag = resp.headers.get("etag").map(|s| s.to_string());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cached = CachedResponse {
            response: resp.clone(),
            cached_at: now,
            max_age_secs: max_age,
            etag,
        };

        let key = self.cache_key(req);
        if let Ok(mut map) = self.storage.lock() {
            map.insert(key, cached);
        }
    }

    fn cache_key(&self, req: &Request) -> String {
        format!("{}:{}", req.method, req.uri)
    }
}

fn parse_max_age(cc: &str) -> Option<u64> {
    for part in cc.split(',') {
        let trimmed = part.trim();
        if trimmed.starts_with("max-age=") {
            return trimmed[8..].parse::<u64>().ok();
        }
    }
    None
}
