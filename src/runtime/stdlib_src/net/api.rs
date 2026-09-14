//! AdeshLang Net Standard Library API Registration
//!
//! Exposes top-level functions, constructors, and classes for the `Net` module.

use super::interface::list_network_interfaces;
use super::ip::IPAddress;
use super::policy::NetworkPolicy;
use super::socket_addr::SocketAddress;
use super::tcp::{tcp_connect, tcp_listen};
use super::udp::udp_bind;
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::net::ToSocketAddrs;
use std::sync::Arc;

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "net_tcp_connect",
        "net",
        "Connect to a TCP host and port",
        builtin_tcp_connect,
    );
    registry.register(
        "net_tcp_listen",
        "net",
        "Listen for TCP connections on a host and port",
        builtin_tcp_listen,
    );
    registry.register(
        "net_udp_bind",
        "net",
        "Bind a UDP socket on a host and port",
        builtin_udp_bind,
    );
    registry.register(
        "net_ip_parse",
        "net",
        "Parse IP address string into IPAddress object",
        builtin_ip_parse,
    );
    registry.register(
        "net_socket_address_parse",
        "net",
        "Parse formatted string into SocketAddress object",
        builtin_socket_address_parse,
    );
    registry.register(
        "net_interfaces",
        "net",
        "Enumerate network interfaces",
        builtin_interfaces,
    );
    registry.register(
        "net_resolve",
        "net",
        "Resolve hostname to list of IP addresses",
        builtin_resolve,
    );
}

fn builtin_tcp_connect(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("tcpConnect requires (host, [port])".to_string());
    }
    let host = match &args[0] {
        Value::Str(s) => s.as_str(),
        Value::Object(obj) => {
            if let Some(Value::Str(h)) = obj.get("hostname").or_else(|| obj.get("host")) {
                h.as_str()
            } else if let Some(Value::Str(addr)) = obj.get("address") {
                addr.as_str()
            } else {
                return Err("Invalid object parameter to tcpConnect".to_string());
            }
        }
        _ => return Err("host parameter must be string or object".to_string()),
    };

    let port = if args.len() >= 2 {
        match &args[1] {
            Value::Number(n) => *n as u16,
            Value::U16(p) => *p,
            Value::U32(p) => *p as u16,
            _ => 80,
        }
    } else if host.contains(':') && !host.starts_with('[') {
        0
    } else {
        80
    };

    tcp_connect(host, port).map_err(|v| match v {
        Value::Object(m) => m
            .get("message")
            .and_then(|msg| match msg {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "TCP connect failed".to_string()),
        _ => "TCP connect failed".to_string(),
    })
}

fn builtin_tcp_listen(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("tcpListen requires (host, port)".to_string());
    }
    let (host, port_val) = if args.len() == 1 {
        ("0.0.0.0", &args[0])
    } else {
        (
            match &args[0] {
                Value::Str(s) => s.as_str(),
                _ => "0.0.0.0",
            },
            args.get(1).unwrap_or(&Value::Number(8089.0)),
        )
    };

    let port = match port_val {
        Value::Number(n) => *n as u16,
        Value::I64(n) => *n as u16,
        Value::U16(p) => *p,
        Value::U32(p) => *p as u16,
        Value::U64(p) => *p as u16,
        Value::I32(p) => *p as u16,
        Value::Str(s) => s.parse::<u16>().unwrap_or(8089),
        _ => 8089,
    };

    tcp_listen(host, port).map_err(|v| match v {
        Value::Object(m) => m
            .get("message")
            .and_then(|msg| match msg {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "TCP listen failed".to_string()),
        _ => "TCP listen failed".to_string(),
    })
}

fn builtin_udp_bind(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let host = args
        .first()
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .unwrap_or("0.0.0.0");

    let port = args
        .get(1)
        .and_then(|v| match v {
            Value::I64(n) => Some(*n as u16),
            Value::Number(n) => Some(*n as u16),
            Value::U16(p) => Some(*p),
            Value::U32(p) => Some(*p as u16),
            Value::U64(p) => Some(*p as u16),
            Value::I32(p) => Some(*p as u16),
            _ => None,
        })
        .unwrap_or(0);

    udp_bind(host, port).map_err(|v| match v {
        Value::Object(m) => m
            .get("message")
            .and_then(|msg| match msg {
                Value::Str(s) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "UDP bind failed".to_string()),
        _ => "UDP bind failed".to_string(),
    })
}

fn builtin_ip_parse(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let str_val = match args.first() {
        Some(Value::Str(s)) => s,
        _ => return Err("IPAddress.parse requires string parameter".to_string()),
    };
    let ip = IPAddress::parse(str_val)?;
    Ok(ip.to_value())
}

fn builtin_socket_address_parse(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let str_val = match args.first() {
        Some(Value::Str(s)) => s,
        _ => return Err("SocketAddress.parse requires string parameter".to_string()),
    };
    let sa = SocketAddress::parse(str_val)?;
    Ok(sa.to_value())
}

fn builtin_interfaces(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let list = list_network_interfaces();
    let val_list: Vec<Value> = list.iter().map(|iface| iface.to_value()).collect();
    Ok(Value::Array(val_list))
}

fn builtin_resolve(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let host = match args.first() {
        Some(Value::Str(s)) => s.as_str(),
        _ => return Err("resolve requires hostname parameter".to_string()),
    };

    let host_port = format!("{}:0", host);
    let addrs = host_port
        .to_socket_addrs()
        .map_err(|e| format!("Failed to resolve hostname '{}': {}", host, e))?;

    let mut ip_values = Vec::new();
    for sa in addrs {
        if let Ok(ip) = IPAddress::parse(&sa.ip().to_string()) {
            ip_values.push(ip.to_value());
        }
    }
    Ok(Value::Array(ip_values))
}

pub fn build_net_module_object() -> Value {
    let mut map = FastMap::default();

    map.insert(
        "tcpConnect".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_tcp_connect))),
    );
    map.insert(
        "tcpListen".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_tcp_listen))),
    );
    map.insert(
        "udpBind".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_udp_bind))),
    );
    map.insert(
        "resolve".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_resolve))),
    );
    map.insert(
        "resolveAddress".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_resolve))),
    );
    map.insert(
        "interfaces".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_interfaces))),
    );

    // IPAddress nested object namespace
    let mut ip_obj = FastMap::default();
    ip_obj.insert(
        "parse".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_ip_parse))),
    );
    map.insert("IPAddress".to_string(), Value::Object(Arc::new(ip_obj)));

    // SocketAddress nested object namespace
    let mut sa_obj = FastMap::default();
    sa_obj.insert(
        "parse".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_socket_address_parse))),
    );
    map.insert("SocketAddress".to_string(), Value::Object(Arc::new(sa_obj)));

    // NetworkPolicy constructor helper
    map.insert(
        "NetworkPolicy".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let policy = NetworkPolicy::new();
            Ok(policy.to_value())
        }))),
    );

    Value::Object(Arc::new(map))
}
