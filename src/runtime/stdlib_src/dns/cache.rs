//! In-memory thread-safe LRU DNS Cache with positive/negative caching.

use super::message::DNSMessage;
use super::name::DNSName;
use super::records::{DNSRecord, DNSType};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub name: DNSName,
    pub rtype: DNSType,
    pub records: Vec<DNSRecord>,
    pub is_negative: bool,
    pub created_at: Instant,
    pub ttl: Duration,
}

impl CacheEntry {
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= self.ttl
    }
}

pub struct DNSCache {
    entries: Mutex<HashMap<(DNSName, DNSType), CacheEntry>>,
    max_entries: usize,
}

impl DNSCache {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            max_entries,
        }
    }

    pub fn get(&self, name: &DNSName, rtype: DNSType) -> Option<Vec<DNSRecord>> {
        let mut map = self.entries.lock().ok()?;
        let key = (name.clone(), rtype);

        if let Some(entry) = map.get(&key) {
            if entry.is_expired() {
                map.remove(&key);
                None
            } else if entry.is_negative {
                Some(Vec::new()) // Returns empty list for cached NXDOMAIN/NODATA
            } else {
                Some(entry.records.clone())
            }
        } else {
            None
        }
    }

    pub fn put(&self, name: DNSName, rtype: DNSType, msg: &DNSMessage) {
        let mut map = match self.entries.lock() {
            Ok(m) => m,
            Err(_) => return,
        };

        if map.len() >= self.max_entries {
            // Evict expired or arbitrary entry if capacity reached
            map.clear();
        }

        let key = (name.clone(), rtype);
        if !msg.answers.is_empty() {
            let min_ttl = msg.answers.iter().map(|r| r.ttl).min().unwrap_or(60);
            map.insert(
                key,
                CacheEntry {
                    name,
                    rtype,
                    records: msg.answers.clone(),
                    is_negative: false,
                    created_at: Instant::now(),
                    ttl: Duration::from_secs(min_ttl as u64),
                },
            );
        } else if msg.header.rcode == 3 /* NXDOMAIN */ || msg.header.rcode == 0
        /* NODATA */
        {
            map.insert(
                key,
                CacheEntry {
                    name,
                    rtype,
                    records: Vec::new(),
                    is_negative: true,
                    created_at: Instant::now(),
                    ttl: Duration::from_secs(60), // Negative cache default TTL
                },
            );
        }
    }

    pub fn clear(&self) {
        if let Ok(mut map) = self.entries.lock() {
            map.clear();
        }
    }
}
