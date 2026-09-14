use super::version::HttpVersion;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct HostPerformanceMetric {
    pub average_rtt: Duration,
    pub preferred_version: HttpVersion,
    pub supports_http3: bool,
    pub supports_http2: bool,
    pub success_count: u64,
}

#[derive(Clone, Default)]
pub struct TransportAutopilot {
    metrics: Arc<Mutex<HashMap<String, HostPerformanceMetric>>>,
}

impl TransportAutopilot {
    pub fn new() -> Self {
        Self {
            metrics: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn record_success(&self, host: &str, version: HttpVersion, rtt: Duration) {
        if let Ok(mut map) = self.metrics.lock() {
            let entry = map
                .entry(host.to_string())
                .or_insert_with(|| HostPerformanceMetric {
                    average_rtt: rtt,
                    preferred_version: version,
                    supports_http3: version == HttpVersion::Http30,
                    supports_http2: version == HttpVersion::Http20,
                    success_count: 0,
                });

            entry.success_count += 1;
            entry.average_rtt = (entry.average_rtt + rtt) / 2;
            if version == HttpVersion::Http30 {
                entry.supports_http3 = true;
                entry.preferred_version = HttpVersion::Http30;
            } else if version == HttpVersion::Http20 {
                entry.supports_http2 = true;
            }
        }
    }

    pub fn recommend_version(&self, host: &str) -> HttpVersion {
        if let Ok(map) = self.metrics.lock() {
            if let Some(metric) = map.get(host) {
                return metric.preferred_version;
            }
        }
        HttpVersion::Http11
    }
}
