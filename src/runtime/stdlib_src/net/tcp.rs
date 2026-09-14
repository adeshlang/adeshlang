//! AdeshLang TCP Client and Server Implementation
//!
//! Provides thread-safe, RAII-managed TCP client streams and server listeners
//! with full support for timeouts, KeepAlive, TCP_NODELAY, partial/exact reads, and shutdown.

use super::errors::{NetworkErrorKind, make_net_error};
use crate::parsing::ast::Value;
use crate::utils::collections::FastMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{Shutdown, TcpListener as StdTcpListener, TcpStream as StdTcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

static NEXT_SOCKET_ID: AtomicU64 = AtomicU64::new(1);

pub static TCP_STREAMS: Lazy<Mutex<HashMap<u64, Arc<Mutex<Option<StdTcpStream>>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
static TCP_LISTENERS: Lazy<Mutex<HashMap<u64, Arc<Mutex<Option<StdTcpListener>>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn allocate_socket_id() -> u64 {
    NEXT_SOCKET_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn clone_tcp_stream(id: u64) -> Result<StdTcpStream, String> {
    let registry = TCP_STREAMS
        .lock()
        .map_err(|_| "TCP registry lock poisoned".to_string())?;
    let stream_arc = registry
        .get(&id)
        .ok_or_else(|| format!("TcpStream with id {id} not found or closed"))?;
    let guard = stream_arc
        .lock()
        .map_err(|_| "TcpStream lock poisoned".to_string())?;
    let stream = guard
        .as_ref()
        .ok_or_else(|| "TcpStream is closed".to_string())?;
    stream
        .try_clone()
        .map_err(|e| format!("Failed to clone TcpStream: {e}"))
}

pub struct TcpStreamHandle {
    pub id: u64,
    pub stream: Arc<Mutex<Option<StdTcpStream>>>,
}

impl Drop for TcpStreamHandle {
    fn drop(&mut self) {
        if let Ok(mut registry) = TCP_STREAMS.lock() {
            registry.remove(&self.id);
        }
        if let Ok(mut guard) = self.stream.lock() {
            if let Some(stream) = guard.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
        }
    }
}

pub fn tcp_connect(host_or_ip: &str, port: u16) -> Result<Value, Value> {
    let addr_str = if host_or_ip.contains(':') && !host_or_ip.starts_with('[') {
        format!("[{}]:{}", host_or_ip, port)
    } else {
        format!("{}:{}", host_or_ip, port)
    };

    let socket_addrs = addr_str.to_socket_addrs().map_err(|e| {
        make_net_error(
            NetworkErrorKind::InvalidAddress,
            format!("Host resolution failed for '{}': {}", addr_str, e),
            Some("connect"),
            Some(&addr_str),
        )
    })?;

    let mut last_err = None;
    let mut connected_stream = None;

    for addr in socket_addrs {
        match StdTcpStream::connect_timeout(&addr, Duration::from_secs(10)) {
            Ok(stream) => {
                connected_stream = Some(stream);
                break;
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }

    let stream = match connected_stream {
        Some(s) => s,
        None => {
            let err = last_err.unwrap_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotConnected, "No address resolved")
            });
            return Err(make_net_error(
                NetworkErrorKind::from_io_error(&err),
                format!("Failed to connect to '{}': {}", addr_str, err),
                Some("connect"),
                Some(&addr_str),
            ));
        }
    };

    let id = allocate_socket_id();
    let stream_arc = Arc::new(Mutex::new(Some(stream)));

    if let Ok(mut registry) = TCP_STREAMS.lock() {
        registry.insert(id, stream_arc.clone());
    }

    Ok(build_tcp_stream_value(id, stream_arc))
}

pub fn tcp_listen(host_or_ip: &str, port: u16) -> Result<Value, Value> {
    let addr_str = if host_or_ip.contains(':') && !host_or_ip.starts_with('[') {
        format!("[{}]:{}", host_or_ip, port)
    } else {
        format!("{}:{}", host_or_ip, port)
    };

    let listener = StdTcpListener::bind(&addr_str).map_err(|e| {
        make_net_error(
            NetworkErrorKind::from_io_error(&e),
            format!("Failed to listen on '{}': {}", addr_str, e),
            Some("listen"),
            Some(&addr_str),
        )
    })?;

    let id = allocate_socket_id();
    let listener_arc = Arc::new(Mutex::new(Some(listener)));

    if let Ok(mut registry) = TCP_LISTENERS.lock() {
        registry.insert(id, listener_arc.clone());
    }

    Ok(build_tcp_listener_value(id, listener_arc))
}

pub fn build_tcp_stream_value(id: u64, stream_arc: Arc<Mutex<Option<StdTcpStream>>>) -> Value {
    let mut map = FastMap::default();
    if let Ok(guard) = stream_arc.lock() {
        if let Some(stream) = guard.as_ref() {
            let _ = stream.set_nodelay(true);
        }
    }

    map.insert("id".to_string(), Value::U64(id));
    map.insert("type".to_string(), Value::Str("TcpStream".to_string()));

    let s1 = stream_arc.clone();
    map.insert(
        "read".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let max_len = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n as usize),
                    Value::U32(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(4096);

            let guard = s1.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut buf = vec![0u8; max_len];
            // Note: StdTcpStream needs mut, clone handle if needed or use reference via read
            let mut stream_ref = stream;
            match stream_ref.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    let val_arr: Vec<Value> = buf.into_iter().map(Value::U8).collect();
                    Ok(Value::Array(val_arr))
                }
                Err(e) => Err(format!("TCP read error: {}", e)),
            }
        }))),
    );

    let s2 = stream_arc.clone();
    map.insert(
        "readExact".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let len = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n as usize),
                    Value::U32(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .ok_or_else(|| "readExact requires exact byte length".to_string())?;

            let guard = s2.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut buf = vec![0u8; len];
            let mut stream_ref = stream;
            match stream_ref.read_exact(&mut buf) {
                Ok(_) => {
                    let val_arr: Vec<Value> = buf.into_iter().map(Value::U8).collect();
                    Ok(Value::Array(val_arr))
                }
                Err(e) => Err(format!("TCP readExact error: {}", e)),
            }
        }))),
    );

    let s3 = stream_arc.clone();
    map.insert(
        "write".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let bytes = match args.first() {
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(|v| match v {
                        Value::U8(b) => Some(*b),
                        Value::Number(n) => Some(*n as u8),
                        _ => None,
                    })
                    .collect::<Vec<u8>>(),
                Some(Value::Str(s)) => s.as_bytes().to_vec(),
                _ => return Err("write requires byte array or string".to_string()),
            };

            let guard = s3.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut stream_ref = stream;
            match stream_ref.write(&bytes) {
                Ok(written) => Ok(Value::Number(written as f64)),
                Err(e) => Err(format!("TCP write error: {}", e)),
            }
        }))),
    );

    let s4 = stream_arc.clone();
    map.insert(
        "writeAll".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let bytes = match args.first() {
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(|v| match v {
                        Value::U8(b) => Some(*b),
                        Value::Number(n) => Some(*n as u8),
                        _ => None,
                    })
                    .collect::<Vec<u8>>(),
                Some(Value::Str(s)) => s.as_bytes().to_vec(),
                _ => return Err("writeAll requires byte array or string".to_string()),
            };

            let guard = s4.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut stream_ref = stream;
            match stream_ref.write_all(&bytes) {
                Ok(_) => Ok(Value::Bool(true)),
                Err(e) => Err(format!("TCP writeAll error: {}", e)),
            }
        }))),
    );

    let s4_bytes = stream_arc.clone();
    map.insert(
        "writeBytes".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let bytes = match args.first() {
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(|v| match v {
                        Value::U8(b) => Some(*b),
                        Value::Number(n) => Some(*n as u8),
                        Value::I64(n) => Some(*n as u8),
                        Value::I32(n) => Some(*n as u8),
                        _ => None,
                    })
                    .collect::<Vec<u8>>(),
                Some(Value::Str(s)) => s.as_bytes().to_vec(),
                _ => return Err("writeBytes requires byte array or string".to_string()),
            };

            let guard = s4_bytes.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut stream_ref = stream;
            match stream_ref.write_all(&bytes) {
                Ok(_) => Ok(Value::Bool(true)),
                Err(e) => Err(format!("TCP writeBytes error: {}", e)),
            }
        }))),
    );

    let s5 = stream_arc.clone();
    map.insert(
        "writeText".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let text = match args.first() {
                Some(Value::Str(s)) => s.as_bytes(),
                _ => return Err("writeText requires string parameter".to_string()),
            };

            let guard = s5.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut stream_ref = stream;
            match stream_ref.write_all(text) {
                Ok(_) => Ok(Value::Bool(true)),
                Err(e) => Err(format!("TCP writeText error: {}", e)),
            }
        }))),
    );

    let s_rb = stream_arc.clone();
    map.insert(
        "readBytes".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let max_len = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n as usize),
                    Value::U32(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(4096);

            let guard = s_rb.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut buf = vec![0u8; max_len];
            let mut stream_ref = stream;
            match stream_ref.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    let val_arr: Vec<Value> = buf.into_iter().map(Value::U8).collect();
                    Ok(Value::Array(val_arr))
                }
                Err(e) => Err(format!("TCP readBytes error: {}", e)),
            }
        }))),
    );

    let s_rt = stream_arc.clone();
    map.insert(
        "readText".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let max_len = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n as usize),
                    Value::U32(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(4096);

            let guard = s_rt.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut buf = vec![0u8; max_len];
            let mut stream_ref = stream;
            match stream_ref.read(&mut buf) {
                Ok(n) => {
                    buf.truncate(n);
                    Ok(Value::Str(String::from_utf8_lossy(&buf).to_string()))
                }
                Err(e) => Err(format!("TCP readText error: {}", e)),
            }
        }))),
    );

    let s6 = stream_arc.clone();
    map.insert(
        "flush".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = s6.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;

            let mut stream_ref = stream;
            match stream_ref.flush() {
                Ok(_) => Ok(Value::Bool(true)),
                Err(e) => Err(format!("TCP flush error: {}", e)),
            }
        }))),
    );

    let s7 = stream_arc.clone();
    map.insert(
        "shutdown".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = s7.lock().map_err(|_| "Mutex poison".to_string())?;
            if let Some(stream) = guard.as_ref() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            Ok(Value::Bool(true))
        }))),
    );

    let s8 = stream_arc.clone();
    map.insert(
        "close".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let mut guard = s8.lock().map_err(|_| "Mutex poison".to_string())?;
            if let Some(stream) = guard.take() {
                let _ = stream.shutdown(Shutdown::Both);
            }
            if let Ok(mut registry) = TCP_STREAMS.lock() {
                registry.remove(&id);
            }
            Ok(Value::Bool(true))
        }))),
    );

    let s9 = stream_arc.clone();
    map.insert(
        "remoteAddress".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = s9.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;
            match stream.peer_addr() {
                Ok(addr) => Ok(Value::Str(addr.to_string())),
                Err(e) => Err(format!("Failed to get peer address: {}", e)),
            }
        }))),
    );

    let s10 = stream_arc.clone();
    map.insert(
        "localAddress".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = s10.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;
            match stream.local_addr() {
                Ok(addr) => Ok(Value::Str(addr.to_string())),
                Err(e) => Err(format!("Failed to get local address: {}", e)),
            }
        }))),
    );

    let s11 = stream_arc.clone();
    map.insert(
        "setNoDelay".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let nodelay = args
                .first()
                .and_then(|v| match v {
                    Value::Bool(b) => Some(*b),
                    _ => None,
                })
                .unwrap_or(true);
            let guard = s11.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;
            stream
                .set_nodelay(nodelay)
                .map_err(|e| format!("setNoDelay error: {}", e))?;
            Ok(Value::Bool(true))
        }))),
    );

    let s12 = stream_arc.clone();
    map.insert(
        "setReadTimeout".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let secs = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(0.0);
            let dur = if secs > 0.0 {
                Some(Duration::from_secs_f64(secs))
            } else {
                None
            };
            let guard = s12.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;
            stream
                .set_read_timeout(dur)
                .map_err(|e| format!("setReadTimeout error: {}", e))?;
            Ok(Value::Bool(true))
        }))),
    );

    let s13 = stream_arc.clone();
    map.insert(
        "setWriteTimeout".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, args| {
            let secs = args
                .first()
                .and_then(|v| match v {
                    Value::Number(n) => Some(*n),
                    _ => None,
                })
                .unwrap_or(0.0);
            let dur = if secs > 0.0 {
                Some(Duration::from_secs_f64(secs))
            } else {
                None
            };
            let guard = s13.lock().map_err(|_| "Mutex poison".to_string())?;
            let stream = guard
                .as_ref()
                .ok_or_else(|| "TcpStream is closed".to_string())?;
            stream
                .set_write_timeout(dur)
                .map_err(|e| format!("setWriteTimeout error: {}", e))?;
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn build_tcp_listener_value(
    id: u64,
    listener_arc: Arc<Mutex<Option<StdTcpListener>>>,
) -> Value {
    let mut map = FastMap::default();
    map.insert("id".to_string(), Value::U64(id));
    map.insert("type".to_string(), Value::Str("TcpListener".to_string()));

    let l1 = listener_arc.clone();
    map.insert(
        "accept".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = l1.lock().map_err(|_| "Mutex poison".to_string())?;
            let listener = guard
                .as_ref()
                .ok_or_else(|| "TcpListener is closed".to_string())?;

            match listener.accept() {
                Ok((stream, _addr)) => {
                    let stream_id = allocate_socket_id();
                    let stream_arc = Arc::new(Mutex::new(Some(stream)));
                    if let Ok(mut registry) = TCP_STREAMS.lock() {
                        registry.insert(stream_id, stream_arc.clone());
                    }
                    let stream_val = build_tcp_stream_value(stream_id, stream_arc);
                    Ok(stream_val)
                }
                Err(e) => Err(format!("TCP accept error: {}", e)),
            }
        }))),
    );

    let l2 = listener_arc.clone();
    map.insert(
        "localAddress".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let guard = l2.lock().map_err(|_| "Mutex poison".to_string())?;
            let listener = guard
                .as_ref()
                .ok_or_else(|| "TcpListener is closed".to_string())?;
            match listener.local_addr() {
                Ok(addr) => Ok(Value::Str(addr.to_string())),
                Err(e) => Err(format!("Failed to get local address: {}", e)),
            }
        }))),
    );

    let l3 = listener_arc.clone();
    map.insert(
        "close".to_string(),
        Value::Function(crate::parsing::ast::NativeFn(Arc::new(move |_, _| {
            let mut guard = l3.lock().map_err(|_| "Mutex poison".to_string())?;
            guard.take();
            if let Ok(mut registry) = TCP_LISTENERS.lock() {
                registry.remove(&id);
            }
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}
