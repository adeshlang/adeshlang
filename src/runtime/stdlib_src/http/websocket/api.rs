use super::connection::WebSocketMessage;
use super::frame::{WebSocketFrame, WebSocketOpcode};
use super::handshake::{calculate_websocket_accept, generate_websocket_key};
use super::session::{
    STATE_CLOSED, STATE_CLOSING, STATE_CONNECTING, STATE_FAILED, STATE_HANDSHAKING, STATE_OPEN,
    WsClientOptions, WsIoOwned, WsServerOptions, WsSession, accept_session, bind_listener,
    connect_uri, echo_until_close, handshake_existing,
};
use crate::parsing::ast::{BuiltinEnv, NativeFn, UserFn, Value};
use crate::runtime::stdlib_src::http::errors::{HttpError, HttpErrorKind};
use crate::utils::collections::FastMap;
use once_cell::sync::Lazy;
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// `Value` is only used on the interpreter thread; this wrapper lets the
/// server registry live in a process-wide `static`.
struct SendValue(Value);
unsafe impl Send for SendValue {}
unsafe impl Sync for SendValue {}

impl Clone for SendValue {
    fn clone(&self) -> Self {
        SendValue(self.0.clone())
    }
}

struct WsConnEntry {
    session: WsSession,
}

struct WsServerEntry {
    listener: Option<TcpListener>,
    options: WsServerOptions,
    handler: Option<SendValue>,
    stop: Arc<AtomicBool>,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static CONNECTIONS: Lazy<Mutex<FastMap<u64, Arc<Mutex<WsConnEntry>>>>> =
    Lazy::new(|| Mutex::new(FastMap::default()));
static SERVERS: Lazy<Mutex<FastMap<u64, Arc<Mutex<WsServerEntry>>>>> =
    Lazy::new(|| Mutex::new(FastMap::default()));
static ROOMS: Lazy<Mutex<FastMap<String, Vec<u64>>>> = Lazy::new(|| Mutex::new(FastMap::default()));

struct BoundedThreadPool {
    sender: std::sync::mpsc::SyncSender<Box<dyn FnOnce() + Send + 'static>>,
}

impl BoundedThreadPool {
    fn new(num_workers: usize, max_queue: usize) -> Self {
        let (tx, rx) =
            std::sync::mpsc::sync_channel::<Box<dyn FnOnce() + Send + 'static>>(max_queue);
        let rx = Arc::new(Mutex::new(rx));
        for _ in 0..num_workers {
            let rx = rx.clone();
            std::thread::spawn(move || {
                loop {
                    let task = {
                        let lock = rx.lock().unwrap();
                        match lock.recv() {
                            Ok(task) => task,
                            Err(_) => break,
                        }
                    };
                    task();
                }
            });
        }
        Self { sender: tx }
    }

    fn execute<F>(&self, job: F) -> Result<(), String>
    where
        F: FnOnce() + Send + 'static,
    {
        self.sender
            .try_send(Box::new(job))
            .map_err(|e| format!("Worker pool queue full: {e}"))
    }
}

static SERVER_WORKER_POOL: Lazy<BoundedThreadPool> = Lazy::new(|| {
    let workers = (num_cpus::get() * 4).max(8);
    BoundedThreadPool::new(workers, 10_000)
});

fn alloc_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

pub fn build_websocket_module_object() -> Value {
    let mut map = FastMap::default();

    map.insert("Opcode".to_string(), opcode_object());
    map.insert("State".to_string(), state_object());
    map.insert("CloseCode".to_string(), close_code_object());

    map.insert(
        "ClientConfig".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let mut cfg = client_config_object();
            if let Some(Value::Object(map)) = args.first() {
                let mut new_map = match &cfg {
                    Value::Object(m) => (**m).clone(),
                    _ => FastMap::default(),
                };
                for (k, v) in map.iter() {
                    new_map.insert(k.clone(), v.clone());
                }
                cfg = Value::Object(Arc::new(new_map));
            }
            Ok(cfg)
        }))),
    );
    map.insert(
        "ServerConfig".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let mut cfg = server_config_object();
            if let Some(Value::Object(map)) = args.first() {
                let mut new_map = match &cfg {
                    Value::Object(m) => (**m).clone(),
                    _ => FastMap::default(),
                };
                for (k, v) in map.iter() {
                    new_map.insert(k.clone(), v.clone());
                }
                cfg = Value::Object(Arc::new(new_map));
            }
            Ok(cfg)
        }))),
    );
    map.insert(
        "WebSocketError".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let kind = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("Error")
                .to_string();
            let message = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            let cause = args.get(2).cloned().unwrap_or(Value::Null);
            Ok(error_object(&kind, &message, cause))
        }))),
    );
    map.insert(
        "WebSocketMessage".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| Ok(message_from_ctor(args))))),
    );
    map.insert(
        "WebSocketFrame".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| Ok(frame_from_ctor(args))))),
    );
    map.insert(
        "WebSocketConnection".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| wrap_existing_ctor(args)))),
    );
    map.insert(
        "WebSocketServer".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_server))),
    );

    map.insert(
        "connect".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_connect))),
    );
    map.insert(
        "connectAsync".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_connect_async))),
    );
    map.insert(
        "server".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_server))),
    );
    map.insert(
        "isError".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            Ok(Value::Bool(is_ws_error(
                args.first().unwrap_or(&Value::Null),
            )))
        }))),
    );
    map.insert(
        "computeAcceptKey".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let key = args
                .first()
                .and_then(val_as_str)
                .ok_or_else(|| "computeAcceptKey requires a client key".to_string())?;
            Ok(Value::Str(calculate_websocket_accept(key)))
        }))),
    );
    map.insert(
        "generateKey".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            Ok(Value::Str(generate_websocket_key()))
        }))),
    );
    map.insert(
        "DTO".to_string(),
        crate::runtime::stdlib_src::http::api::build_dto_object(),
    );
    map.insert(
        "Schema".to_string(),
        crate::runtime::stdlib_src::http::api::build_schema_object(),
    );
    map.insert(
        "joinRoom".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let room = args.first().and_then(val_as_str).unwrap_or("default");
            let id = args.get(1).and_then(val_as_u64).unwrap_or(0);
            let mut rooms = ROOMS.lock().unwrap();
            rooms.entry(room.to_string()).or_default().push(id);
            Ok(Value::Bool(true))
        }))),
    );
    map.insert(
        "leaveRoom".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let room = args.first().and_then(val_as_str).unwrap_or("default");
            let id = args.get(1).and_then(val_as_u64).unwrap_or(0);
            let mut rooms = ROOMS.lock().unwrap();
            if let Some(list) = rooms.get_mut(room) {
                list.retain(|&x| x != id);
            }
            Ok(Value::Bool(true))
        }))),
    );
    map.insert(
        "broadcastRoom".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let room = args.first().and_then(val_as_str).unwrap_or("default");
            let msg = args.get(1).and_then(val_as_str).unwrap_or("");
            let ids = {
                let rooms = ROOMS.lock().unwrap();
                rooms.get(room).cloned().unwrap_or_default()
            };
            let mut count = 0;
            for id in ids {
                if let Some(entry_arc) = CONNECTIONS.lock().unwrap().get(&id).cloned() {
                    let mut entry = entry_arc.lock().unwrap();
                    if entry.session.send_text(msg).is_ok() {
                        count += 1;
                    }
                }
            }
            Ok(Value::Number(count as f64))
        }))),
    );
    map.insert(
        "broadcastRoomExcept".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let room = args.first().and_then(val_as_str).unwrap_or("default");
            let except_id = args.get(1).and_then(val_as_u64).unwrap_or(0);
            let msg = args.get(2).and_then(val_as_str).unwrap_or("");
            let ids = {
                let rooms = ROOMS.lock().unwrap();
                rooms.get(room).cloned().unwrap_or_default()
            };
            let mut count = 0;
            for id in ids {
                if id != except_id {
                    if let Some(entry_arc) = CONNECTIONS.lock().unwrap().get(&id).cloned() {
                        let mut entry = entry_arc.lock().unwrap();
                        if entry.session.send_text(msg).is_ok() {
                            count += 1;
                        }
                    }
                }
            }
            Ok(Value::Number(count as f64))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn builtin_connect(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let uri = args
        .first()
        .and_then(val_as_str)
        .ok_or_else(|| "WebSocket.connect requires a ws:// or wss:// URI".to_string())?;
    let options = parse_client_options(args.get(1));
    match connect_uri(uri, &options) {
        Ok(session) => Ok(connection_object(register_session(session))),
        Err(err) => Ok(http_to_ws_error(&err)),
    }
}

fn builtin_connect_async(env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let uri = args
        .first()
        .and_then(val_as_str)
        .ok_or_else(|| "WebSocket.connectAsync requires a ws:// or wss:// URI".to_string())?
        .to_string();
    let options = parse_client_options(args.get(1));
    spawn_promise_work(env, move || match connect_uri(&uri, &options) {
        Ok(session) => Ok(SendValue(connection_object(register_session(session)))),
        Err(err) => Ok(SendValue(http_to_ws_error(&err))),
    })
}

fn builtin_server(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let options = parse_server_options(args.first());
    let id = alloc_id();
    let entry = WsServerEntry {
        listener: None,
        options,
        handler: None,
        stop: Arc::new(AtomicBool::new(false)),
    };
    SERVERS
        .lock()
        .map_err(|_| "WebSocket server registry lock poisoned".to_string())?
        .insert(id, Arc::new(Mutex::new(entry)));
    Ok(server_object(id))
}

fn wrap_existing_ctor(args: Vec<Value>) -> Result<Value, String> {
    let socket = args.first().cloned().unwrap_or(Value::Null);
    let is_client = args.get(1).map(val_truthy).unwrap_or(true);
    let config = parse_client_options(args.get(2));
    if is_client {
        return Ok(pending_connection_object(socket, config));
    }
    Err("Server-side WebSocketConnection wrapping requires WebSocket.server()".to_string())
}

fn register_session(session: WsSession) -> u64 {
    let id = alloc_id();
    if let Ok(mut map) = CONNECTIONS.lock() {
        map.insert(id, Arc::new(Mutex::new(WsConnEntry { session })));
    }
    id
}

fn connection_object(id: u64) -> Value {
    let mut map = FastMap::default();
    map.insert("id".to_string(), Value::U64(id));
    map.insert("handle".to_string(), Value::U64(id));
    map.insert(
        "__type".to_string(),
        Value::Str("WebSocketConnection".to_string()),
    );
    map.insert(
        "isOpen".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            with_conn(id, |entry| Ok(Value::Bool(entry.session.is_open())))
        }))),
    );
    map.insert(
        "getState".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            with_conn(id, |entry| Ok(Value::Number(entry.session.state as f64)))
        }))),
    );
    map.insert(
        "subprotocol".to_string(),
        match CONNECTIONS.lock() {
            Ok(map) => map
                .get(&id)
                .and_then(|entry| entry.lock().ok())
                .and_then(|guard| guard.session.subprotocol.clone().map(Value::Str))
                .unwrap_or(Value::Null),
            Err(_) => Value::Null,
        },
    );
    map.insert(
        "isAlive".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            with_conn(id, |entry| Ok(Value::Bool(entry.session.is_open())))
        }))),
    );
    map.insert(
        "getState".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            with_conn(id, |entry| Ok(Value::Number(entry.session.state as f64)))
        }))),
    );
    map.insert(
        "lastPing".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| Ok(Value::Number(0.0))))),
    );
    map.insert(
        "lastPong".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| Ok(Value::Number(0.0))))),
    );
    map.insert(
        "sendText".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let text = args
                .first()
                .and_then(val_as_str)
                .ok_or_else(|| "sendText requires a string".to_string())?;
            with_conn(id, |entry| match entry.session.send_text(text) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "sendBinary".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let bytes = args
                .first()
                .map(value_to_bytes)
                .unwrap_or_else(|| Ok(Vec::new()))?;
            with_conn(id, |entry| match entry.session.send_binary(bytes.clone()) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "sendPing".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let bytes = args
                .first()
                .map(value_to_bytes)
                .transpose()?
                .unwrap_or_default();
            with_conn(id, |entry| match entry.session.send_ping(bytes.clone()) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "sendPong".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let bytes = args
                .first()
                .map(value_to_bytes)
                .transpose()?
                .unwrap_or_default();
            with_conn(id, |entry| match entry.session.send_pong(bytes.clone()) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "receive".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            with_conn(id, |entry| match entry.session.receive() {
                Ok(msg) => Ok(message_to_value(&msg)),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let code = args.first().and_then(val_as_u16).unwrap_or(1000);
            let reason = args.get(1).and_then(val_as_str).unwrap_or("");
            with_conn(id, |entry| match entry.session.close(code, reason) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "receiveAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            spawn_promise_work(env, move || {
                let res_val = with_conn(id, |entry| match entry.session.receive() {
                    Ok(msg) => Ok(message_to_value(&msg)),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "sendTextAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let text = args
                .first()
                .and_then(val_as_str)
                .ok_or_else(|| "sendTextAsync requires a string".to_string())?
                .to_string();
            spawn_promise_work(env, move || {
                let res_val = with_conn(id, |entry| match entry.session.send_text(&text) {
                    Ok(()) => Ok(Value::Null),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "sendBinaryAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let bytes = args
                .first()
                .map(value_to_bytes)
                .unwrap_or_else(|| Ok(Vec::new()))?;
            spawn_promise_work(env, move || {
                let res_val =
                    with_conn(id, |entry| match entry.session.send_binary(bytes.clone()) {
                        Ok(()) => Ok(Value::Null),
                        Err(err) => Ok(http_to_ws_error(&err)),
                    })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "sendPingAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let bytes = args
                .first()
                .map(value_to_bytes)
                .transpose()?
                .unwrap_or_default();
            spawn_promise_work(env, move || {
                let res_val =
                    with_conn(id, |entry| match entry.session.send_ping(bytes.clone()) {
                        Ok(()) => Ok(Value::Null),
                        Err(err) => Ok(http_to_ws_error(&err)),
                    })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "sendPongAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let bytes = args
                .first()
                .map(value_to_bytes)
                .transpose()?
                .unwrap_or_default();
            spawn_promise_work(env, move || {
                let res_val =
                    with_conn(id, |entry| match entry.session.send_pong(bytes.clone()) {
                        Ok(()) => Ok(Value::Null),
                        Err(err) => Ok(http_to_ws_error(&err)),
                    })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "closeAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let code = args.first().and_then(val_as_u16).unwrap_or(1000);
            let reason = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            spawn_promise_work(env, move || {
                let res_val = with_conn(id, |entry| match entry.session.close(code, &reason) {
                    Ok(()) => Ok(Value::Null),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "sendDTO".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let dto_def = args
                .first()
                .ok_or_else(|| "sendDTO requires DTO schema or instance".to_string())?;
            let data = args
                .get(1)
                .ok_or_else(|| "sendDTO requires data object".to_string())?;
            let json_str = validate_and_serialize_dto(env, dto_def, data)?;
            with_conn(id, |entry| match entry.session.send_text(&json_str) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "sendDTOAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let dto_def = args
                .first()
                .ok_or_else(|| "sendDTOAsync requires DTO schema".to_string())?
                .clone();
            let data = args
                .get(1)
                .ok_or_else(|| "sendDTOAsync requires data object".to_string())?
                .clone();
            let json_str = validate_and_serialize_dto(env, &dto_def, &data)?;
            spawn_promise_work(env, move || {
                let res_val = with_conn(id, |entry| match entry.session.send_text(&json_str) {
                    Ok(()) => Ok(Value::Null),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    map.insert(
        "receiveDTO".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let dto_def = args.first().cloned();
            with_conn(id, |entry| match entry.session.receive() {
                Ok(msg) => {
                    let text = extract_msg_text(&msg)?;
                    parse_and_validate_dto(env, dto_def.as_ref(), &text)
                }
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    map.insert(
        "receiveDTOAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let dto_def_send = args.first().cloned().map(SendValue);
            spawn_promise_work(env, move || {
                let res_val = with_conn(id, |entry| match entry.session.receive() {
                    Ok(msg) => {
                        let text = extract_msg_text(&msg)?;
                        let parsed: serde_json::Value =
                            serde_json::from_str(&text).map_err(|e| format!("InvalidJSON: {e}"))?;
                        let val =
                            crate::runtime::stdlib_src::http::api::serde_json_to_adesh_val(&parsed);
                        if let Some(SendValue(dto)) = &dto_def_send {
                            if let Value::Object(m) = dto {
                                if let Some(val_fn) = m.get("validate") {
                                    if let Value::Function(NativeFn(f)) = val_fn {
                                        let mut dummy_env = crate::parsing::ast::NoopEnv;
                                        if let Ok(res) = (f)(&mut dummy_env, vec![val.clone()]) {
                                            if is_ws_error(&res) {
                                                return Ok(res);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(val)
                    }
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    Value::Object(Arc::new(map))
}

fn pending_connection_object(socket: Value, options: WsClientOptions) -> Value {
    let socket = Arc::new(socket);
    let options = Arc::new(options);
    let session_id: Arc<Mutex<Option<u64>>> = Arc::new(Mutex::new(None));
    let mut map = FastMap::default();
    map.insert(
        "__type".to_string(),
        Value::Str("WebSocketConnection".to_string()),
    );

    let sid = session_id.clone();
    map.insert(
        "isOpen".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let id = *sid
                .lock()
                .map_err(|_| "WebSocket connection lock poisoned")?;
            match id {
                Some(id) => with_conn(id, |entry| Ok(Value::Bool(entry.session.is_open()))),
                None => Ok(Value::Bool(false)),
            }
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "getState".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let id = *sid
                .lock()
                .map_err(|_| "WebSocket connection lock poisoned")?;
            match id {
                Some(id) => with_conn(id, |entry| Ok(Value::Number(entry.session.state as f64))),
                None => Ok(Value::Number(STATE_CONNECTING as f64)),
            }
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "sendText".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let id = sid
                .lock()
                .map_err(|_| "WebSocket connection lock poisoned")?
                .ok_or_else(|| "WebSocket handshake has not completed".to_string())?;
            let text = args
                .first()
                .and_then(val_as_str)
                .ok_or_else(|| "sendText requires a string".to_string())?;
            with_conn(id, |entry| match entry.session.send_text(text) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "sendBinary".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let id = sid
                .lock()
                .map_err(|_| "WebSocket connection lock poisoned")?
                .ok_or_else(|| "WebSocket handshake has not completed".to_string())?;
            let bytes = args
                .first()
                .map(value_to_bytes)
                .unwrap_or_else(|| Ok(Vec::new()))?;
            with_conn(id, |entry| match entry.session.send_binary(bytes.clone()) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "receive".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let id = sid
                .lock()
                .map_err(|_| "WebSocket connection lock poisoned")?
                .ok_or_else(|| "WebSocket handshake has not completed".to_string())?;
            with_conn(id, |entry| match entry.session.receive() {
                Ok(msg) => Ok(message_to_value(&msg)),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let id = sid
                .lock()
                .map_err(|_| "WebSocket connection lock poisoned")?
                .ok_or_else(|| "WebSocket handshake has not completed".to_string())?;
            let code = args.first().and_then(val_as_u16).unwrap_or(1000);
            let reason = args.get(1).and_then(val_as_str).unwrap_or("");
            with_conn(id, |entry| match entry.session.close(code, reason) {
                Ok(()) => Ok(Value::Null),
                Err(err) => Ok(http_to_ws_error(&err)),
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "receiveAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            let sid_clone = sid.clone();
            spawn_promise_work(env, move || {
                let id = get_pending_sid(&sid_clone)?;
                let res_val = with_conn(id, |entry| match entry.session.receive() {
                    Ok(msg) => Ok(message_to_value(&msg)),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "sendTextAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let sid_clone = sid.clone();
            let text = args
                .first()
                .and_then(val_as_str)
                .ok_or_else(|| "sendTextAsync requires a string".to_string())?
                .to_string();
            spawn_promise_work(env, move || {
                let id = get_pending_sid(&sid_clone)?;
                let res_val = with_conn(id, |entry| match entry.session.send_text(&text) {
                    Ok(()) => Ok(Value::Null),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "sendBinaryAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let sid_clone = sid.clone();
            let bytes = args
                .first()
                .map(value_to_bytes)
                .unwrap_or_else(|| Ok(Vec::new()))?;
            spawn_promise_work(env, move || {
                let id = get_pending_sid(&sid_clone)?;
                let res_val =
                    with_conn(id, |entry| match entry.session.send_binary(bytes.clone()) {
                        Ok(()) => Ok(Value::Null),
                        Err(err) => Ok(http_to_ws_error(&err)),
                    })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    let sid = session_id.clone();
    map.insert(
        "closeAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let sid_clone = sid.clone();
            let code = args.first().and_then(val_as_u16).unwrap_or(1000);
            let reason = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            spawn_promise_work(env, move || {
                let id = get_pending_sid(&sid_clone)?;
                let res_val = with_conn(id, |entry| match entry.session.close(code, &reason) {
                    Ok(()) => Ok(Value::Null),
                    Err(err) => Ok(http_to_ws_error(&err)),
                })?;
                Ok(SendValue(res_val))
            })
        }))),
    );
    let socket_ref = socket.clone();
    let options_ref = options.clone();
    let sid = session_id.clone();
    map.insert(
        "performClientHandshake".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = url_from_handshake_arg(args.first())?;
            let key = args.get(1).and_then(val_as_str);
            let io = extract_io(&socket_ref)?;
            match handshake_existing(io, &url, key, options_ref.as_ref()) {
                Ok(session) => {
                    let id = register_session(session);
                    *sid.lock()
                        .map_err(|_| "WebSocket connection lock poisoned")? = Some(id);
                    Ok(Value::Null)
                }
                Err(err) => Ok(http_to_ws_error(&err)),
            }
        }))),
    );
    Value::Object(Arc::new(map))
}

fn server_object(id: u64) -> Value {
    let mut map = FastMap::default();
    map.insert("id".to_string(), Value::U64(id));
    map.insert(
        "__type".to_string(),
        Value::Str("WebSocketServer".to_string()),
    );
    map.insert(
        "onConnection".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let handler = args.first().cloned().unwrap_or(Value::Null);
            with_server(id, |server| {
                server.handler = Some(SendValue(handler.clone()));
                Ok(Value::Null)
            })
        }))),
    );
    map.insert(
        "run".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            bind_server(id)?;
            let addr = with_server(id, |server| {
                Ok(Value::Str(format!(
                    "{}:{}",
                    server.options.bind_address, server.options.port
                )))
            })?;
            if let Value::Str(addr) = &addr {
                println!("WebSocket server running on ws://{addr}");
            }
            loop {
                if server_should_stop(id)? {
                    break;
                }
                let listener = with_server(id, |server| {
                    server
                        .listener
                        .as_ref()
                        .ok_or_else(|| "WebSocket server listener is closed".to_string())?
                        .try_clone()
                        .map_err(|e| format!("Failed to clone WebSocket listener: {e}"))
                })?;
                let options = with_server(id, |server| Ok(server.options.clone()))?;
                let session = match accept_session(&listener, &options) {
                    Ok(session) => session,
                    Err(err) => {
                        if server_should_stop(id)? {
                            break;
                        }
                        eprintln!("WebSocket accept error: {err}");
                        continue;
                    }
                };
                let conn = connection_object(register_session(session));
                let handler = with_server(id, |server| {
                    Ok(server
                        .handler
                        .as_ref()
                        .map(|h| h.0.clone())
                        .unwrap_or(Value::Null))
                })?;
                if !matches!(handler, Value::Null) {
                    let _ = call_value(env, &handler, vec![conn]);
                }
            }
            Ok(Value::Null)
        }))),
    );
    map.insert(
        "startEcho".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            bind_server(id)?;
            let listener = with_server(id, |server| {
                server
                    .listener
                    .as_ref()
                    .ok_or_else(|| "WebSocket server listener is closed".to_string())?
                    .try_clone()
                    .map_err(|e| format!("Failed to clone WebSocket listener: {e}"))
            })?;
            let options = with_server(id, |server| Ok(server.options.clone()))?;
            let stop = with_server(id, |server| Ok(server.stop.clone()))?;
            let started = Arc::new(AtomicBool::new(false));
            let started_ref = started.clone();
            std::thread::spawn(move || {
                started_ref.store(true, Ordering::SeqCst);
                let _ = listener.set_nonblocking(true);
                while !stop.load(Ordering::SeqCst) {
                    match accept_session(&listener, &options) {
                        Ok(mut session) => {
                            let _ = SERVER_WORKER_POOL.execute(move || {
                                let _ = echo_until_close(&mut session);
                            });
                        }
                        Err(_) => {
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                            std::thread::sleep(Duration::from_millis(10));
                        }
                    }
                }
            });
            let start_time = std::time::Instant::now();
            while !started.load(Ordering::SeqCst)
                && start_time.elapsed() < Duration::from_millis(500)
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            std::thread::sleep(Duration::from_millis(50));
            Ok(Value::Bool(true))
        }))),
    );
    map.insert(
        "close".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            with_server(id, |server| {
                server.stop.store(true, Ordering::SeqCst);
                server.listener = None;
                let active_ids: Vec<u64> = CONNECTIONS.lock().unwrap().keys().cloned().collect();
                for conn_id in active_ids {
                    if let Some(entry_arc) = CONNECTIONS.lock().unwrap().get(&conn_id).cloned() {
                        if let Ok(mut entry) = entry_arc.lock() {
                            let _ = entry.session.close(1001, "Server shutting down");
                        }
                    }
                }
                Ok(Value::Null)
            })
        }))),
    );
    map.insert(
        "runAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            bind_server(id)?;
            let stop = with_server(id, |s| Ok(s.stop.clone()))?;
            let options = with_server(id, |s| Ok(s.options.clone()))?;
            let handler = with_server(id, |s| Ok(s.handler.clone())).unwrap_or(None);
            spawn_promise_work(env, move || {
                let listener = match with_server(id, |server| {
                    server
                        .listener
                        .as_ref()
                        .ok_or_else(|| "WebSocket server listener is closed".to_string())?
                        .try_clone()
                        .map_err(|e| format!("Failed to clone WebSocket listener: {e}"))
                }) {
                    Ok(l) => l,
                    Err(e) => return Ok(SendValue(error_object("ServerError", &e, Value::Null))),
                };
                while !stop.load(Ordering::SeqCst) {
                    match accept_session(&listener, &options) {
                        Ok(session) => {
                            let conn_id = register_session(session);
                            if let Some(ref h) = handler {
                                let conn = connection_object(conn_id);
                                let mut dummy_env = crate::parsing::ast::NoopEnv;
                                let _ = call_value(&mut dummy_env, &h.0, vec![conn]);
                            }
                        }
                        Err(_) => {
                            if stop.load(Ordering::SeqCst) {
                                break;
                            }
                        }
                    }
                }
                Ok(SendValue(Value::Null))
            })
        }))),
    );
    map.insert(
        "acceptAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            bind_server(id)?;
            spawn_promise_work(env, move || {
                let listener = with_server(id, |server| {
                    server
                        .listener
                        .as_ref()
                        .ok_or_else(|| "WebSocket server listener is closed".to_string())?
                        .try_clone()
                        .map_err(|e| format!("Failed to clone listener: {e}"))
                })?;
                let options = with_server(id, |s| Ok(s.options.clone()))?;
                match accept_session(&listener, &options) {
                    Ok(session) => {
                        let conn_id = register_session(session);
                        Ok(SendValue(connection_object(conn_id)))
                    }
                    Err(err) => Ok(SendValue(http_to_ws_error(&err))),
                }
            })
        }))),
    );
    map.insert(
        "broadcast".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let msg_val = args
                .first()
                .ok_or_else(|| "broadcast requires message".to_string())?;
            let text = match msg_val {
                Value::Str(s) => s.clone(),
                _ => {
                    let json_val =
                        crate::runtime::stdlib_src::http::api::adesh_val_to_serde_json(msg_val)?;
                    serde_json::to_string(&json_val).map_err(|e| e.to_string())?
                }
            };
            let count = broadcast_to_all(&text)?;
            Ok(Value::I64(count as i64))
        }))),
    );
    map.insert(
        "broadcastAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let msg_val = args
                .first()
                .ok_or_else(|| "broadcastAsync requires message".to_string())?;
            let text = match msg_val {
                Value::Str(s) => s.clone(),
                _ => {
                    let json_val =
                        crate::runtime::stdlib_src::http::api::adesh_val_to_serde_json(msg_val)?;
                    serde_json::to_string(&json_val).map_err(|e| e.to_string())?
                }
            };
            spawn_promise_work(env, move || {
                let count = broadcast_to_all(&text)?;
                Ok(SendValue(Value::I64(count as i64)))
            })
        }))),
    );
    map.insert(
        "closeAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            spawn_promise_work(env, move || {
                with_server(id, |server| {
                    server.stop.store(true, Ordering::SeqCst);
                    server.listener = None;
                    Ok(SendValue(Value::Null))
                })
            })
        }))),
    );
    Value::Object(Arc::new(map))
}

fn bind_server(id: u64) -> Result<(), String> {
    with_server(id, |server| {
        if server.listener.is_none() {
            let listener = bind_listener(&server.options).map_err(|e| e.to_string())?;
            server.listener = Some(listener);
        }
        Ok(Value::Null)
    })?;
    Ok(())
}

fn with_conn<F>(id: u64, f: F) -> Result<Value, String>
where
    F: FnOnce(&mut WsConnEntry) -> Result<Value, String>,
{
    let entry = {
        let map = CONNECTIONS
            .lock()
            .map_err(|_| "WebSocket connection registry lock poisoned".to_string())?;
        map.get(&id)
            .cloned()
            .ok_or_else(|| format!("WebSocket connection {id} not found or already closed"))?
    };
    let mut guard = entry
        .lock()
        .map_err(|_| "WebSocket connection lock poisoned".to_string())?;
    f(&mut guard)
}

fn with_server<F, T>(id: u64, f: F) -> Result<T, String>
where
    F: FnOnce(&mut WsServerEntry) -> Result<T, String>,
{
    let entry = {
        let map = SERVERS
            .lock()
            .map_err(|_| "WebSocket server registry lock poisoned".to_string())?;
        map.get(&id)
            .cloned()
            .ok_or_else(|| format!("WebSocket server {id} not found"))?
    };
    let mut guard = entry
        .lock()
        .map_err(|_| "WebSocket server lock poisoned".to_string())?;
    f(&mut guard)
}

fn server_should_stop(id: u64) -> Result<bool, String> {
    with_server(id, |server| Ok(server.stop.load(Ordering::SeqCst)))
}

fn call_value(env: &mut dyn BuiltinEnv, val: &Value, args: Vec<Value>) -> Result<Value, String> {
    match val {
        Value::Function(NativeFn(f)) => (f)(env, args),
        Value::UserFunction(u) => call_user(env, u, args),
        Value::BoundMethod(u, _) => call_user(env, u, args),
        _ => Err("WebSocket onConnection handler must be a function".to_string()),
    }
}

fn call_user(env: &mut dyn BuiltinEnv, u: &UserFn, args: Vec<Value>) -> Result<Value, String> {
    if let Some(interp) = env
        .as_any_mut()
        .downcast_mut::<crate::execution::runtime::Interpreter>()
    {
        interp.call_user_function(u, args)
    } else {
        crate::execution::runtime::public_call_user(
            u.clone(),
            args,
            None,
            env.native_side_effects(),
        )
    }
}

fn get_pending_sid(sid: &Arc<Mutex<Option<u64>>>) -> Result<u64, String> {
    sid.lock()
        .map_err(|_| "WebSocket connection lock poisoned".to_string())?
        .ok_or_else(|| "WebSocket handshake has not completed".to_string())
}

struct SendSideEffects(Arc<std::sync::Mutex<Vec<crate::parsing::ast::NativeEffect>>>);
unsafe impl Send for SendSideEffects {}
unsafe impl Sync for SendSideEffects {}

fn spawn_promise_work<F>(env: &mut dyn BuiltinEnv, work: F) -> Result<Value, String>
where
    F: FnOnce() -> Result<SendValue, String> + Send + 'static,
{
    let side_effects = env.native_side_effects().map(SendSideEffects);
    let promise_val = env.create_promise_executor(Value::Null)?;
    let promise_id = match promise_val {
        Value::Promise(id) => id,
        _ => return Ok(promise_val),
    };

    std::thread::spawn(move || {
        let res = work();
        if let Some(SendSideEffects(se)) = side_effects {
            let mut q = se.lock().unwrap();
            match res {
                Ok(SendValue(val)) => {
                    q.push(crate::parsing::ast::NativeEffect::ResolvePromise(
                        promise_id, val,
                    ));
                }
                Err(err_msg) => {
                    q.push(crate::parsing::ast::NativeEffect::RejectPromise(
                        promise_id,
                        Value::Str(err_msg),
                    ));
                }
            }
        }
    });

    Ok(promise_val)
}

fn extract_msg_text(msg: &WebSocketMessage) -> Result<String, String> {
    match msg {
        WebSocketMessage::Text(t) => Ok(t.clone()),
        WebSocketMessage::Binary(b) => {
            String::from_utf8(b.clone()).map_err(|e| format!("Invalid UTF-8 binary frame: {e}"))
        }
        _ => Err("Received non-data WebSocket frame".to_string()),
    }
}

fn validate_and_serialize_dto(
    env: &mut dyn BuiltinEnv,
    dto: &Value,
    data: &Value,
) -> Result<String, String> {
    if let Value::Object(map) = dto {
        if let Some(val_fn) = map.get("validate") {
            let res = call_value(env, val_fn, vec![data.clone()])?;
            if is_ws_error(&res) {
                let msg = match &res {
                    Value::Object(m) => m
                        .get("message")
                        .and_then(val_as_str)
                        .unwrap_or("Validation failed")
                        .to_string(),
                    _ => "Validation failed".to_string(),
                };
                return Err(format!("ValidationError: {msg}"));
            }
        }
    }
    let json_val = crate::runtime::stdlib_src::http::api::adesh_val_to_serde_json(data)?;
    serde_json::to_string(&json_val).map_err(|e| format!("SerializationError: {e}"))
}

fn parse_and_validate_dto(
    env: &mut dyn BuiltinEnv,
    dto: Option<&Value>,
    text: &str,
) -> Result<Value, String> {
    let parsed: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("InvalidJSON: {e}"))?;
    let val = crate::runtime::stdlib_src::http::api::serde_json_to_adesh_val(&parsed);
    if let Some(dto_val) = dto {
        if let Value::Object(map) = dto_val {
            if let Some(val_fn) = map.get("validate") {
                let res = call_value(env, val_fn, vec![val.clone()])?;
                if is_ws_error(&res) {
                    return Ok(res);
                }
            }
        }
    }
    Ok(val)
}

fn broadcast_to_all(text: &str) -> Result<usize, String> {
    let mut count = 0;
    if let Ok(map) = CONNECTIONS.lock() {
        for (_id, entry_arc) in map.iter() {
            if let Ok(mut entry) = entry_arc.lock() {
                if entry.session.is_open() {
                    if entry.session.send_text(text).is_ok() {
                        count += 1;
                    }
                }
            }
        }
    }
    Ok(count)
}

fn extract_io(socket: &Value) -> Result<WsIoOwned, String> {
    if let Value::Object(map) = socket {
        if let Some(id) = map
            .get("handle")
            .and_then(val_as_u64)
            .or_else(|| map.get("id").and_then(val_as_u64))
        {
            if map
                .get("type")
                .and_then(val_as_str)
                .map(|s| s.eq_ignore_ascii_case("TcpStream"))
                .unwrap_or(false)
            {
                let tcp = crate::runtime::stdlib_src::net::tcp::clone_tcp_stream(id)?;
                return Ok(WsIoOwned::Tcp(tcp));
            }
            if let Ok(tls) = crate::runtime::stdlib_src::tls::api::take_tls_connection(id) {
                return Ok(WsIoOwned::Tls(tls));
            }
            if let Ok(tcp) = crate::runtime::stdlib_src::net::tcp::clone_tcp_stream(id) {
                return Ok(WsIoOwned::Tcp(tcp));
            }
        }
    }
    crate::runtime::stdlib_src::tls::api::extract_tcp_stream(socket).map(WsIoOwned::Tcp)
}

fn url_from_handshake_arg(arg: Option<&Value>) -> Result<String, String> {
    match arg {
        Some(Value::Str(s)) => Ok(s.clone()),
        Some(Value::Object(map)) => {
            if let Some(Value::Str(s)) = map.get("href") {
                return Ok(s.clone());
            }
            let scheme = map.get("scheme").and_then(val_as_str).unwrap_or("ws");
            let host = map
                .get("host")
                .and_then(val_as_str)
                .ok_or_else(|| "URL object missing host".to_string())?;
            let path = map.get("path").and_then(val_as_str).unwrap_or("/");
            let mut uri = format!("{scheme}://{host}");
            if let Some(port) = map.get("port").and_then(val_as_u16) {
                uri.push(':');
                uri.push_str(&port.to_string());
            }
            if !path.starts_with('/') {
                uri.push('/');
            }
            uri.push_str(path);
            Ok(uri)
        }
        _ => Err("performClientHandshake requires a URL object or string".to_string()),
    }
}

fn message_to_value(msg: &WebSocketMessage) -> Value {
    let mut map = FastMap::default();
    match msg {
        WebSocketMessage::Text(text) => {
            map.insert("type".to_string(), Value::Str("text".to_string()));
            map.insert("payload".to_string(), Value::Str(text.clone()));
            map.insert("closeCode".to_string(), Value::Null);
            map.insert("closeReason".to_string(), Value::Null);
        }
        WebSocketMessage::Binary(bytes) => {
            map.insert("type".to_string(), Value::Str("binary".to_string()));
            map.insert("payload".to_string(), bytes_to_value(bytes));
            map.insert("closeCode".to_string(), Value::Null);
            map.insert("closeReason".to_string(), Value::Null);
        }
        WebSocketMessage::Ping(bytes) => {
            map.insert("type".to_string(), Value::Str("ping".to_string()));
            map.insert("payload".to_string(), bytes_to_value(bytes));
            map.insert("closeCode".to_string(), Value::Null);
            map.insert("closeReason".to_string(), Value::Null);
        }
        WebSocketMessage::Pong(bytes) => {
            map.insert("type".to_string(), Value::Str("pong".to_string()));
            map.insert("payload".to_string(), bytes_to_value(bytes));
            map.insert("closeCode".to_string(), Value::Null);
            map.insert("closeReason".to_string(), Value::Null);
        }
        WebSocketMessage::Close { code, reason } => {
            map.insert("type".to_string(), Value::Str("close".to_string()));
            map.insert("payload".to_string(), Value::Null);
            map.insert("closeCode".to_string(), Value::Number(*code as f64));
            map.insert("closeReason".to_string(), Value::Str(reason.clone()));
        }
    }
    Value::Object(Arc::new(map))
}

fn message_from_ctor(args: Vec<Value>) -> Value {
    let ty = args
        .first()
        .and_then(val_as_str)
        .unwrap_or("text")
        .to_string();
    let payload = args.get(1).cloned().unwrap_or(Value::Null);
    let close_code = args.get(2).cloned().unwrap_or(Value::Null);
    let close_reason = args.get(3).cloned().unwrap_or(Value::Null);
    let mut map = FastMap::default();
    map.insert("type".to_string(), Value::Str(ty));
    map.insert("payload".to_string(), payload);
    map.insert("closeCode".to_string(), close_code);
    map.insert("closeReason".to_string(), close_reason);
    Value::Object(Arc::new(map))
}

fn frame_from_ctor(args: Vec<Value>) -> Value {
    let (fin, rsv, opcode, mask, payload) = if args.len() >= 5 {
        (
            args.first().map(val_truthy).unwrap_or(true),
            args.get(1).and_then(val_as_i64).unwrap_or(0) as u8,
            args.get(2).and_then(val_as_u8).unwrap_or(1),
            args.get(3).cloned(),
            args.get(4)
                .map(value_to_bytes)
                .and_then(Result::ok)
                .unwrap_or_default(),
        )
    } else {
        (
            args.first().map(val_truthy).unwrap_or(true),
            0,
            args.get(1).and_then(val_as_u8).unwrap_or(1),
            args.get(2).cloned(),
            args.get(3)
                .map(value_to_bytes)
                .and_then(Result::ok)
                .unwrap_or_default(),
        )
    };
    let opcode = WebSocketOpcode::from_u8(opcode).unwrap_or(WebSocketOpcode::Text);
    let mask_bytes = mask.as_ref().and_then(|v| match v {
        Value::Null => None,
        other => value_to_bytes(other).ok().and_then(|b| {
            if b.len() == 4 {
                Some([b[0], b[1], b[2], b[3]])
            } else {
                None
            }
        }),
    });
    let frame = WebSocketFrame {
        fin,
        rsv1: (rsv & 0x40) != 0,
        rsv2: (rsv & 0x20) != 0,
        rsv3: (rsv & 0x10) != 0,
        opcode,
        mask: mask_bytes,
        payload,
    };
    frame_to_value(&frame)
}

fn frame_to_value(frame: &WebSocketFrame) -> Value {
    let encoded = frame.encode();
    let mut map = FastMap::default();
    map.insert("fin".to_string(), Value::Bool(frame.fin));
    let mut rsv = 0;
    if frame.rsv1 {
        rsv |= 0x40;
    }
    if frame.rsv2 {
        rsv |= 0x20;
    }
    if frame.rsv3 {
        rsv |= 0x10;
    }
    map.insert("rsv".to_string(), Value::Number(rsv as f64));
    map.insert(
        "opcode".to_string(),
        Value::Number(opcode_to_u8(frame.opcode) as f64),
    );
    map.insert(
        "mask".to_string(),
        match frame.mask {
            Some(m) => Value::Array(m.into_iter().map(|b| Value::Number(b as f64)).collect()),
            None => Value::Null,
        },
    );
    map.insert("payload".to_string(), bytes_to_value(&frame.payload));
    let encoded_clone = encoded.clone();
    map.insert(
        "encode".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(bytes_to_value(&encoded_clone))
        }))),
    );
    map.insert(
        "isControl".to_string(),
        Value::Function(NativeFn(Arc::new({
            let opcode = frame.opcode;
            move |_, _| Ok(Value::Bool(opcode.is_control()))
        }))),
    );
    Value::Object(Arc::new(map))
}

fn opcode_object() -> Value {
    let mut map = FastMap::default();
    map.insert("Continuation".to_string(), Value::Number(0.0));
    map.insert("Text".to_string(), Value::Number(1.0));
    map.insert("Binary".to_string(), Value::Number(2.0));
    map.insert("Close".to_string(), Value::Number(8.0));
    map.insert("Ping".to_string(), Value::Number(9.0));
    map.insert("Pong".to_string(), Value::Number(10.0));
    map.insert(
        "isControl".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let opcode = args.first().and_then(val_as_u8).unwrap_or(0);
            Ok(Value::Bool(opcode >= 0x8))
        }))),
    );
    map.insert(
        "isData".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let opcode = args.first().and_then(val_as_u8).unwrap_or(0);
            Ok(Value::Bool(opcode == 0x0 || opcode == 0x1 || opcode == 0x2))
        }))),
    );
    Value::Object(Arc::new(map))
}

fn state_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "CONNECTING".to_string(),
        Value::Number(STATE_CONNECTING as f64),
    );
    map.insert(
        "HANDSHAKING".to_string(),
        Value::Number(STATE_HANDSHAKING as f64),
    );
    map.insert("OPEN".to_string(), Value::Number(STATE_OPEN as f64));
    map.insert("CLOSING".to_string(), Value::Number(STATE_CLOSING as f64));
    map.insert("CLOSED".to_string(), Value::Number(STATE_CLOSED as f64));
    map.insert("FAILED".to_string(), Value::Number(STATE_FAILED as f64));
    Value::Object(Arc::new(map))
}

fn close_code_object() -> Value {
    let mut map = FastMap::default();
    map.insert("NormalClosure".to_string(), Value::Number(1000.0));
    map.insert("GoingAway".to_string(), Value::Number(1001.0));
    map.insert("ProtocolError".to_string(), Value::Number(1002.0));
    map.insert("UnsupportedData".to_string(), Value::Number(1003.0));
    map.insert("InvalidPayloadData".to_string(), Value::Number(1007.0));
    map.insert("PolicyViolation".to_string(), Value::Number(1008.0));
    map.insert("MessageTooBig".to_string(), Value::Number(1009.0));
    map.insert("MandatoryExtension".to_string(), Value::Number(1010.0));
    map.insert("InternalError".to_string(), Value::Number(1011.0));
    map.insert("ServiceRestart".to_string(), Value::Number(1012.0));
    map.insert("TryAgainLater".to_string(), Value::Number(1013.0));
    map.insert("BadGateway".to_string(), Value::Number(1014.0));
    Value::Object(Arc::new(map))
}

fn client_config_object() -> Value {
    let mut map = FastMap::default();
    map.insert("uri".to_string(), Value::Null);
    map.insert(
        "headers".to_string(),
        Value::Object(Arc::new(FastMap::default())),
    );
    map.insert("protocols".to_string(), Value::Array(Vec::new()));
    map.insert("extensions".to_string(), Value::Array(Vec::new()));
    map.insert("origin".to_string(), Value::Null);
    map.insert("proxy_url".to_string(), Value::Null);
    map.insert("connect_timeout".to_string(), Value::Number(10_000.0));
    map.insert("handshake_timeout".to_string(), Value::Number(10_000.0));
    map.insert("idle_timeout".to_string(), Value::Number(60_000.0));
    map.insert("max_message_size".to_string(), Value::Number(16_777_216.0));
    map.insert("max_frame_size".to_string(), Value::Number(4_194_304.0));
    map.insert(
        "max_send_queue_messages".to_string(),
        Value::Number(1_000.0),
    );
    map.insert(
        "max_send_queue_bytes".to_string(),
        Value::Number(16_777_216.0),
    );
    map.insert("automatic_pong".to_string(), Value::Bool(true));
    map.insert("compression".to_string(), Value::Bool(false));
    map.insert("verify_tls".to_string(), Value::Bool(true));
    map.insert("tls_ca_pem".to_string(), Value::Null);
    Value::Object(Arc::new(map))
}

fn server_config_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "bind_address".to_string(),
        Value::Str("127.0.0.1".to_string()),
    );
    map.insert("port".to_string(), Value::Number(8099.0));
    map.insert("max_connections".to_string(), Value::Number(10_000.0));
    map.insert("max_connections_per_ip".to_string(), Value::Number(100.0));
    map.insert("max_message_size".to_string(), Value::Number(16_777_216.0));
    map.insert("max_frame_size".to_string(), Value::Number(4_194_304.0));
    map.insert(
        "max_send_queue_messages".to_string(),
        Value::Number(1_000.0),
    );
    map.insert(
        "max_send_queue_bytes".to_string(),
        Value::Number(16_777_216.0),
    );
    map.insert("handshake_timeout".to_string(), Value::Number(5_000.0));
    map.insert("idle_timeout".to_string(), Value::Number(60_000.0));
    map.insert("ping_interval".to_string(), Value::Number(30_000.0));
    map.insert("pong_timeout".to_string(), Value::Number(10_000.0));
    map.insert("protocols".to_string(), Value::Array(Vec::new()));
    map.insert("origins".to_string(), Value::Array(Vec::new()));
    map.insert("compression".to_string(), Value::Bool(false));
    map.insert("tls_cert_pem".to_string(), Value::Null);
    map.insert("tls_key_pem".to_string(), Value::Null);
    map.insert("automatic_pong".to_string(), Value::Bool(true));
    Value::Object(Arc::new(map))
}

fn parse_client_options(val: Option<&Value>) -> WsClientOptions {
    let mut options = WsClientOptions::default();
    let Some(val) = val else {
        return options;
    };
    if let Some(map) = value_fields(val) {
        if let Some(v) = map_get(&map, "connect_timeout").and_then(val_as_i64) {
            options.connect_timeout = Duration::from_millis(v.max(0) as u64);
        }
        if let Some(v) = map_get(&map, "handshake_timeout").and_then(val_as_i64) {
            options.handshake_timeout = Duration::from_millis(v.max(0) as u64);
        }
        if let Some(v) = map_get(&map, "idle_timeout").and_then(val_as_i64) {
            options.idle_timeout = if v <= 0 {
                None
            } else {
                Some(Duration::from_millis(v as u64))
            };
        }
        if let Some(v) = map_get(&map, "max_message_size").and_then(val_as_i64) {
            options.max_message_size = v.max(1) as usize;
        }
        if let Some(v) = map_get(&map, "max_frame_size").and_then(val_as_i64) {
            options.max_frame_size = v.max(1) as usize;
        }
        if let Some(v) = map_get(&map, "automatic_pong").and_then(val_as_bool) {
            options.automatic_pong = v;
        }
        if let Some(v) = map_get(&map, "compression").and_then(val_as_bool) {
            options.compression = v;
        }
        if let Some(v) = map_get(&map, "verify_tls").and_then(val_as_bool) {
            options.verify_tls = v;
        }
        if let Some(v) = map_get(&map, "origin").and_then(val_as_str) {
            options.origin = Some(v.to_string());
        }
        if let Some(v) = map_get(&map, "proxy_url").and_then(val_as_str) {
            options.proxy_url = Some(v.to_string());
        }
        if let Some(v) = map_get(&map, "tls_ca_pem")
            .or_else(|| map_get(&map, "ca_pem"))
            .or_else(|| map_get(&map, "ca"))
            .and_then(val_as_str)
        {
            options.tls_ca_pem = Some(v.as_bytes().to_vec());
        }
        if let Some(protocols) = map_get(&map, "protocols").and_then(val_as_string_list) {
            options.protocols = protocols;
        }
        if let Some(headers) = map_get(&map, "headers") {
            if let Some(hmap) = value_fields(headers) {
                for (k, v) in hmap {
                    if let Some(s) = val_as_str(&v) {
                        options.headers.insert(k, s.to_string());
                    }
                }
            }
        }
    }
    options
}

fn parse_server_options(val: Option<&Value>) -> WsServerOptions {
    let mut options = WsServerOptions::default();
    let Some(val) = val else {
        return options;
    };
    if let Some(map) = value_fields(val) {
        if let Some(v) = map_get(&map, "bind_address").and_then(val_as_str) {
            options.bind_address = v.to_string();
        }
        if let Some(v) = map_get(&map, "port").and_then(val_as_u16) {
            options.port = v;
        }
        if let Some(v) = map_get(&map, "max_message_size").and_then(val_as_i64) {
            options.max_message_size = v.max(1) as usize;
        }
        if let Some(v) = map_get(&map, "max_frame_size").and_then(val_as_i64) {
            options.max_frame_size = v.max(1) as usize;
        }
        if let Some(v) = map_get(&map, "handshake_timeout").and_then(val_as_i64) {
            options.handshake_timeout = Duration::from_millis(v.max(0) as u64);
        }
        if let Some(v) = map_get(&map, "idle_timeout").and_then(val_as_i64) {
            options.idle_timeout = if v <= 0 {
                None
            } else {
                Some(Duration::from_millis(v as u64))
            };
        }
        if let Some(v) = map_get(&map, "automatic_pong").and_then(val_as_bool) {
            options.automatic_pong = v;
        }
        if let Some(v) = map_get(&map, "compression").and_then(val_as_bool) {
            options.compression = v;
        }
        if let Some(protocols) = map_get(&map, "protocols").and_then(val_as_string_list) {
            options.protocols = protocols;
        }
        if let Some(origins) = map_get(&map, "origins").and_then(val_as_string_list) {
            options.origins = origins;
        }
        if let Some(v) = map_get(&map, "tls_cert_pem")
            .or_else(|| map_get(&map, "tls_cert"))
            .or_else(|| map_get(&map, "cert_pem"))
            .or_else(|| map_get(&map, "cert"))
            .and_then(val_as_str)
        {
            options.tls_cert_pem = Some(v.as_bytes().to_vec());
        }
        if let Some(v) = map_get(&map, "tls_key_pem")
            .or_else(|| map_get(&map, "tls_key"))
            .or_else(|| map_get(&map, "key_pem"))
            .or_else(|| map_get(&map, "key"))
            .and_then(val_as_str)
        {
            options.tls_key_pem = Some(v.as_bytes().to_vec());
        }
    }
    options
}

fn value_fields(val: &Value) -> Option<FastMap<String, Value>> {
    match val {
        Value::Object(map) => Some((**map).clone()),
        Value::Instance(inst) => {
            let mut out = FastMap::default();
            if let Ok(fields) = inst.fields.read() {
                for (k, v) in fields.iter() {
                    out.insert(k.clone(), v.clone());
                }
            }
            Some(out)
        }
        _ => None,
    }
}

fn map_get<'a>(map: &'a FastMap<String, Value>, key: &str) -> Option<&'a Value> {
    map.get(key)
}

fn error_object(kind: &str, message: &str, cause: Value) -> Value {
    let mut map = FastMap::default();
    map.insert("kind".to_string(), Value::Str(kind.to_string()));
    map.insert("message".to_string(), Value::Str(message.to_string()));
    map.insert("cause".to_string(), cause);
    map.insert("__wsError".to_string(), Value::Bool(true));
    map.insert(
        "__type".to_string(),
        Value::Str("WebSocketError".to_string()),
    );
    let display = format!("WebSocketError[{kind}]: {message}");
    map.insert(
        "toString".to_string(),
        Value::Function(NativeFn(Arc::new({
            let display = display.clone();
            move |_, _| Ok(Value::Str(display.clone()))
        }))),
    );
    Value::Object(Arc::new(map))
}

fn http_to_ws_error(err: &HttpError) -> Value {
    let kind = if err.message.contains("non-open") {
        "Closed"
    } else if err.message.contains("Sec-WebSocket-Accept") {
        "InvalidAccept"
    } else if err.message.contains("exceeds") || err.kind == HttpErrorKind::BodyTooLarge {
        "MessageTooBig"
    } else {
        match err.kind {
            HttpErrorKind::InvalidUri => "InvalidUrl",
            HttpErrorKind::ConnectError | HttpErrorKind::DnsError => "ConnectionRefused",
            HttpErrorKind::TlsError => "TlsFailure",
            HttpErrorKind::ProtocolError | HttpErrorKind::InvalidHeader => {
                if err.message.contains("101") || err.message.contains("Upgrade") {
                    "HandshakeFailure"
                } else {
                    "ProtocolError"
                }
            }
            _ => "HandshakeFailure",
        }
    };
    error_object(kind, &err.message, Value::Null)
}

fn is_ws_error(val: &Value) -> bool {
    match val {
        Value::Object(map) => {
            map.get("__wsError").and_then(val_as_bool).unwrap_or(false)
                || map
                    .get("__type")
                    .and_then(val_as_str)
                    .map(|s| s == "WebSocketError")
                    .unwrap_or(false)
        }
        Value::Instance(inst) => inst.class_name.contains("WebSocketError"),
        _ => false,
    }
}

fn opcode_to_u8(opcode: WebSocketOpcode) -> u8 {
    match opcode {
        WebSocketOpcode::Continuation => 0x0,
        WebSocketOpcode::Text => 0x1,
        WebSocketOpcode::Binary => 0x2,
        WebSocketOpcode::Close => 0x8,
        WebSocketOpcode::Ping => 0x9,
        WebSocketOpcode::Pong => 0xA,
    }
}

fn bytes_to_value(bytes: &[u8]) -> Value {
    Value::Array(bytes.iter().map(|b| Value::Number(*b as f64)).collect())
}

fn value_to_bytes(val: &Value) -> Result<Vec<u8>, String> {
    match val {
        Value::Null => Ok(Vec::new()),
        Value::Str(s) => Ok(s.as_bytes().to_vec()),
        Value::Array(arr) | Value::RawArray(_, arr) => {
            Ok(arr.iter().filter_map(val_as_u8).collect())
        }
        Value::DynArray(da) => Ok(da.data.iter().filter_map(val_as_u8).collect()),
        _ => Err("Expected byte array or string".to_string()),
    }
}

fn val_as_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) => Some(s.as_str()),
        Value::Ref(inner, _) => val_as_str(inner),
        Value::Share(sr) => unsafe { val_as_str(&(*sr.ptr).value) },
        _ => None,
    }
}

fn val_as_string_list(v: &Value) -> Option<Vec<String>> {
    match v {
        Value::Array(arr) | Value::RawArray(_, arr) => Some(
            arr.iter()
                .filter_map(val_as_str)
                .map(String::from)
                .collect(),
        ),
        Value::DynArray(da) => Some(
            da.data
                .iter()
                .filter_map(val_as_str)
                .map(String::from)
                .collect(),
        ),
        Value::Ref(inner, _) => val_as_string_list(inner),
        Value::Share(sr) => unsafe { val_as_string_list(&(*sr.ptr).value) },
        _ => None,
    }
}

fn val_as_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Ref(inner, _) => val_as_bool(inner),
        Value::Share(sr) => unsafe { val_as_bool(&(*sr.ptr).value) },
        _ => None,
    }
}

fn val_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Ref(inner, _) => val_truthy(inner),
        Value::Share(sr) => unsafe { val_truthy(&(*sr.ptr).value) },
        _ => true,
    }
}

fn val_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => Some(*n as i64),
        Value::F64(n) => Some(*n as i64),
        Value::F32(n) => Some(*n as i64),
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::I16(n) => Some(*n as i64),
        Value::I8(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U16(n) => Some(*n as i64),
        Value::U8(n) => Some(*n as i64),
        Value::Ref(inner, _) => val_as_i64(inner),
        Value::Share(sr) => unsafe { val_as_i64(&(*sr.ptr).value) },
        _ => None,
    }
}

fn val_as_u16(v: &Value) -> Option<u16> {
    val_as_i64(v).map(|n| n as u16)
}

fn val_as_u8(v: &Value) -> Option<u8> {
    match v {
        Value::Number(n) => Some(*n as u8),
        Value::U8(n) => Some(*n),
        Value::I64(n) => Some(*n as u8),
        Value::I32(n) => Some(*n as u8),
        Value::U32(n) => Some(*n as u8),
        Value::Ref(inner, _) => val_as_u8(inner),
        Value::Share(sr) => unsafe { val_as_u8(&(*sr.ptr).value) },
        _ => None,
    }
}

fn val_as_u64(v: &Value) -> Option<u64> {
    match v {
        Value::U64(n) => Some(*n),
        Value::I64(n) if *n >= 0 => Some(*n as u64),
        Value::Number(n) if *n >= 0.0 => Some(*n as u64),
        Value::Ref(inner, _) => val_as_u64(inner),
        Value::Share(sr) => unsafe { val_as_u64(&(*sr.ptr).value) },
        _ => None,
    }
}
