//! AdeshLang Network Security Policy
//!
//! Enforces network policies including SSRF prevention, port filtering, and network sandboxing.

use super::ip::IPAddress;
use super::socket_addr::SocketAddress;
use crate::parsing::ast::{NativeFn, Value};
use crate::utils::collections::FastMap;
use std::sync::{Arc, RwLock};

#[derive(Debug, Clone)]
pub struct NetworkPolicy {
    pub allow_tcp: bool,
    pub allow_udp: bool,
    pub allow_ipv4: bool,
    pub allow_ipv6: bool,
    pub deny_private_networks: bool,
    pub deny_loopback: bool,
    pub allowed_ports: Vec<u16>,
    pub denied_ports: Vec<u16>,
    pub allowed_hosts: Vec<String>,
    pub denied_hosts: Vec<String>,
}

impl Default for NetworkPolicy {
    fn default() -> Self {
        Self {
            allow_tcp: true,
            allow_udp: true,
            allow_ipv4: true,
            allow_ipv6: true,
            deny_private_networks: false,
            deny_loopback: false,
            allowed_ports: Vec::new(),
            denied_ports: Vec::new(),
            allowed_hosts: Vec::new(),
            denied_hosts: Vec::new(),
        }
    }
}

impl NetworkPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn deny_private_networks(mut self) -> Self {
        self.deny_private_networks = true;
        self
    }

    pub fn deny_loopback(mut self) -> Self {
        self.deny_loopback = true;
        self
    }

    pub fn validate_address(&self, ip: &IPAddress) -> Result<(), String> {
        if ip.is_v4() && !self.allow_ipv4 {
            return Err("IPv4 connection denied by network policy".to_string());
        }
        if ip.is_v6() && !self.allow_ipv6 {
            return Err("IPv6 connection denied by network policy".to_string());
        }
        if self.deny_loopback && ip.is_loopback() {
            return Err("Loopback network connection denied by network policy".to_string());
        }
        if self.deny_private_networks && ip.is_private() {
            return Err(
                "Private network connection denied by network policy (SSRF protection)".to_string(),
            );
        }
        Ok(())
    }

    pub fn validate_socket_address(&self, addr: &SocketAddress) -> Result<(), String> {
        self.validate_address(&addr.ip)?;
        let port = addr.port;
        if self.denied_ports.contains(&port) {
            return Err(format!("Port {} explicitly denied by network policy", port));
        }
        if !self.allowed_ports.is_empty() && !self.allowed_ports.contains(&port) {
            return Err(format!(
                "Port {} is not in allowed ports list of network policy",
                port
            ));
        }
        Ok(())
    }

    pub fn to_value(&self) -> Value {
        let policy_arc = Arc::new(RwLock::new(self.clone()));
        build_policy_value(policy_arc)
    }
}

pub fn build_policy_value(policy_arc: Arc<RwLock<NetworkPolicy>>) -> Value {
    let mut map = FastMap::default();

    {
        if let Ok(p) = policy_arc.read() {
            map.insert("allowTcp".to_string(), Value::Bool(p.allow_tcp));
            map.insert("allowUdp".to_string(), Value::Bool(p.allow_udp));
            map.insert("allowIPv4".to_string(), Value::Bool(p.allow_ipv4));
            map.insert("allowIPv6".to_string(), Value::Bool(p.allow_ipv6));
            map.insert(
                "denyPrivateNetworks".to_string(),
                Value::Bool(p.deny_private_networks),
            );
            map.insert("denyLoopback".to_string(), Value::Bool(p.deny_loopback));
        }
    }

    let p1 = policy_arc.clone();
    map.insert(
        "allowTcp".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            if let Ok(mut guard) = p1.write() {
                guard.allow_tcp = true;
            }
            Ok(build_policy_value(p1.clone()))
        }))),
    );

    let p2 = policy_arc.clone();
    map.insert(
        "denyTcp".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            if let Ok(mut guard) = p2.write() {
                guard.allow_tcp = false;
            }
            Ok(build_policy_value(p2.clone()))
        }))),
    );

    let p3 = policy_arc.clone();
    map.insert(
        "allowUdp".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            if let Ok(mut guard) = p3.write() {
                guard.allow_udp = true;
            }
            Ok(build_policy_value(p3.clone()))
        }))),
    );

    let p4 = policy_arc.clone();
    map.insert(
        "denyUdp".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            if let Ok(mut guard) = p4.write() {
                guard.allow_udp = false;
            }
            Ok(build_policy_value(p4.clone()))
        }))),
    );

    let p5 = policy_arc.clone();
    map.insert(
        "denyPrivateNetworks".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            if let Ok(mut guard) = p5.write() {
                guard.deny_private_networks = true;
            }
            Ok(build_policy_value(p5.clone()))
        }))),
    );

    let p6 = policy_arc.clone();
    map.insert(
        "denyLoopback".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            if let Ok(mut guard) = p6.write() {
                guard.deny_loopback = true;
            }
            Ok(build_policy_value(p6.clone()))
        }))),
    );

    let p7 = policy_arc.clone();
    map.insert(
        "allowPort".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            if let Some(arg) = args.first() {
                let port = match arg {
                    Value::Number(n) => *n as u16,
                    Value::U16(p) => *p,
                    Value::U32(p) => *p as u16,
                    _ => 0,
                };
                if port > 0 {
                    if let Ok(mut guard) = p7.write() {
                        if !guard.allowed_ports.contains(&port) {
                            guard.allowed_ports.push(port);
                        }
                    }
                }
            }
            Ok(build_policy_value(p7.clone()))
        }))),
    );

    let p8 = policy_arc.clone();
    map.insert(
        "denyPort".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            if let Some(arg) = args.first() {
                let port = match arg {
                    Value::Number(n) => *n as u16,
                    Value::U16(p) => *p,
                    Value::U32(p) => *p as u16,
                    _ => 0,
                };
                if port > 0 {
                    if let Ok(mut guard) = p8.write() {
                        if !guard.denied_ports.contains(&port) {
                            guard.denied_ports.push(port);
                        }
                    }
                }
            }
            Ok(build_policy_value(p8.clone()))
        }))),
    );

    let p9 = policy_arc.clone();
    map.insert(
        "validate".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            if let Some(Value::Str(s)) = args.first() {
                if let Ok(ip) = IPAddress::parse(s) {
                    if let Ok(guard) = p9.read() {
                        guard.validate_address(&ip)?;
                    }
                    return Ok(Value::Bool(true));
                }
                if let Ok(sa) = SocketAddress::parse(s) {
                    if let Ok(guard) = p9.read() {
                        guard.validate_socket_address(&sa)?;
                    }
                    return Ok(Value::Bool(true));
                }
            }
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}
