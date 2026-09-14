use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct AltSvcEntry {
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Default)]
pub struct AltSvcCache {
    entries: Arc<Mutex<HashMap<String, Vec<AltSvcEntry>>>>,
}

impl AltSvcCache {
    pub fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn insert(&self, origin: &str, protocol: &str, host: &str, port: u16, max_age_secs: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let entry = AltSvcEntry {
            protocol: protocol.to_string(),
            host: host.to_string(),
            port,
            expires_at: now + max_age_secs,
        };
        if let Ok(mut map) = self.entries.lock() {
            map.entry(origin.to_string()).or_default().push(entry);
        }
    }

    pub fn get_h3_alt_service(&self, origin: &str) -> Option<(String, u16)> {
        let map = self.entries.lock().ok()?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if let Some(list) = map.get(origin) {
            for item in list {
                if item.expires_at > now && (item.protocol == "h3" || item.protocol == "h3-29") {
                    return Some((item.host.clone(), item.port));
                }
            }
        }
        None
    }
}
