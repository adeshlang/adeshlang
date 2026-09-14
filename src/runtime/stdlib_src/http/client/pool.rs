use super::super::http1::connection::Http1Connection;
use crate::utils::collections::FastMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PoolKey {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    pub alpn: Option<String>,
}

struct IdleConnection {
    conn: Http1Connection,
    idle_since: Instant,
}

#[derive(Clone, Default)]
pub struct ConnectionPool {
    pool: Arc<Mutex<FastMap<PoolKey, Vec<IdleConnection>>>>,
    max_idle_per_host: usize,
    idle_timeout: Duration,
}

impl ConnectionPool {
    pub fn new(max_idle_per_host: usize, idle_timeout: Duration) -> Self {
        Self {
            pool: Arc::new(Mutex::new(FastMap::default())),
            max_idle_per_host,
            idle_timeout,
        }
    }

    pub fn get(&self, key: &PoolKey) -> Option<Http1Connection> {
        let mut map = self.pool.lock().ok()?;
        let list = map.get_mut(key)?;
        while let Some(idle) = list.pop() {
            if idle.idle_since.elapsed() < self.idle_timeout {
                return Some(idle.conn);
            }
        }
        None
    }

    pub fn put(&self, key: PoolKey, conn: Http1Connection) {
        if let Ok(mut map) = self.pool.lock() {
            let list = map.entry(key).or_default();
            if list.len() < self.max_idle_per_host {
                list.push(IdleConnection {
                    conn,
                    idle_since: Instant::now(),
                });
            }
        }
    }
}
