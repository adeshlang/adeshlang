//! AdeshLang Socket Options
//!
//! Safe socket option configurations for TCP and UDP sockets.

use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug, Clone, Default)]
pub struct SocketOptions {
    pub reuse_address: Option<bool>,
    pub reuse_port: Option<bool>,
    pub keep_alive: Option<Option<Duration>>,
    pub no_delay: Option<bool>,
    pub broadcast: Option<bool>,
    pub receive_buffer_size: Option<usize>,
    pub send_buffer_size: Option<usize>,
    pub read_timeout: Option<Option<Duration>>,
    pub write_timeout: Option<Option<Duration>>,
}

impl SocketOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn to_value(&self) -> Value {
        let mut map = FastMap::default();

        if let Some(val) = self.reuse_address {
            map.insert("reuseAddress".to_string(), Value::Bool(val));
        }
        if let Some(val) = self.no_delay {
            map.insert("noDelay".to_string(), Value::Bool(val));
        }
        if let Some(val) = self.broadcast {
            map.insert("broadcast".to_string(), Value::Bool(val));
        }
        if let Some(val) = self.receive_buffer_size {
            map.insert("receiveBufferSize".to_string(), Value::Number(val as f64));
        }
        if let Some(val) = self.send_buffer_size {
            map.insert("sendBufferSize".to_string(), Value::Number(val as f64));
        }
        if let Some(ref dur_opt) = self.read_timeout {
            let secs = dur_opt.map(|d| d.as_secs_f64()).unwrap_or(0.0);
            map.insert("readTimeout".to_string(), Value::Number(secs));
        }
        if let Some(ref dur_opt) = self.write_timeout {
            let secs = dur_opt.map(|d| d.as_secs_f64()).unwrap_or(0.0);
            map.insert("writeTimeout".to_string(), Value::Number(secs));
        }

        Value::Object(Arc::new(map))
    }
}
