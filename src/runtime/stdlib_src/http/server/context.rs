use super::super::method::HttpMethod;
use super::super::request::Request;
use super::super::response::Response;
use crate::parsing::ast::{NativeFn, Value};
use crate::utils::collections::FastMap;
use std::net::SocketAddr;
use std::sync::Arc;

pub struct RequestContext {
    pub request: Request,
    pub params: FastMap<String, String>,
    pub request_id: String,
    pub peer_addr: Option<SocketAddr>,
    pub local_addr: Option<SocketAddr>,
    pub locals: Arc<std::sync::Mutex<FastMap<String, Value>>>,
}

impl RequestContext {
    pub fn new(request: Request, params: FastMap<String, String>) -> Self {
        let request_id = format!("req_{:x}", rand::random::<u64>());
        Self {
            request,
            params,
            request_id,
            peer_addr: None,
            local_addr: None,
            locals: Arc::new(std::sync::Mutex::new(FastMap::default())),
        }
    }

    pub fn from_request(request: Request, peer_addr: Option<SocketAddr>) -> Self {
        let request_id = format!("req_{:x}", rand::random::<u64>());
        Self {
            request,
            params: FastMap::default(),
            request_id,
            peer_addr,
            local_addr: None,
            locals: Arc::new(std::sync::Mutex::new(FastMap::default())),
        }
    }

    pub fn method(&self) -> &HttpMethod {
        &self.request.method
    }

    pub fn param(&self, name: &str) -> Option<&str> {
        self.params.get(name).map(|s| s.as_str())
    }

    pub fn query(&self, name: &str) -> Option<String> {
        self.request.uri.query_params().get(name).cloned()
    }

    pub fn build_req_res_tuple(&self) -> (Value, Value, Arc<std::sync::Mutex<Option<Response>>>) {
        // Build `req` object
        let mut req_map = FastMap::default();
        req_map.insert(
            "method".to_string(),
            Value::Str(self.request.method.as_str().to_string()),
        );
        req_map.insert(
            "version".to_string(),
            Value::Str(self.request.version.as_str().to_string()),
        );
        req_map.insert("url".to_string(), Value::Str(self.request.uri.to_string()));
        req_map.insert("uri".to_string(), Value::Str(self.request.uri.to_string()));
        req_map.insert(
            "scheme".to_string(),
            Value::Str(if self.request.uri.is_https() {
                "https".to_string()
            } else {
                "http".to_string()
            }),
        );
        req_map.insert(
            "host".to_string(),
            Value::Str(
                self.request
                    .uri
                    .host
                    .clone()
                    .unwrap_or_else(|| "127.0.0.1".to_string()),
            ),
        );
        req_map.insert(
            "port".to_string(),
            Value::I64(self.request.uri.effective_port() as i64),
        );
        req_map.insert(
            "path".to_string(),
            Value::Str(self.request.uri.path.clone()),
        );
        req_map.insert(
            "rawPath".to_string(),
            Value::Str(self.request.uri.path.clone()),
        );
        req_map.insert(
            "queryString".to_string(),
            Value::Str(self.request.uri.query.clone().unwrap_or_default()),
        );
        req_map.insert("requestId".to_string(), Value::Str(self.request_id.clone()));
        req_map.insert(
            "isSecure".to_string(),
            Value::Bool(self.request.uri.is_https()),
        );
        req_map.insert(
            "protocol".to_string(),
            Value::Str(self.request.version.as_str().to_string()),
        );

        let peer_ip = self
            .peer_addr
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let peer_port = self.peer_addr.map(|a| a.port() as i64).unwrap_or(0);
        let loc_ip = self
            .local_addr
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|| "127.0.0.1".to_string());
        let loc_port = self.local_addr.map(|a| a.port() as i64).unwrap_or(80);

        req_map.insert("remoteAddress".to_string(), Value::Str(peer_ip));
        req_map.insert("remotePort".to_string(), Value::I64(peer_port));
        req_map.insert("localAddress".to_string(), Value::Str(loc_ip));
        req_map.insert("localPort".to_string(), Value::I64(loc_port));

        // Body features & streaming
        let body_bytes = self.request.body.to_bytes().unwrap_or_default();
        let body_text = String::from_utf8_lossy(&body_bytes).to_string();
        let content_len = body_bytes.len() as i64;
        let content_type = self
            .request
            .headers
            .get("content-type")
            .unwrap_or("")
            .to_string();

        req_map.insert("body".to_string(), Value::Str(body_text.clone()));
        req_map.insert("contentLength".to_string(), Value::I64(content_len));
        req_map.insert("contentType".to_string(), Value::Str(content_type.clone()));

        let has_body_bytes = body_bytes.clone();
        req_map.insert(
            "hasBody".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                Ok(Value::Bool(!has_body_bytes.is_empty()))
            }))),
        );

        let text_val = body_text.clone();
        req_map.insert(
            "text".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                Ok(Value::Str(text_val.clone()))
            }))),
        );

        let bytes_val = body_bytes.clone();
        req_map.insert(
            "bytes".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                let arr = bytes_val.iter().map(|&b| Value::Number(b as f64)).collect();
                Ok(Value::Array(arr))
            }))),
        );

        let json_text = body_text.clone();
        req_map.insert(
            "json".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                if json_text.trim().is_empty() {
                    return Ok(Value::Null);
                }
                let parsed: serde_json::Value = serde_json::from_str(&json_text)
                    .map_err(|e| format!("Failed to parse request JSON body: {}", e))?;
                Ok(crate::runtime::stdlib_src::http::api::serde_json_to_adesh_val(&parsed))
            }))),
        );

        let try_json_text = body_text.clone();
        req_map.insert(
            "tryJson".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                if try_json_text.trim().is_empty() {
                    return Ok(Value::Null);
                }
                match serde_json::from_str::<serde_json::Value>(&try_json_text) {
                    Ok(parsed) => {
                        Ok(crate::runtime::stdlib_src::http::api::serde_json_to_adesh_val(&parsed))
                    }
                    Err(_) => Ok(Value::Null),
                }
            }))),
        );

        let json_or_text = body_text.clone();
        req_map.insert(
            "jsonOr".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let default_val = args.first().cloned().unwrap_or(Value::Null);
                if json_or_text.trim().is_empty() {
                    return Ok(default_val);
                }
                match serde_json::from_str::<serde_json::Value>(&json_or_text) {
                    Ok(parsed) => {
                        Ok(crate::runtime::stdlib_src::http::api::serde_json_to_adesh_val(&parsed))
                    }
                    Err(_) => Ok(default_val),
                }
            }))),
        );

        let json_err_text = body_text;
        req_map.insert(
            "jsonOrError".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                match serde_json::from_str::<serde_json::Value>(&json_err_text) {
                    Ok(parsed) => {
                        Ok(crate::runtime::stdlib_src::http::api::serde_json_to_adesh_val(&parsed))
                    }
                    Err(e) => {
                        let mut err_map = FastMap::default();
                        err_map.insert("message".to_string(), Value::Str(e.to_string()));
                        err_map.insert("line".to_string(), Value::I64(e.line() as i64));
                        err_map.insert("column".to_string(), Value::I64(e.column() as i64));
                        err_map.insert("offset".to_string(), Value::I64(0));
                        Ok(Value::Object(Arc::new(err_map)))
                    }
                }
            }))),
        );

        // Streaming Body API: req.bodyStream()
        let stream_bytes = body_bytes;
        req_map.insert(
            "bodyStream".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                let mut st_map = FastMap::default();
                let c1 = stream_bytes.clone();
                let c2 = stream_bytes.clone();
                let c3 = stream_bytes.clone();

                st_map.insert(
                    "read".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        let arr = c1.iter().map(|&b| Value::Number(b as f64)).collect();
                        Ok(Value::Array(arr))
                    }))),
                );
                st_map.insert(
                    "readChunk".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        let arr = c2.iter().map(|&b| Value::Number(b as f64)).collect();
                        Ok(Value::Array(arr))
                    }))),
                );
                st_map.insert(
                    "readAll".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Str(String::from_utf8_lossy(&c3).to_string()))
                    }))),
                );
                st_map.insert(
                    "cancel".to_string(),
                    Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
                );
                Ok(Value::Object(Arc::new(st_map)))
            }))),
        );

        // Headers & Cookies
        let mut headers_map = FastMap::default();
        for (k, v) in self.request.headers.to_map() {
            headers_map.insert(k.to_lowercase(), Value::Str(v));
        }
        let hdrs_obj = Arc::new(headers_map);
        req_map.insert("headers".to_string(), Value::Object(hdrs_obj.clone()));

        let hdrs_get = hdrs_obj;
        req_map.insert(
            "header".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let key = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.to_lowercase()),
                        _ => None,
                    })
                    .unwrap_or_default();
                Ok(hdrs_get.get(&key).cloned().unwrap_or(Value::Null))
            }))),
        );

        // Cookie Extraction
        let raw_cookie = self.request.headers.get("cookie").unwrap_or("").to_string();
        let mut cookies_map = FastMap::default();
        for pair in raw_cookie.split(';') {
            let mut parts = pair.splitn(2, '=');
            if let (Some(k), Some(v)) = (parts.next(), parts.next()) {
                cookies_map.insert(k.trim().to_string(), Value::Str(v.trim().to_string()));
            }
        }
        let cookies_obj = Arc::new(cookies_map);
        req_map.insert("cookies".to_string(), Value::Object(cookies_obj.clone()));

        let cookies_get = cookies_obj;
        req_map.insert(
            "cookie".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let key = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                Ok(cookies_get.get(key).cloned().unwrap_or(Value::Null))
            }))),
        );

        // Params & Typed Extraction
        let params_map = self.params.clone();
        let mut p_obj = FastMap::default();
        for (k, v) in &params_map {
            p_obj.insert(k.clone(), Value::Str(v.clone()));
        }
        req_map.insert("params".to_string(), Value::Object(Arc::new(p_obj)));

        let p_get = params_map.clone();
        req_map.insert(
            "param".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let key = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                Ok(p_get
                    .get(key)
                    .map(|s| Value::Str(s.clone()))
                    .unwrap_or(Value::Null))
            }))),
        );

        let p_int = params_map.clone();
        req_map.insert(
            "paramInt".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let key = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                if let Some(val_str) = p_int.get(key) {
                    if let Ok(num) = val_str.parse::<i64>() {
                        return Ok(Value::I64(num));
                    }
                }
                Ok(Value::Null)
            }))),
        );

        let p_uuid = params_map;
        req_map.insert(
            "paramUuid".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let key = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                if let Some(val_str) = p_uuid.get(key) {
                    if val_str.len() == 36 && val_str.contains('-') {
                        return Ok(Value::Str(val_str.clone()));
                    }
                }
                Ok(Value::Null)
            }))),
        );

        // Query params
        let query_params_map = self.request.uri.query_params();
        let mut q_obj = FastMap::default();
        for (k, v) in &query_params_map {
            q_obj.insert(k.clone(), Value::Str(v.clone()));
        }
        let q_obj_arc = Arc::new(q_obj);
        req_map.insert("queryParams".to_string(), Value::Object(q_obj_arc.clone()));

        let q_map = query_params_map;
        let q_obj_fallback = q_obj_arc;
        req_map.insert(
            "query".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                if args.is_empty() {
                    return Ok(Value::Object(q_obj_fallback.clone()));
                }
                let key = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                Ok(q_map
                    .get(key)
                    .map(|s| Value::Str(s.clone()))
                    .unwrap_or(Value::Null))
            }))),
        );

        // Tenant extraction
        let host_str = self.request.uri.host.clone().unwrap_or_default();
        let tenant_id = if host_str.contains('.') {
            host_str.split('.').next().unwrap_or("").to_string()
        } else {
            "default".to_string()
        };
        req_map.insert("tenant".to_string(), Value::Str(tenant_id));

        // Content negotiation
        let accept_hdr = self
            .request
            .headers
            .get("accept")
            .unwrap_or("*/*")
            .to_string();
        req_map.insert(
            "accepts".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let target = args
                    .first()
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("");
                Ok(Value::Bool(
                    accept_hdr.contains(target) || accept_hdr.contains("*/*"),
                ))
            }))),
        );

        let req_val = Value::Object(Arc::new(req_map));

        // Build `res` response builder object
        let res_map = FastMap::default();
        let status_code = Arc::new(std::sync::atomic::AtomicU16::new(200));
        let custom_headers = Arc::new(std::sync::Mutex::new(FastMap::default()));
        let response_slot: Arc<std::sync::Mutex<Option<Response>>> =
            Arc::new(std::sync::Mutex::new(None));
        let res_map_arc = Arc::new(std::sync::Mutex::new(res_map));

        // Chainable res.status(code)
        let status_clone = status_code.clone();
        let res_self_status = res_map_arc.clone();
        res_map_arc.lock().unwrap().insert(
            "status".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                if let Some(val) = args.first() {
                    let code = match val {
                        Value::I64(n) => *n as u16,
                        Value::Number(n) => *n as u16,
                        _ => 200,
                    };
                    status_clone.store(code, std::sync::atomic::Ordering::SeqCst);
                }
                if let Ok(map) = res_self_status.lock() {
                    Ok(Value::Object(Arc::new(map.clone())))
                } else {
                    Ok(Value::Bool(true))
                }
            }))),
        );

        // Header builder: res.setHeader(k, v) / res.header(k, v)
        let headers_clone = custom_headers.clone();
        let res_self_header = res_map_arc.clone();
        res_map_arc.lock().unwrap().insert(
            "setHeader".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                if let (Some(Value::Str(k)), Some(Value::Str(v))) = (args.get(0), args.get(1)) {
                    if let Ok(mut map) = headers_clone.lock() {
                        map.insert(k.clone(), v.clone());
                    }
                }
                if let Ok(map) = res_self_header.lock() {
                    Ok(Value::Object(Arc::new(map.clone())))
                } else {
                    Ok(Value::Bool(true))
                }
            }))),
        );

        let headers_h_clone = custom_headers.clone();
        let res_self_h = res_map_arc.clone();
        res_map_arc.lock().unwrap().insert(
            "header".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                if let (Some(Value::Str(k)), Some(Value::Str(v))) = (args.get(0), args.get(1)) {
                    if let Ok(mut map) = headers_h_clone.lock() {
                        map.insert(k.clone(), v.clone());
                    }
                }
                if let Ok(map) = res_self_h.lock() {
                    Ok(Value::Object(Arc::new(map.clone())))
                } else {
                    Ok(Value::Bool(true))
                }
            }))),
        );

        // Cookie builder: res.cookie(name, val)
        let headers_cookie = custom_headers.clone();
        let res_self_cookie = res_map_arc.clone();
        res_map_arc.lock().unwrap().insert(
            "cookie".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                if let (Some(Value::Str(k)), Some(Value::Str(v))) = (args.get(0), args.get(1)) {
                    let cookie_str = format!("{}={}; Path=/; HttpOnly; SameSite=Lax", k, v);
                    if let Ok(mut map) = headers_cookie.lock() {
                        map.insert("Set-Cookie".to_string(), cookie_str);
                    }
                }
                if let Ok(map) = res_self_cookie.lock() {
                    Ok(Value::Object(Arc::new(map.clone())))
                } else {
                    Ok(Value::Bool(true))
                }
            }))),
        );

        // res.send(body)
        let status_send = status_code.clone();
        let headers_send = custom_headers.clone();
        let resp_slot_send = response_slot.clone();
        res_map_arc.lock().unwrap().insert(
            "send".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let body_str = match args.first() {
                    Some(Value::Str(s)) => s.clone(),
                    Some(v) => {
                        match crate::runtime::stdlib_src::http::api::adesh_val_to_serde_json(v) {
                            Ok(json_val) => json_val.to_string(),
                            Err(_) => format!("{:?}", v),
                        }
                    }
                    None => String::new(),
                };
                let code = status_send.load(std::sync::atomic::Ordering::SeqCst);
                let mut resp = Response::new(super::super::status::HttpStatus(code));
                resp.body = super::super::body::Body::from_string(&body_str);
                if let Ok(map) = headers_send.lock() {
                    for (k, v) in map.iter() {
                        let _ = resp.headers.insert(k, v);
                    }
                }
                let _ = resp.headers.set_content_length(body_str.len() as u64);
                let resp_val = super::super::api::build_response_val(resp.clone());
                if let Ok(mut lock) = resp_slot_send.lock() {
                    *lock = Some(resp);
                }
                Ok(resp_val)
            }))),
        );

        // res.json(val)
        let status_json = status_code.clone();
        let headers_json = custom_headers.clone();
        let resp_slot_json = response_slot.clone();
        res_map_arc.lock().unwrap().insert(
            "json".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let json_str = match args.first() {
                    Some(Value::Str(s)) => s.clone(),
                    Some(v) => {
                        match crate::runtime::stdlib_src::http::api::adesh_val_to_serde_json(v) {
                            Ok(json_val) => json_val.to_string(),
                            Err(_) => format!("{:?}", v),
                        }
                    }
                    None => "{}".to_string(),
                };
                let code = status_json.load(std::sync::atomic::Ordering::SeqCst);
                let mut resp = Response::new(super::super::status::HttpStatus(code));
                resp.body = super::super::body::Body::from_string(&json_str);
                let _ = resp.headers.insert("content-type", "application/json");
                if let Ok(map) = headers_json.lock() {
                    for (k, v) in map.iter() {
                        let _ = resp.headers.insert(k, v);
                    }
                }
                let _ = resp.headers.set_content_length(json_str.len() as u64);
                let resp_val = super::super::api::build_response_val(resp.clone());
                if let Ok(mut lock) = resp_slot_json.lock() {
                    *lock = Some(resp);
                }
                Ok(resp_val)
            }))),
        );

        // res.empty() -> 204 No Content
        let resp_slot_empty = response_slot.clone();
        res_map_arc.lock().unwrap().insert(
            "empty".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _| {
                let mut resp = Response::new(super::super::status::HttpStatus(204));
                resp.body = super::super::body::Body::Empty;
                let resp_val = super::super::api::build_response_val(resp.clone());
                if let Ok(mut lock) = resp_slot_empty.lock() {
                    *lock = Some(resp);
                }
                Ok(resp_val)
            }))),
        );

        // res.problem(status, title, detail) -> 404 / 500 application/problem+json
        let resp_slot_prob = response_slot.clone();
        res_map_arc.lock().unwrap().insert(
            "problem".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let code = args
                    .get(0)
                    .and_then(|v| match v {
                        Value::I64(n) => Some(*n as u16),
                        Value::Number(n) => Some(*n as u16),
                        _ => None,
                    })
                    .unwrap_or(500);
                let title = args
                    .get(1)
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("Error");
                let detail = args
                    .get(2)
                    .and_then(|v| match v {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    })
                    .unwrap_or("An error occurred");

                let json_payload = format!(
                    "{{\"type\":\"about:blank\",\"title\":\"{}\",\"status\":{},\"detail\":\"{}\"}}",
                    title, code, detail
                );
                let mut resp = Response::new(super::super::status::HttpStatus(code));
                resp.body = super::super::body::Body::from_string(&json_payload);
                let _ = resp
                    .headers
                    .insert("content-type", "application/problem+json");
                let _ = resp.headers.set_content_length(json_payload.len() as u64);

                let resp_val = super::super::api::build_response_val(resp.clone());
                if let Ok(mut lock) = resp_slot_prob.lock() {
                    *lock = Some(resp);
                }
                Ok(resp_val)
            }))),
        );

        // res.sse(stream_fn) -> text/event-stream
        let resp_slot_sse = response_slot.clone();
        res_map_arc.lock().unwrap().insert(
            "sse".to_string(),
            Value::Function(NativeFn(Arc::new(move |_, _args| {
                let mut resp = Response::new(super::super::status::HttpStatus(200));
                let _ = resp.headers.insert("content-type", "text/event-stream");
                let _ = resp.headers.insert("cache-control", "no-cache");
                let _ = resp.headers.insert("connection", "keep-alive");
                resp.body =
                    super::super::body::Body::from_string("event: message\ndata: Connected\n\n");
                let resp_val = super::super::api::build_response_val(resp.clone());
                if let Ok(mut lock) = resp_slot_sse.lock() {
                    *lock = Some(resp);
                }
                Ok(resp_val)
            }))),
        );

        let final_map = res_map_arc.lock().unwrap().clone();
        let res_val = Value::Object(Arc::new(final_map));

        (req_val, res_val, response_slot)
    }
}
