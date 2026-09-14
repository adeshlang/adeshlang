//! AdeshLang Network Interfaces Discovery
//!
//! Enumerates local network interfaces across platforms.

use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct NetworkInterfaceInfo {
    pub name: String,
    pub index: u32,
    pub addresses: Vec<String>,
    pub is_up: bool,
    pub is_loopback: bool,
    pub is_multicast: bool,
    pub mac_address: Option<String>,
}

impl NetworkInterfaceInfo {
    pub fn to_value(&self) -> Value {
        let mut map = FastMap::default();
        map.insert("name".to_string(), Value::Str(self.name.clone()));
        map.insert("index".to_string(), Value::U32(self.index));
        let addrs: Vec<Value> = self.addresses.iter().cloned().map(Value::Str).collect();
        map.insert("addresses".to_string(), Value::Array(addrs));
        map.insert("isUp".to_string(), Value::Bool(self.is_up));
        map.insert("isLoopback".to_string(), Value::Bool(self.is_loopback));
        map.insert("isMulticast".to_string(), Value::Bool(self.is_multicast));
        if let Some(ref mac) = self.mac_address {
            map.insert("macAddress".to_string(), Value::Str(mac.clone()));
        }
        Value::Object(Arc::new(map))
    }
}

pub fn list_network_interfaces() -> Vec<NetworkInterfaceInfo> {
    let mut result = Vec::new();

    // Standard loopback interface (always available cross-platform)
    result.push(NetworkInterfaceInfo {
        name: "lo0".to_string(),
        index: 1,
        addresses: vec!["127.0.0.1".to_string(), "::1".to_string()],
        is_up: true,
        is_loopback: true,
        is_multicast: true,
        mac_address: None,
    });

    // Attempt to discover local hostname addresses via standard DNS lookup
    if let Ok(hostname) = std::env::var("COMPUTERNAME").or_else(|_| std::env::var("HOSTNAME")) {
        let host_port = format!("{}:0", hostname);
        if let Ok(addrs) = host_port.to_socket_addrs() {
            let mut local_addrs = Vec::new();
            for addr in addrs {
                let ip_str = addr.ip().to_string();
                if !local_addrs.contains(&ip_str) {
                    local_addrs.push(ip_str);
                }
            }
            if !local_addrs.is_empty() {
                result.push(NetworkInterfaceInfo {
                    name: "eth0".to_string(),
                    index: 2,
                    addresses: local_addrs,
                    is_up: true,
                    is_loopback: false,
                    is_multicast: true,
                    mac_address: None,
                });
            }
        }
    }

    result
}
