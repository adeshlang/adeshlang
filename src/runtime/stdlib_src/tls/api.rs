use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::stdlib_src::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::cert::TlsTrustStore;
use super::connection::TlsConnection;
use super::policy::{TlsSecurityPolicy, TlsVersion};

type TlsConnRef = Arc<Mutex<Option<TlsConnection>>>;

static NEXT_TLS_ID: AtomicU64 = AtomicU64::new(1);
static TLS_CONNECTIONS: Lazy<Mutex<HashMap<u64, TlsConnRef>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

fn allocate_tls_id() -> u64 {
    NEXT_TLS_ID.fetch_add(1, Ordering::SeqCst)
}

fn register_tls_connection(conn: TlsConnection) -> u64 {
    let id = allocate_tls_id();
    let conn_arc = Arc::new(Mutex::new(Some(conn)));
    if let Ok(mut reg) = TLS_CONNECTIONS.lock() {
        reg.insert(id, conn_arc);
    }
    id
}

pub fn take_tls_connection(id: u64) -> Result<TlsConnection, String> {
    let mut reg = TLS_CONNECTIONS
        .lock()
        .map_err(|_| "TLS registry lock poisoned".to_string())?;
    let conn_arc = reg
        .remove(&id)
        .ok_or_else(|| format!("TLS connection with ID {id} not found or already closed"))?;
    let mut guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    guard
        .take()
        .ok_or_else(|| "TLS connection is closed".to_string())
}

fn get_tls_connection(id: u64) -> Result<TlsConnRef, String> {
    let reg = TLS_CONNECTIONS
        .lock()
        .map_err(|_| "TLS registry lock poisoned".to_string())?;
    reg.get(&id)
        .cloned()
        .ok_or_else(|| format!("TLS connection with ID {} not found or already closed", id))
}

fn extract_tls_id(val: &Value) -> Result<u64, String> {
    match val {
        Value::U64(id) => Ok(*id),
        Value::I64(id) if *id > 0 => Ok(*id as u64),
        Value::Number(n) if *n > 0.0 => Ok(*n as u64),
        Value::Object(m) => {
            if let Some(Value::U64(id)) = m.get("handle") {
                Ok(*id)
            } else if let Some(Value::I64(id)) = m.get("handle") {
                Ok(*id as u64)
            } else if let Some(Value::Number(id)) = m.get("handle") {
                Ok(*id as u64)
            } else if let Some(Value::U64(id)) = m.get("id") {
                Ok(*id)
            } else {
                Err("TLS object does not contain a valid handle ID".into())
            }
        }
        _ => Err("Invalid TLS handle parameter".into()),
    }
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    let cat = "tls";

    registry.register(
        "TLS.connect",
        cat,
        "Establish secure TLS connection to host and port",
        builtin_connect,
    );

    registry.register(
        "TLS.connectWithOptions",
        cat,
        "Establish secure TLS connection with custom options (ALPN, custom CA, verify flags)",
        builtin_connect_with_options,
    );

    registry.register(
        "TLS.bindServer",
        cat,
        "Wrap socket connection into TLS Server stream",
        builtin_bind_server,
    );

    registry.register(
        "TLS.wrap",
        cat,
        "Wrap existing socket handle into TLS stream (STARTTLS)",
        builtin_wrap,
    );

    registry.register(
        "TLS.write",
        cat,
        "Write data bytes to TLS connection",
        builtin_write,
    );

    registry.register(
        "TLS.read",
        cat,
        "Read data bytes from TLS connection",
        builtin_read,
    );

    registry.register(
        "TLS.version",
        cat,
        "Get negotiated TLS protocol version",
        builtin_version,
    );

    registry.register(
        "TLS.alpn",
        cat,
        "Get negotiated ALPN protocol",
        builtin_alpn,
    );

    registry.register(
        "TLS.peerCertificates",
        cat,
        "Get peer certificate chain DER byte arrays",
        builtin_peer_certificates,
    );

    registry.register(
        "TLS.trace",
        cat,
        "Get diagnostic trace events of TLS connection",
        builtin_trace,
    );

    registry.register(
        "TLS.readBytes",
        cat,
        "Read raw binary data bytes array from TLS connection",
        builtin_read_bytes,
    );

    registry.register(
        "TLS.readText",
        cat,
        "Read UTF-8 text string from TLS connection",
        builtin_read,
    );

    registry.register(
        "TLS.writeBytes",
        cat,
        "Write binary byte array to TLS connection",
        builtin_write,
    );

    registry.register(
        "TLS.writeText",
        cat,
        "Write UTF-8 text string to TLS connection",
        builtin_write,
    );

    registry.register(
        "TLS.Server",
        cat,
        "Create production TLS Server listener wrapper over Net.tcpListen",
        builtin_create_server,
    );

    registry.register(
        "TLS.close",
        cat,
        "Gracefully close TLS connection",
        builtin_close,
    );
}

pub fn build_tls_module_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "connect".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_connect))),
    );
    map.insert(
        "connectWithOptions".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_connect_with_options))),
    );
    map.insert(
        "bindServer".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_bind_server))),
    );
    map.insert(
        "Server".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_create_server))),
    );
    map.insert(
        "wrap".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_wrap))),
    );
    map.insert(
        "write".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_write))),
    );
    map.insert(
        "writeBytes".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_write))),
    );
    map.insert(
        "writeText".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_write))),
    );
    map.insert(
        "read".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read))),
    );
    map.insert(
        "readBytes".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read_bytes))),
    );
    map.insert(
        "readText".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_read))),
    );
    map.insert(
        "version".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_version))),
    );
    map.insert(
        "alpn".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_alpn))),
    );
    map.insert(
        "peerCertificates".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_peer_certificates))),
    );
    map.insert(
        "trace".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_trace))),
    );
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_close))),
    );

    // TLS.SecurityPolicy builder constructor
    map.insert(
        "SecurityPolicy".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut sec_map = FastMap::default();
            sec_map.insert("verifyCertificates".to_string(), Value::Bool(true));
            sec_map.insert("verifyHostname".to_string(), Value::Bool(true));
            sec_map.insert("minVersion".to_string(), Value::Str("TLS1.2".to_string()));
            sec_map.insert("maxVersion".to_string(), Value::Str("TLS1.3".to_string()));
            sec_map.insert("alpn".to_string(), Value::Array(Vec::new()));

            sec_map.insert(
                "strict".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| {
                    let mut s = FastMap::default();
                    s.insert("verifyCertificates".to_string(), Value::Bool(true));
                    s.insert("verifyHostname".to_string(), Value::Bool(true));
                    s.insert("minVersion".to_string(), Value::Str("TLS1.3".to_string()));
                    s.insert("maxVersion".to_string(), Value::Str("TLS1.3".to_string()));
                    s.insert("alpn".to_string(), Value::Array(Vec::new()));
                    Ok(Value::Object(Arc::new(s)))
                }))),
            );

            Ok(Value::Object(Arc::new(sec_map)))
        }))),
    );

    // TLS.TrustStore builder constructor
    map.insert(
        "TrustStore".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut ts_map = FastMap::default();
            ts_map.insert("systemTrust".to_string(), Value::Bool(true));
            ts_map.insert("customCaPems".to_string(), Value::Array(Vec::new()));

            ts_map.insert(
                "systemAndWebpki".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| {
                    let mut t = FastMap::default();
                    t.insert("systemTrust".to_string(), Value::Bool(true));
                    t.insert("customCaPems".to_string(), Value::Array(Vec::new()));
                    Ok(Value::Object(Arc::new(t)))
                }))),
            );

            Ok(Value::Object(Arc::new(ts_map)))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn builtin_connect(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("TLS.connect requires hostname and port".into());
    }
    let host = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("Hostname must be a string".into()),
    };
    let port = match &args[1] {
        Value::I64(p) => *p as u16,
        Value::Number(p) => *p as u16,
        Value::U16(p) => *p,
        Value::U32(p) => *p as u16,
        Value::U64(p) => *p as u16,
        _ => return Err("Port must be an integer".into()),
    };

    let policy = TlsSecurityPolicy::default();
    let trust_store = TlsTrustStore::default();

    let conn = TlsConnection::connect_client(host, port, &policy, &trust_store, None)
        .map_err(|e| e.to_string())?;

    let id = register_tls_connection(conn);
    Ok(build_connection_object(id))
}

fn builtin_connect_with_options(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("TLS.connectWithOptions requires (host, port, optionsObject)".into());
    }
    let host = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("Hostname must be a string".into()),
    };
    let port = match &args[1] {
        Value::I64(p) => *p as u16,
        Value::Number(p) => *p as u16,
        Value::U16(p) => *p,
        Value::U32(p) => *p as u16,
        Value::U64(p) => *p as u16,
        _ => return Err("Port must be an integer".into()),
    };

    let opts = match &args[2] {
        Value::Object(m) => m,
        _ => return Err("Options must be an object".into()),
    };

    let mut policy = TlsSecurityPolicy::default();

    if let Some(Value::Bool(v)) = opts.get("verifyCertificates") {
        policy.verify_certificates = *v;
    }
    if let Some(Value::Bool(v)) = opts.get("verifyHostname") {
        policy.verify_hostname = *v;
    }
    if let Some(Value::Str(v)) = opts.get("minVersion") {
        if v == "TLS1.3" {
            policy.min_version = TlsVersion::Tls13;
        } else if v == "TLS1.2" {
            policy.min_version = TlsVersion::Tls12;
        }
    }
    if let Some(Value::Str(v)) = opts.get("maxVersion") {
        if v == "TLS1.3" {
            policy.max_version = TlsVersion::Tls13;
        } else if v == "TLS1.2" {
            policy.max_version = TlsVersion::Tls12;
        }
    }
    if let Some(Value::Array(alpn_arr)) = opts.get("alpn") {
        let mut protocols = Vec::new();
        for item in alpn_arr.iter() {
            if let Value::Str(proto) = item {
                protocols.push(proto.as_bytes().to_vec());
            }
        }
        policy.alpn_protocols = protocols;
    }
    if let Some(Value::Str(ca_pem)) = opts
        .get("customCaPem")
        .or_else(|| opts.get("caPath"))
        .or_else(|| opts.get("customCaPath"))
    {
        policy.custom_ca_pems.push(ca_pem.as_bytes().to_vec());
    }

    let cert_val = opts
        .get("clientCertPem")
        .or_else(|| opts.get("clientCertPath"))
        .or_else(|| opts.get("certPath"));
    let key_val = opts
        .get("clientKeyPem")
        .or_else(|| opts.get("clientKeyPath"))
        .or_else(|| opts.get("keyPath"));

    let client_cert_pair =
        if let (Some(Value::Str(cert)), Some(Value::Str(key))) = (cert_val, key_val) {
            Some((cert.as_bytes(), key.as_bytes()))
        } else {
            None
        };

    let trust_store = TlsTrustStore::default();

    let conn = TlsConnection::connect_client(host, port, &policy, &trust_store, client_cert_pair)
        .map_err(|e| e.to_string())?;

    let id = register_tls_connection(conn);
    Ok(build_connection_object(id))
}

pub fn extract_tcp_stream(val: &Value) -> Result<std::net::TcpStream, String> {
    match val {
        Value::Object(m) => {
            if let Some(Value::U64(id)) = m.get("id") {
                let registry = crate::runtime::stdlib_src::net::tcp::TCP_STREAMS
                    .lock()
                    .map_err(|_| "Registry lock failed")?;
                let stream_arc = registry
                    .get(id)
                    .ok_or_else(|| format!("TcpStream with id {} not found or closed", id))?;
                let guard = stream_arc.lock().map_err(|_| "Socket lock failed")?;
                let stream = guard
                    .as_ref()
                    .ok_or_else(|| "TcpStream is closed".to_string())?;
                stream
                    .try_clone()
                    .map_err(|e| format!("Failed to clone socket: {}", e))
            } else {
                Err("Invalid TcpStream object parameter".into())
            }
        }
        _ => Err("Invalid TCP socket parameter".into()),
    }
}

fn builtin_bind_server(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("TLS.bindServer requires (tcpSocket, certPem, keyPem)".into());
    }
    let socket = extract_tcp_stream(&args[0])?;
    let cert_pem = match &args[1] {
        Value::Str(s) => s.as_bytes(),
        _ => return Err("certPem must be string".into()),
    };
    let key_pem = match &args[2] {
        Value::Str(s) => s.as_bytes(),
        _ => return Err("keyPem must be string".into()),
    };

    let policy = TlsSecurityPolicy::default();
    let conn = TlsConnection::wrap_server(socket, cert_pem, key_pem, &policy)
        .map_err(|e| e.to_string())?;

    let id = register_tls_connection(conn);
    Ok(build_connection_object(id))
}

fn builtin_wrap(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("TLS.wrap requires (tcpSocket, hostname)".into());
    }
    let socket = extract_tcp_stream(&args[0])?;
    let host = match &args[1] {
        Value::Str(s) => s.as_str(),
        _ => return Err("Hostname must be string".into()),
    };

    let policy = TlsSecurityPolicy::default();
    let trust_store = TlsTrustStore::default();

    let conn = TlsConnection::wrap_client(socket, host, &policy, &trust_store, None)
        .map_err(|e| e.to_string())?;

    let id = register_tls_connection(conn);
    Ok(build_connection_object(id))
}

fn builtin_write(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("TLS.write requires connection handle and data".into());
    }
    let id = extract_tls_id(&args[0])?;

    let data_bytes = match &args[1] {
        Value::Str(s) => s.as_bytes().to_vec(),
        Value::Array(arr) => {
            let mut b = Vec::new();
            for item in arr.iter() {
                match item {
                    Value::Number(n) => b.push(*n as u8),
                    Value::U8(n) => b.push(*n),
                    Value::U16(n) => b.push(*n as u8),
                    Value::U32(n) => b.push(*n as u8),
                    Value::U64(n) => b.push(*n as u8),
                    Value::I8(n) => b.push(*n as u8),
                    Value::I16(n) => b.push(*n as u8),
                    Value::I32(n) => b.push(*n as u8),
                    Value::I64(n) => b.push(*n as u8),
                    _ => {}
                }
            }
            b
        }
        Value::RawArray(_, arr) => {
            let mut b = Vec::new();
            for item in arr.iter() {
                match item {
                    Value::Number(n) => b.push(*n as u8),
                    Value::U8(n) => b.push(*n),
                    Value::U16(n) => b.push(*n as u8),
                    Value::U32(n) => b.push(*n as u8),
                    Value::U64(n) => b.push(*n as u8),
                    Value::I8(n) => b.push(*n as u8),
                    Value::I16(n) => b.push(*n as u8),
                    Value::I32(n) => b.push(*n as u8),
                    Value::I64(n) => b.push(*n as u8),
                    _ => {}
                }
            }
            b
        }
        Value::DynArray(dyn_arr) => {
            let mut b = Vec::new();
            for item in dyn_arr.data.iter() {
                match item {
                    Value::Number(n) => b.push(*n as u8),
                    Value::U8(n) => b.push(*n),
                    Value::U16(n) => b.push(*n as u8),
                    Value::U32(n) => b.push(*n as u8),
                    Value::U64(n) => b.push(*n as u8),
                    Value::I8(n) => b.push(*n as u8),
                    Value::I16(n) => b.push(*n as u8),
                    Value::I32(n) => b.push(*n as u8),
                    Value::I64(n) => b.push(*n as u8),
                    _ => {}
                }
            }
            b
        }
        _ => return Err("Data must be string or array of bytes".into()),
    };

    let conn_arc = get_tls_connection(id)?;
    let mut guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_mut()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let written = conn.write(&data_bytes).map_err(|e| e.to_string())?;
    conn.flush().map_err(|e| e.to_string())?;

    Ok(Value::I64(written as i64))
}

fn builtin_read(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.read requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;
    let max_len = if args.len() > 1 {
        match &args[1] {
            Value::I64(i) => *i as usize,
            Value::Number(i) => *i as usize,
            Value::U32(i) => *i as usize,
            Value::U64(i) => *i as usize,
            _ => 8192,
        }
    } else {
        8192
    };

    let conn_arc = get_tls_connection(id)?;
    let mut guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_mut()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let mut buf = vec![0u8; max_len];
    let n = conn.read(&mut buf).map_err(|e| e.to_string())?;
    buf.truncate(n);

    String::from_utf8(buf)
        .map(Value::Str)
        .map_err(|e| format!("TLS readText error: Invalid UTF-8 sequence ({})", e))
}

fn builtin_read_bytes(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.readBytes requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;
    let max_len = if args.len() > 1 {
        match &args[1] {
            Value::I64(i) => *i as usize,
            Value::Number(i) => *i as usize,
            Value::U32(i) => *i as usize,
            Value::U64(i) => *i as usize,
            _ => 8192,
        }
    } else {
        8192
    };

    let conn_arc = get_tls_connection(id)?;
    let mut guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_mut()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let bytes = conn.read_bytes(max_len).map_err(|e| e.to_string())?;

    let val_arr: Vec<Value> = bytes.into_iter().map(|b| Value::Number(b as f64)).collect();
    Ok(Value::Array(val_arr))
}

fn builtin_create_server(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 3 {
        return Err("TLS.Server requires (tcpListener, certPem, keyPem)".into());
    }
    let listener_val = args[0].clone();
    let cert_pem = match &args[1] {
        Value::Str(s) => s.clone(),
        _ => return Err("certPem must be string".into()),
    };
    let key_pem = match &args[2] {
        Value::Str(s) => s.clone(),
        _ => return Err("keyPem must be string".into()),
    };

    let mut map = FastMap::default();
    map.insert(
        "accept".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let accept_fn = match &listener_val {
                Value::Object(m) => m.get("accept").cloned(),
                _ => None,
            }
            .ok_or_else(|| "Invalid listener object".to_string())?;

            let client_obj = match accept_fn {
                Value::Function(f) => (f.0)(&mut crate::parsing::ast::NoopEnv, vec![])?,
                _ => return Err("accept method missing on listener".into()),
            };

            let stream_val = match &client_obj {
                Value::Object(m) => m
                    .get("stream")
                    .cloned()
                    .or_else(|| Some(client_obj.clone())),
                _ => None,
            }
            .ok_or_else(|| "Failed to extract client stream".to_string())?;

            builtin_bind_server(
                &mut crate::parsing::ast::NoopEnv,
                vec![
                    stream_val,
                    Value::Str(cert_pem.clone()),
                    Value::Str(key_pem.clone()),
                ],
            )
        }))),
    );

    Ok(Value::Object(Arc::new(map)))
}

fn builtin_version(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.version requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;
    let conn_arc = get_tls_connection(id)?;
    let guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let ver = conn
        .protocol_version()
        .unwrap_or_else(|| "Unknown".to_string());
    Ok(Value::Str(ver))
}

fn builtin_alpn(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.alpn requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;
    let conn_arc = get_tls_connection(id)?;
    let guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let alpn = conn.alpn_protocol().unwrap_or_default();
    Ok(Value::Str(alpn))
}

fn builtin_peer_certificates(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.peerCertificates requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;
    let conn_arc = get_tls_connection(id)?;
    let guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let certs = conn.peer_certificates();
    let val_certs: Vec<Value> = certs
        .into_iter()
        .map(|c| Value::Array(c.into_iter().map(|b| Value::Number(b as f64)).collect()))
        .collect();

    Ok(Value::Array(val_certs))
}

fn builtin_trace(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.trace requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;
    let conn_arc = get_tls_connection(id)?;
    let guard = conn_arc
        .lock()
        .map_err(|_| "Failed to lock TLS connection".to_string())?;
    let conn = guard
        .as_ref()
        .ok_or_else(|| "TLS connection is closed".to_string())?;
    let events: Vec<Value> = conn
        .trace_events
        .iter()
        .map(|e| Value::Str(e.clone()))
        .collect();
    Ok(Value::Array(events))
}

fn builtin_close(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("TLS.close requires connection handle".into());
    }
    let id = extract_tls_id(&args[0])?;

    if let Ok(mut reg) = TLS_CONNECTIONS.lock() {
        if let Some(conn_arc) = reg.remove(&id) {
            if let Ok(mut guard) = conn_arc.lock() {
                if let Some(mut conn) = guard.take() {
                    let _ = conn.shutdown();
                }
            }
        }
    }

    Ok(Value::Bool(true))
}

fn build_connection_object(id: u64) -> Value {
    let mut map = FastMap::default();
    map.insert("handle".to_string(), Value::U64(id));
    map.insert("id".to_string(), Value::U64(id));
    map.insert(
        "read".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let mut full_args = vec![Value::U64(id)];
            full_args.extend(args);
            builtin_read(&mut crate::parsing::ast::NoopEnv, full_args)
        }))),
    );
    map.insert(
        "readText".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let mut full_args = vec![Value::U64(id)];
            full_args.extend(args);
            builtin_read(&mut crate::parsing::ast::NoopEnv, full_args)
        }))),
    );
    map.insert(
        "readBytes".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let mut full_args = vec![Value::U64(id)];
            full_args.extend(args);
            builtin_read_bytes(&mut crate::parsing::ast::NoopEnv, full_args)
        }))),
    );
    map.insert(
        "write".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let mut full_args = vec![Value::U64(id)];
            full_args.extend(args);
            builtin_write(&mut crate::parsing::ast::NoopEnv, full_args)
        }))),
    );
    map.insert(
        "writeText".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let mut full_args = vec![Value::U64(id)];
            full_args.extend(args);
            builtin_write(&mut crate::parsing::ast::NoopEnv, full_args)
        }))),
    );
    map.insert(
        "writeBytes".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let mut full_args = vec![Value::U64(id)];
            full_args.extend(args);
            builtin_write(&mut crate::parsing::ast::NoopEnv, full_args)
        }))),
    );
    map.insert(
        "version".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            builtin_version(&mut crate::parsing::ast::NoopEnv, vec![Value::U64(id)])
        }))),
    );
    map.insert(
        "alpn".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            builtin_alpn(&mut crate::parsing::ast::NoopEnv, vec![Value::U64(id)])
        }))),
    );
    map.insert(
        "peerCertificates".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            builtin_peer_certificates(&mut crate::parsing::ast::NoopEnv, vec![Value::U64(id)])
        }))),
    );
    map.insert(
        "trace".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            builtin_trace(&mut crate::parsing::ast::NoopEnv, vec![Value::U64(id)])
        }))),
    );
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            builtin_close(&mut crate::parsing::ast::NoopEnv, vec![Value::U64(id)])
        }))),
    );
    Value::Object(Arc::new(map))
}
