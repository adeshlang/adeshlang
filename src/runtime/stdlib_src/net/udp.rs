//! AdeshLang UDP Socket Implementation
//!
//! Provides thread-safe UDP socket handling, datagram send/receive, broadcast,
//! connected UDP mode, and IPv4/IPv6 multicast group operations.

use super::errors::{NetworkErrorKind, make_net_error};
use super::ip::IPAddress;
use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::net::{IpAddr, UdpSocket as StdUdpSocket};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static NEXT_UDP_ID: AtomicU64 = AtomicU64::new(1);

static UDP_SOCKETS: Lazy<Mutex<HashMap<u64, Arc<Mutex<Option<StdUdpSocket>>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn udp_bind(host_or_ip: &str, port: u16) -> Result<Value, Value> {
    let addr_str = if host_or_ip.contains(':') && !host_or_ip.starts_with('[') {
        format!("[{}]:{}", host_or_ip, port)
    } else {
        format!("{}:{}", host_or_ip, port)
    };

    let socket = StdUdpSocket::bind(&addr_str).map_err(|e| {
        make_net_error(
            NetworkErrorKind::from_io_error(&e),
            format!("Failed to bind UDP socket on '{}': {}", addr_str, e),
            Some("bind"),
            Some(&addr_str),
        )
    })?;

    let id = NEXT_UDP_ID.fetch_add(1, Ordering::SeqCst);
    let socket_arc = Arc::new(Mutex::new(Some(socket)));

    if let Ok(mut registry) = UDP_SOCKETS.lock() {
        registry.insert(id, socket_arc.clone());
    }

    Ok(build_udp_socket_value(id, socket_arc))
}

pub fn build_udp_socket_value(id: u64, socket_arc: Arc<Mutex<Option<StdUdpSocket>>>) -> Value {
    let mut map = FastMap::default();
    map.insert("id".to_string(), Value::U64(id));
    map.insert("type".to_string(), Value::Str("UdpSocket".to_string()));

    let u1 = socket_arc.clone();
    map.insert(
        "sendTo".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            if args.len() < 2 {
                return Err("sendTo requires (data, targetHost, [port])".to_string());
            }

            let bytes = match &args[0] {
                Value::Array(arr) => arr
                    .iter()
                    .filter_map(|v| match v {
                        Value::U8(b) => Some(*b),
                        Value::Number(n) => Some(*n as u8),
                        _ => None,
                    })
                    .collect::<Vec<u8>>(),
                Value::Str(s) => s.as_bytes().to_vec(),
                _ => return Err("data must be byte array or string".to_string()),
            };

            let target_str = match &args[1] {
                Value::Str(s) => s.clone(),
                _ => return Err("target must be host/IP string".to_string()),
            };

            let port = if args.len() >= 3 {
                match &args[2] {
                    Value::Number(n) => *n as u16,
                    Value::U16(p) => *p,
                    Value::U32(p) => *p as u16,
                    _ => 0,
                }
            } else {
                0
            };

            let dest_str = if target_str.contains(':') {
                target_str
            } else {
                format!("{}:{}", target_str, port)
            };

            let guard = u1.lock().map_err(|_| "Mutex poison".to_string())?;
            let socket = guard
                .as_ref()
                .ok_or_else(|| "UdpSocket is closed".to_string())?;

            match socket.send_to(&bytes, &dest_str) {
                Ok(sent) => Ok(Value::Number(sent as f64)),
                Err(e) => Err(format!("UDP sendTo error: {}", e)),
            }
        }))),
    );

    let u2 = socket_arc.clone();
    map.insert(
        "receiveFrom".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let max_len = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n as usize),
                    Value::U32(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(65535);

            let guard = u2.lock().map_err(|_| "Mutex poison".to_string())?;
            let socket = guard
                .as_ref()
                .ok_or_else(|| "UdpSocket is closed".to_string())?;

            let mut buf = vec![0u8; max_len];
            match socket.recv_from(&mut buf) {
                Ok((n, src_addr)) => {
                    buf.truncate(n);
                    let val_arr: Vec<Value> = buf.into_iter().map(Value::U8).collect();
                    let mut packet = FastMap::default();
                    packet.insert("data".to_string(), Value::Array(val_arr));
                    packet.insert(
                        "remoteAddress".to_string(),
                        Value::Str(src_addr.ip().to_string()),
                    );
                    packet.insert("remotePort".to_string(), Value::U16(src_addr.port()));
                    packet.insert("bytesReceived".to_string(), Value::Number(n as f64));
                    Ok(Value::Object(Arc::new(packet)))
                }
                Err(e) => Err(format!("UDP receiveFrom error: {}", e)),
            }
        }))),
    );

    let u3 = socket_arc.clone();
    map.insert(
        "setBroadcast".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let broadcast = args
                .first()
                .and_then(|v| match v {
                    Value::Bool(b) => Some(*b),
                    _ => None,
                })
                .unwrap_or(true);
            let guard = u3.lock().map_err(|_| "Mutex poison".to_string())?;
            let socket = guard
                .as_ref()
                .ok_or_else(|| "UdpSocket is closed".to_string())?;
            socket
                .set_broadcast(broadcast)
                .map_err(|e| format!("setBroadcast error: {}", e))?;
            Ok(Value::Bool(true))
        }))),
    );

    let u4 = socket_arc.clone();
    map.insert(
        "joinMulticast".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            if args.is_empty() {
                return Err("joinMulticast requires group IP string".to_string());
            }
            let group_str = match &args[0] {
                Value::Str(s) => s.as_str(),
                _ => return Err("group IP must be string".to_string()),
            };
            let interface_str = args
                .get(1)
                .and_then(|v| match v {
                    Value::Str(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("0.0.0.0");

            let group_ip = IPAddress::parse(group_str)?;
            let interface_ip = IPAddress::parse(interface_str)?;

            let guard = u4.lock().map_err(|_| "Mutex poison".to_string())?;
            let socket = guard
                .as_ref()
                .ok_or_else(|| "UdpSocket is closed".to_string())?;

            match (group_ip.to_std_ip(), interface_ip.to_std_ip()) {
                (IpAddr::V4(g), IpAddr::V4(i)) => {
                    socket
                        .join_multicast_v4(&g, &i)
                        .map_err(|e| format!("joinMulticast v4 error: {}", e))?;
                }
                (IpAddr::V6(g), _) => {
                    socket
                        .join_multicast_v6(&g, 0)
                        .map_err(|e| format!("joinMulticast v6 error: {}", e))?;
                }
                _ => return Err("Incompatible IP versions for multicast".to_string()),
            }

            Ok(Value::Bool(true))
        }))),
    );

    let u5 = socket_arc.clone();
    map.insert(
        "leaveMulticast".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            if args.is_empty() {
                return Err("leaveMulticast requires group IP string".to_string());
            }
            let group_str = match &args[0] {
                Value::Str(s) => s.as_str(),
                _ => return Err("group IP must be string".to_string()),
            };
            let interface_str = args
                .get(1)
                .and_then(|v| match v {
                    Value::Str(s) => Some(s.as_str()),
                    _ => None,
                })
                .unwrap_or("0.0.0.0");

            let group_ip = IPAddress::parse(group_str)?;
            let interface_ip = IPAddress::parse(interface_str)?;

            let guard = u5.lock().map_err(|_| "Mutex poison".to_string())?;
            let socket = guard
                .as_ref()
                .ok_or_else(|| "UdpSocket is closed".to_string())?;

            match (group_ip.to_std_ip(), interface_ip.to_std_ip()) {
                (IpAddr::V4(g), IpAddr::V4(i)) => {
                    socket
                        .leave_multicast_v4(&g, &i)
                        .map_err(|e| format!("leaveMulticast v4 error: {}", e))?;
                }
                (IpAddr::V6(g), _) => {
                    socket
                        .leave_multicast_v6(&g, 0)
                        .map_err(|e| format!("leaveMulticast v6 error: {}", e))?;
                }
                _ => return Err("Incompatible IP versions for multicast".to_string()),
            }

            Ok(Value::Bool(true))
        }))),
    );

    let u6 = socket_arc.clone();
    map.insert(
        "localAddress".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = u6.lock().map_err(|_| "Mutex poison".to_string())?;
            let socket = guard
                .as_ref()
                .ok_or_else(|| "UdpSocket is closed".to_string())?;
            match socket.local_addr() {
                Ok(addr) => Ok(Value::Str(addr.to_string())),
                Err(e) => Err(format!("Failed to get local address: {}", e)),
            }
        }))),
    );

    let u7 = socket_arc.clone();
    map.insert(
        "close".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let mut guard = u7.lock().map_err(|_| "Mutex poison".to_string())?;
            guard.take();
            if let Ok(mut registry) = UDP_SOCKETS.lock() {
                registry.remove(&id);
            }
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}
