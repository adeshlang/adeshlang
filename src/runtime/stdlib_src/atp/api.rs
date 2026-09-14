//! ATP API — AdeshLang Integration

use super::config::AtpConfig;
use super::engine::{AtpEngine, Delivery, SendRequest};
use super::errors::AtpError;
use super::id::ConnectionId;
use super::identity::IdentityKeyPair;
use super::stream::Priority;
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::stdlib::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

static NEXT_HANDLE: AtomicU64 = AtomicU64::new(1);

/// Weak registry: engines are primarily owned by Adesh Value objects.
static ENGINES: Mutex<Option<HashMap<u64, Weak<AtpEngine>>>> = Mutex::new(None);

fn engines() -> std::sync::MutexGuard<'static, Option<HashMap<u64, Weak<AtpEngine>>>> {
    let mut guard = ENGINES.lock().unwrap();
    if guard.is_none() {
        *guard = Some(HashMap::new());
    }
    guard
}

fn register_engine(handle: u64, engine: &Arc<AtpEngine>) {
    if let Some(map) = engines().as_mut() {
        map.insert(handle, Arc::downgrade(engine));
    }
}

#[allow(dead_code)]
fn resolve_engine(handle: u64) -> Option<Arc<AtpEngine>> {
    let guard = engines();
    guard.as_ref()?.get(&handle)?.upgrade()
}

fn unregister_engine(handle: u64) {
    if let Some(map) = engines().as_mut() {
        map.remove(&handle);
    }
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register("atp_listen", "atp", "Listen for ATP connections", builtin_atp_listen);
    registry.register("atp_connect", "atp", "Connect to an ATP server", builtin_atp_connect);
    registry.register("atp_generate_identity_key", "atp", "Generate Ed25519 identity key", builtin_generate_identity_key);
    registry.register("atp_get_public_key", "atp", "Get Ed25519 public key from private key", builtin_get_public_key);
    registry.register("atp_default_config", "atp", "Get default ATP config", builtin_default_config);
}

fn unwrap_value(mut v: &Value) -> &Value {
    loop {
        match v {
            Value::Ref(inner, _) => v = inner.as_ref(),
            _ => return v,
        }
    }
}

fn val_as_slice(v: &Value) -> Option<&[Value]> {
    let v = unwrap_value(v);
    match v {
        Value::Array(arr) => Some(arr.as_slice()),
        Value::RawArray(_, arr) => Some(arr.as_slice()),
        Value::DynArray(da) => Some(da.data.as_slice()),
        _ => None,
    }
}

fn parse_32_bytes(v: &Value, field: &str) -> Result<[u8; 32], String> {
    let v = unwrap_value(v);
    match v {
        Value::Str(hex_str) => {
            let bytes = hex::decode(hex_str.trim()).map_err(|e| format!("invalid hex in {}: {}", field, e))?;
            if bytes.len() != 32 {
                return Err(format!("{} must be 32 bytes (got {})", field, bytes.len()));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&bytes);
            Ok(arr)
        }
        _ => {
            if let Some(arr) = val_as_slice(v) {
                if arr.len() != 32 {
                    return Err(format!("{} array must have 32 elements (got {})", field, arr.len()));
                }
                let mut out = [0u8; 32];
                for (i, elem) in arr.iter().enumerate() {
                    let elem = unwrap_value(elem);
                    out[i] = match elem {
                        Value::U8(b) => *b,
                        Value::Number(n) if *n >= 0.0 && *n <= 255.0 => *n as u8,
                        Value::BigInt(b) => {
                            use num_traits::ToPrimitive;
                            b.to_u8().ok_or_else(|| format!("byte out of range at index {}", i))?
                        }
                        Value::I32(n) if *n >= 0 && *n <= 255 => *n as u8,
                        Value::I64(n) if *n >= 0 && *n <= 255 => *n as u8,
                        Value::U32(n) if *n <= 255 => *n as u8,
                        Value::U64(n) if *n <= 255 => *n as u8,
                        _ => return Err(format!("invalid byte at index {} in {}", i, field)),
                    };
                }
                Ok(out)
            } else {
                Err(format!("{} must be hex string or 32-byte array", field))
            }
        }
    }
}

fn parse_config(v: Option<&Value>) -> Result<AtpConfig, String> {
    let mut config = AtpConfig::default();
    let v = match v {
        Some(val) => unwrap_value(val),
        None => return Ok(config),
    };
    let obj = match v {
        Value::Object(map) => map,
        Value::Null => return Ok(config),
        _ => return Err("config must be an object".to_string()),
    };

    if let Some(key_val) = obj.get("identityKey").or_else(|| obj.get("localIdentity")) {
        let bytes = parse_32_bytes(key_val, "identityKey")?;
        config.local_identity = Some(Arc::new(IdentityKeyPair::from_seed(bytes)));
    }

    if let Some(trusted_val) = obj.get("trustedPeers").or_else(|| obj.get("trustedPeerKeys")) {
        let trusted_val = unwrap_value(trusted_val);
        if let Some(arr) = val_as_slice(trusted_val) {
            let mut peers = Vec::new();
            for item in arr {
                peers.push(parse_32_bytes(item, "trustedPeer")?);
            }
            config.trusted_peer_keys = peers;
        } else {
            return Err("trustedPeers must be an array of 32-byte keys".to_string());
        }
    }

    if let Some(req_val) = obj.get("requireIdentity").or_else(|| obj.get("requirePeerIdentity")) {
        let req_val = unwrap_value(req_val);
        if let Value::Bool(b) = req_val {
            config.require_peer_identity = *b;
        }
    }

    if let Some(ttl_val) = obj.get("retryTokenTtl") {
        if let Some(secs) = parse_u64_opt(Some(ttl_val)) {
            config.retry_token_ttl = std::time::Duration::from_secs(secs);
        }
    }

    if let Some(rate_val) = obj.get("rateLimit").or_else(|| obj.get("handshakeRateLimit")) {
        if let Some(lim) = parse_u64_opt(Some(rate_val)) {
            config.handshake_rate_limit = lim as u32;
        }
    }

    if let Some(psize_val) = obj.get("maxPacketSize") {
        if let Some(sz) = parse_u64_opt(Some(psize_val)) {
            config.max_packet_size = sz as usize;
        }
    }

    if let Some(idle_val) = obj.get("idleTimeout") {
        if let Some(secs) = parse_u64_opt(Some(idle_val)) {
            config.idle_timeout = std::time::Duration::from_secs(secs);
        }
    }

    if let Some(hs_val) = obj.get("handshakeTimeout") {
        if let Some(secs) = parse_u64_opt(Some(hs_val)) {
            config.handshake_timeout = std::time::Duration::from_secs(secs);
        }
    }

    if let Some(streams_val) = obj.get("maxStreams") {
        if let Some(s) = parse_u64_opt(Some(streams_val)) {
            config.max_streams = s;
        }
    }

    Ok(config)
}

fn builtin_atp_listen(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let host = args.first().map(unwrap_value).and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }).unwrap_or("0.0.0.0");
    let port = parse_port(args.get(1))?;
    let config = parse_config(args.get(2))?;
    atp_listen(host, port, config).map_err(|e| e.to_string())
}

fn builtin_atp_connect(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let host = args.first().map(unwrap_value).and_then(|v| match v {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }).ok_or("atp.connect requires (host, port)")?;
    let port = parse_port(args.get(1))?;
    let config = parse_config(args.get(2))?;
    atp_connect(host, port, config).map_err(|e| e.to_string())
}

fn builtin_generate_identity_key(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let keypair = IdentityKeyPair::generate();
    let priv_bytes = keypair.seed_bytes();
    let pub_bytes = keypair.public_bytes();
    let mut obj = FastMap::default();
    obj.insert("privateKey".to_string(), Value::Array(priv_bytes.iter().map(|b| Value::U8(*b)).collect()));
    obj.insert("publicKey".to_string(), Value::Array(pub_bytes.iter().map(|b| Value::U8(*b)).collect()));
    obj.insert("privateKeyHex".to_string(), Value::Str(hex::encode(priv_bytes)));
    obj.insert("publicKeyHex".to_string(), Value::Str(hex::encode(pub_bytes)));
    Ok(Value::Object(Arc::new(obj)))
}

fn builtin_get_public_key(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let key_val = args.first().ok_or("getPublicKey requires privateKey")?;
    let priv_bytes = parse_32_bytes(key_val, "privateKey")?;
    let keypair = IdentityKeyPair::from_seed(priv_bytes);
    let pub_bytes = keypair.public_bytes();
    let mut obj = FastMap::default();
    obj.insert("publicKey".to_string(), Value::Array(pub_bytes.iter().map(|b| Value::U8(*b)).collect()));
    obj.insert("publicKeyHex".to_string(), Value::Str(hex::encode(pub_bytes)));
    Ok(Value::Object(Arc::new(obj)))
}

fn builtin_default_config(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let mut obj = FastMap::default();
    obj.insert("maxPacketSize".to_string(), Value::U64(1200));
    obj.insert("maxMessageSize".to_string(), Value::U64(64 * 1024 * 1024));
    obj.insert("maxStreams".to_string(), Value::U64(256));
    obj.insert("idleTimeout".to_string(), Value::U64(30));
    obj.insert("handshakeTimeout".to_string(), Value::U64(10));
    obj.insert("retryTokenTtl".to_string(), Value::U64(30));
    obj.insert("handshakeRateLimit".to_string(), Value::U64(10));
    obj.insert("requireIdentity".to_string(), Value::Bool(false));
    Ok(Value::Object(Arc::new(obj)))
}

fn parse_port(v: Option<&Value>) -> Result<u16, String> {
    let v = match v {
        Some(val) => unwrap_value(val),
        None => return Err("missing port".to_string()),
    };
    match v {
        Value::Number(n) if *n >= 0.0 && *n <= 65535.0 => Ok(*n as u16),
        Value::BigInt(b) => {
            use num_traits::ToPrimitive;
            b.to_u16().ok_or_else(|| "port out of range".to_string())
        }
        Value::Str(s) => s.trim().parse::<u16>().map_err(|_| format!("invalid port string '{}'", s)),
        Value::U8(p) => Ok(*p as u16),
        Value::U16(p) => Ok(*p),
        Value::U32(p) if *p <= 65535 => Ok(*p as u16),
        Value::U64(p) if *p <= 65535 => Ok(*p as u16),
        Value::I8(p) if *p >= 0 => Ok(*p as u16),
        Value::I16(p) if *p >= 0 => Ok(*p as u16),
        Value::I32(p) if *p >= 0 && *p <= 65535 => Ok(*p as u16),
        Value::I64(p) if *p >= 0 && *p <= 65535 => Ok(*p as u16),
        _ => Err("invalid port".to_string()),
    }
}

fn parse_host(host: &str) -> Result<IpAddr, String> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Ok(ip);
    }
    let cleaned = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = cleaned.parse::<IpAddr>() {
        return Ok(ip);
    }
    let addrs = format!("{}:0", host)
        .to_socket_addrs()
        .map_err(|e| format!("DNS failed for '{}': {}", host, e))?;
    addrs
        .map(|sa| sa.ip())
        .next()
        .ok_or_else(|| format!("no addresses for '{}'", host))
}

fn atp_listen(host: &str, port: u16, config: AtpConfig) -> Result<Value, AtpError> {
    let ip = parse_host(host).map_err(AtpError::transport)?;
    let addr = SocketAddr::new(ip, port);
    let engine = AtpEngine::new_server_with_config(addr, config)?;
    engine.start();
    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    register_engine(handle, &engine);
    Ok(build_server_object(handle, engine))
}

fn atp_connect(host: &str, port: u16, config: AtpConfig) -> Result<Value, AtpError> {
    let ip = parse_host(host).map_err(AtpError::transport)?;
    let addr = SocketAddr::new(ip, port);
    let engine = AtpEngine::new_client_with_config(addr, config)?;
    let conn_id = engine.ensure_client_handshake()?;
    engine.start();

    let handle = NEXT_HANDLE.fetch_add(1, Ordering::SeqCst);
    register_engine(handle, &engine);

    Ok(build_connection_object(handle, engine, conn_id))
}

pub fn build_atp_module_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "listen".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_atp_listen))),
    );
    map.insert(
        "connect".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_atp_connect))),
    );
    map.insert(
        "generateIdentityKey".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_generate_identity_key))),
    );
    map.insert(
        "getPublicKey".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_get_public_key))),
    );
    map.insert(
        "defaultConfig".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_default_config))),
    );

    let mut priority_map = FastMap::default();
    priority_map.insert("Critical".to_string(), Value::Str("critical".to_string()));
    priority_map.insert("High".to_string(), Value::Str("high".to_string()));
    priority_map.insert("Medium".to_string(), Value::Str("medium".to_string()));
    priority_map.insert("Low".to_string(), Value::Str("low".to_string()));
    map.insert("Priority".to_string(), Value::Object(Arc::new(priority_map)));

    let mut delivery_map = FastMap::default();
    delivery_map.insert("Connected".to_string(), Value::Str("connected".to_string()));
    delivery_map.insert("Message".to_string(), Value::Str("message".to_string()));
    delivery_map.insert("Datagram".to_string(), Value::Str("datagram".to_string()));
    delivery_map.insert("Closed".to_string(), Value::Str("closed".to_string()));
    delivery_map.insert("StreamReset".to_string(), Value::Str("stream_reset".to_string()));
    delivery_map.insert("StreamClosed".to_string(), Value::Str("stream_closed".to_string()));
    map.insert("DeliveryType".to_string(), Value::Object(Arc::new(delivery_map)));

    Value::Object(Arc::new(map))
}

fn build_server_object(handle: u64, engine: Arc<AtpEngine>) -> Value {
    let mut map = FastMap::default();
    map.insert("id".to_string(), Value::U64(handle));
    map.insert("type".to_string(), Value::Str("AtpServer".to_string()));

    let e1 = engine.clone();
    map.insert(
        "accept".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let deliveries = e1.poll_deliveries();
            for d in deliveries {
                if let Delivery::Connected { connection_id } = d {
                    return Ok(build_connection_object(handle, e1.clone(), connection_id));
                }
            }
            Ok(Value::Null) // Non-blocking poll: no connection yet.
        }))),
    );

    let e2 = engine.clone();
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            e2.stop();
            unregister_engine(handle);
            Ok(Value::Bool(true))
        }))),
    );

    let e3 = engine.clone();
    map.insert(
        "metrics".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(engine_metrics_value(&e3.metrics()))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn build_connection_object(
    engine_handle: u64,
    engine: Arc<AtpEngine>,
    connection_id: ConnectionId,
) -> Value {
    let mut map = FastMap::default();
    map.insert("id".to_string(), Value::U64(connection_id.0));
    map.insert("type".to_string(), Value::Str("AtpConnection".to_string()));
    map.insert(
        "connectionId".to_string(),
        Value::U64(connection_id.0),
    );

    // Ensure default stream exists conceptually (stream 1 for client-initiated).
    let default_stream = if engine.is_server { 2 } else { 1 };

    let e1 = engine.clone();
    let cid1 = connection_id;
    map.insert(
        "send".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let (data, reliable, ordered, stream_id) = parse_send_args(&args, default_stream)?;
            e1.submit_request(SendRequest::Message {
                connection_id: cid1,
                stream_id,
                data,
                reliable,
                ordered,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e2 = engine.clone();
    let cid2 = connection_id;
    map.insert(
        "receive".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            poll_connection_delivery(&e2, cid2)
        }))),
    );

    let e3 = engine.clone();
    let cid3 = connection_id;
    map.insert(
        "openStream".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let priority = parse_priority(args.first());
            e3.submit_request(SendRequest::OpenStream {
                connection_id: cid3,
                priority,
            });
            // Stream ID assigned asynchronously; return handle bound to connection.
            Ok(build_stream_object(engine_handle, e3.clone(), cid3, 0))
        }))),
    );

    let e4 = engine.clone();
    let cid4 = connection_id;
    map.insert(
        "datagram".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let data = parse_data(args.first())?;
            e4.submit_request(SendRequest::Datagram {
                connection_id: cid4,
                data,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e5 = engine.clone();
    let cid5 = connection_id;
    map.insert(
        "ping".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            e5.submit_request(SendRequest::Ping {
                connection_id: cid5,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e6 = engine.clone();
    let cid6 = connection_id;
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            e6.submit_request(SendRequest::Close {
                connection_id: cid6,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e7 = engine.clone();
    let cid7 = connection_id;
    map.insert(
        "cancel".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let stream_id = parse_u64(args.first(), "streamId")?;
            let message_id = parse_u64(args.get(1), "messageId")?;
            e7.submit_request(SendRequest::Cancel {
                connection_id: cid7,
                stream_id,
                message_id,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e8 = engine.clone();
    let cid8 = connection_id;
    map.insert(
        "metrics".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let m = e8
                .connection_metrics(cid8)
                .unwrap_or_default();
            Ok(connection_metrics_value(&m))
        }))),
    );

    let e9 = engine.clone();
    let cid9 = connection_id;
    map.insert(
        "migrate".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let host = args.first().and_then(|v| match v {
                Value::Str(s) => Some(s.as_str()),
                _ => None,
            }).ok_or("migrate requires (host, port)")?;
            let port = parse_port(args.get(1))?;
            let ip = parse_host(host).map_err(|e| format!("migrate host resolution failed: {}", e))?;
            let new_addr = SocketAddr::new(ip, port);
            e9.submit_request(SendRequest::Migrate {
                connection_id: cid9,
                new_addr,
            });
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn build_stream_object(
    _engine_handle: u64,
    engine: Arc<AtpEngine>,
    connection_id: ConnectionId,
    stream_id: u64,
) -> Value {
    let mut map = FastMap::default();
    map.insert("type".to_string(), Value::Str("AtpStream".to_string()));
    map.insert("connectionId".to_string(), Value::U64(connection_id.0));
    map.insert("streamId".to_string(), Value::U64(stream_id));

    let e1 = engine.clone();
    map.insert(
        "send".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let (data, reliable, ordered, _) = parse_send_args(&args, stream_id)?;
            e1.submit_request(SendRequest::Message {
                connection_id,
                stream_id,
                data,
                reliable,
                ordered,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e2 = engine.clone();
    map.insert(
        "receive".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            poll_connection_delivery(&e2, connection_id)
        }))),
    );

    let e3 = engine.clone();
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            e3.submit_request(SendRequest::CloseStream {
                connection_id,
                stream_id,
            });
            Ok(Value::Bool(true))
        }))),
    );

    let e4 = engine.clone();
    map.insert(
        "cancel".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let message_id = parse_u64(args.first(), "messageId")?;
            e4.submit_request(SendRequest::Cancel {
                connection_id,
                stream_id,
                message_id,
            });
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn poll_connection_delivery(engine: &Arc<AtpEngine>, conn_id: ConnectionId) -> Result<Value, String> {
    let deliveries = engine.poll_deliveries();
    for d in deliveries {
        match d {
            Delivery::Message {
                connection_id,
                stream_id,
                message_id,
                data,
            } if connection_id == conn_id => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("message".to_string()));
                msg.insert("streamId".to_string(), Value::U64(stream_id));
                msg.insert("messageId".to_string(), Value::U64(message_id));
                if let Ok(text) = std::str::from_utf8(&data) {
                    msg.insert("text".to_string(), Value::Str(text.to_string()));
                }
                msg.insert(
                    "data".to_string(),
                    Value::Array(data.into_iter().map(Value::U8).collect()),
                );
                return Ok(Value::Object(Arc::new(msg)));
            }
            Delivery::Datagram {
                connection_id,
                data,
            } if connection_id == conn_id => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("datagram".to_string()));
                if let Ok(text) = std::str::from_utf8(&data) {
                    msg.insert("text".to_string(), Value::Str(text.to_string()));
                }
                msg.insert(
                    "data".to_string(),
                    Value::Array(data.into_iter().map(Value::U8).collect()),
                );
                return Ok(Value::Object(Arc::new(msg)));
            }
            Delivery::Connected { connection_id } if connection_id == conn_id => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("connected".to_string()));
                msg.insert("connectionId".to_string(), Value::U64(connection_id.0));
                return Ok(Value::Object(Arc::new(msg)));
            }
            Delivery::Closed {
                connection_id,
                reason,
            } if connection_id == conn_id => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("closed".to_string()));
                msg.insert("reason".to_string(), Value::Str(reason));
                return Ok(Value::Object(Arc::new(msg)));
            }
            Delivery::StreamOpen {
                connection_id,
                stream_id,
            } if connection_id == conn_id => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("streamOpen".to_string()));
                msg.insert("streamId".to_string(), Value::U64(stream_id));
                return Ok(Value::Object(Arc::new(msg)));
            }
            Delivery::StreamClose {
                connection_id,
                stream_id,
            } if connection_id == conn_id => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("streamClose".to_string()));
                msg.insert("streamId".to_string(), Value::U64(stream_id));
                return Ok(Value::Object(Arc::new(msg)));
            }
            Delivery::Error {
                connection_id,
                error,
            } if connection_id == Some(conn_id) => {
                let mut msg = FastMap::default();
                msg.insert("type".to_string(), Value::Str("error".to_string()));
                msg.insert("error".to_string(), Value::Str(error.to_string()));
                return Ok(Value::Object(Arc::new(msg)));
            }
            _ => {}
        }
    }
    Ok(Value::Null)
}

fn parse_data(v: Option<&Value>) -> Result<Vec<u8>, String> {
    let v = match v {
        Some(val) => unwrap_value(val),
        None => return Err("missing data".to_string()),
    };
    match v {
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        _ => {
            if let Some(arr) = val_as_slice(v) {
                Ok(arr
                    .iter()
                    .map(unwrap_value)
                    .filter_map(|v| match v {
                        Value::U8(b) => Some(*b),
                        Value::Number(n) if *n >= 0.0 && *n <= 255.0 => Some(*n as u8),
                        Value::BigInt(b) => {
                            use num_traits::ToPrimitive;
                            b.to_u8()
                        }
                        Value::I32(n) if *n >= 0 && *n <= 255 => Some(*n as u8),
                        Value::I64(n) if *n >= 0 && *n <= 255 => Some(*n as u8),
                        Value::U32(n) if *n <= 255 => Some(*n as u8),
                        Value::U64(n) if *n <= 255 => Some(*n as u8),
                        _ => None,
                    })
                    .collect())
            } else {
                Err("data must be string or byte array".to_string())
            }
        }
    }
}

fn parse_send_args(
    args: &[Value],
    default_stream: u64,
) -> Result<(Vec<u8>, bool, bool, u64), String> {
    if args.is_empty() {
        return Err("send requires data".to_string());
    }
    let data = parse_data(args.first())?;
    let reliable = args
        .get(1)
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(true);
    let ordered = args
        .get(2)
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(true);
    let stream_id = args
        .get(3)
        .map(|v| parse_u64(Some(v), "streamId"))
        .transpose()?
        .unwrap_or(default_stream);
    Ok((data, reliable, ordered, stream_id))
}

fn parse_u64(v: Option<&Value>, name: &str) -> Result<u64, String> {
    match v {
        Some(Value::U64(n)) => Ok(*n),
        Some(Value::Number(n)) if *n >= 0.0 => Ok(*n as u64),
        Some(Value::BigInt(b)) => {
            use num_traits::ToPrimitive;
            b.to_u64().ok_or_else(|| format!("{} out of range", name))
        }
        Some(Value::U32(n)) => Ok(*n as u64),
        Some(Value::I32(n)) if *n >= 0 => Ok(*n as u64),
        Some(Value::I64(n)) if *n >= 0 => Ok(*n as u64),
        _ => Err(format!("invalid {}", name)),
    }
}

fn parse_u64_opt(v: Option<&Value>) -> Option<u64> {
    match v {
        Some(Value::U64(n)) => Some(*n),
        Some(Value::Number(n)) if *n >= 0.0 => Some(*n as u64),
        Some(Value::BigInt(b)) => {
            use num_traits::ToPrimitive;
            b.to_u64()
        }
        Some(Value::U32(n)) => Some(*n as u64),
        Some(Value::I32(n)) if *n >= 0 => Some(*n as u64),
        Some(Value::I64(n)) if *n >= 0 => Some(*n as u64),
        _ => None,
    }
}

fn parse_priority(v: Option<&Value>) -> Priority {
    match v {
        Some(Value::Str(s)) => match s.as_str() {
            "low" => Priority::Low,
            "high" => Priority::High,
            "critical" => Priority::Critical,
            _ => Priority::Medium,
        },
        Some(Value::Number(n)) => match *n as u8 {
            0 => Priority::Low,
            2 => Priority::High,
            3 => Priority::Critical,
            _ => Priority::Medium,
        },
        Some(Value::I32(n)) => match *n {
            0 => Priority::Low,
            2 => Priority::High,
            3 => Priority::Critical,
            _ => Priority::Medium,
        },
        Some(Value::U32(n)) => match *n {
            0 => Priority::Low,
            2 => Priority::High,
            3 => Priority::Critical,
            _ => Priority::Medium,
        },
        _ => Priority::Medium,
    }
}

fn engine_metrics_value(m: &super::engine::EngineMetrics) -> Value {
    let mut obj = FastMap::default();
    obj.insert("connections".to_string(), Value::U64(m.connections as u64));
    obj.insert("established".to_string(), Value::U64(m.established as u64));
    obj.insert("streams".to_string(), Value::U64(m.streams as u64));
    obj.insert("memoryUsed".to_string(), Value::U64(m.memory_used as u64));
    obj.insert("bytesSent".to_string(), Value::U64(m.bytes_sent));
    obj.insert("bytesReceived".to_string(), Value::U64(m.bytes_received));
    Value::Object(Arc::new(obj))
}

fn connection_metrics_value(m: &super::connection::ConnectionMetrics) -> Value {
    let mut obj = FastMap::default();
    obj.insert("bytesSent".to_string(), Value::U64(m.bytes_sent));
    obj.insert("bytesReceived".to_string(), Value::U64(m.bytes_received));
    obj.insert("packetsSent".to_string(), Value::U64(m.packets_sent));
    obj.insert("packetsReceived".to_string(), Value::U64(m.packets_received));
    obj.insert("messagesSent".to_string(), Value::U64(m.messages_sent));
    obj.insert(
        "messagesReceived".to_string(),
        Value::U64(m.messages_received),
    );
    obj.insert("connections".to_string(), Value::U64(1));
    obj.insert("established".to_string(), Value::U64(1));
    obj.insert("streams".to_string(), Value::U64(1));
    obj.insert("memoryUsed".to_string(), Value::U64(0));
    Value::Object(Arc::new(obj))
}
