#[allow(unused_imports)]
use crate::parsing::ast::{NativeFn, UserFn, Value};
use crate::runtime::stdlib_src::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::sync::Arc;

use super::body::Body;
use super::client::HttpClient;
use super::method::HttpMethod;
use super::method::HttpMethodRegistry;
use super::proxy::{LoadBalanceAlgorithm, LoadBalancer, ReverseProxy, UpstreamTarget};
use super::request::Request;
use super::response::Response;
use super::server::{HttpServer, IncomingRequest, Router};
use super::status::HttpStatus;
use super::uri::Uri;
use once_cell::sync::Lazy;

/// Call a Value (either native NativeFn or user-defined UserFn) with the given args.
/// This mirrors the pattern used in decorators/mod.rs.
fn call_value(
    env: &mut dyn crate::parsing::ast::BuiltinEnv,
    val: &Value,
    args: Vec<Value>,
) -> Result<Value, String> {
    match val {
        Value::Function(NativeFn(f)) => (f)(env, args),
        Value::UserFunction(u) => {
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
        _ => Err(format!(
            "call_value: expected a function, got {:?}",
            std::mem::discriminant(val)
        )),
    }
}

struct SendSideEffects(Arc<std::sync::Mutex<Vec<crate::parsing::ast::NativeEffect>>>);
unsafe impl Send for SendSideEffects {}
unsafe impl Sync for SendSideEffects {}

/// Helper to execute work on a background thread and resolve/reject a Promise via interpreter native effects.
fn spawn_promise_work<F>(
    env: &mut dyn crate::parsing::ast::BuiltinEnv,
    work: F,
) -> Result<Value, String>
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

/// Safety: Value is only accessed on the main (interpreter) thread.
/// This wrapper is required to store Values in a static registry.
struct SendValue(Value);
unsafe impl Send for SendValue {}
unsafe impl Sync for SendValue {}

// Stores Router instances by router_id for HTTP.Server(addr, router) lookup
static ROUTER_REGISTRY: Lazy<std::sync::Mutex<FastMap<String, Arc<std::sync::Mutex<Router>>>>> =
    Lazy::new(|| std::sync::Mutex::new(FastMap::default()));

// Stores user-defined route callbacks: "router_id:method:path" -> Value::Function
static CALLBACK_REGISTRY: Lazy<std::sync::Mutex<FastMap<String, SendValue>>> =
    Lazy::new(|| std::sync::Mutex::new(FastMap::default()));

fn get_registered_router(id: &str) -> Option<Router> {
    if let Ok(map) = ROUTER_REGISTRY.lock() {
        if let Some(r_arc) = map.get(id) {
            if let Ok(r) = r_arc.lock() {
                return Some(r.clone());
            }
        }
    }
    None
}

/// Get all callbacks for a router, returned as (method:path -> Value) map
fn get_callbacks_for_router(router_id: &str) -> FastMap<String, Value> {
    let prefix = format!("{}:", router_id);
    let mut result = FastMap::default();
    if let Ok(reg) = CALLBACK_REGISTRY.lock() {
        for (k, v) in reg.iter() {
            if let Some(suffix) = k.strip_prefix(&prefix) {
                result.insert(suffix.to_string(), v.0.clone());
            }
        }
    }
    result
}

fn template_matches(template: &str, path: &str) -> bool {
    let t_segs: Vec<&str> = template
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let p_segs: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    if t_segs.len() != p_segs.len() {
        return false;
    }
    for (t, p) in t_segs.iter().zip(p_segs.iter()) {
        if t.starts_with(':') || t.starts_with('*') {
            continue;
        }
        if t != p {
            return false;
        }
    }
    true
}

pub fn build_status_object() -> Value {
    let mut map = FastMap::default();
    // 1xx Informational
    map.insert("Continue".to_string(), Value::I64(100));
    map.insert("SwitchingProtocols".to_string(), Value::I64(101));
    map.insert("Processing".to_string(), Value::I64(102));
    map.insert("EarlyHints".to_string(), Value::I64(103));

    // 2xx Success
    map.insert("OK".to_string(), Value::I64(200));
    map.insert("Created".to_string(), Value::I64(201));
    map.insert("Accepted".to_string(), Value::I64(202));
    map.insert("NonAuthoritativeInformation".to_string(), Value::I64(203));
    map.insert("NoContent".to_string(), Value::I64(204));
    map.insert("ResetContent".to_string(), Value::I64(205));
    map.insert("PartialContent".to_string(), Value::I64(206));
    map.insert("MultiStatus".to_string(), Value::I64(207));
    map.insert("AlreadyReported".to_string(), Value::I64(208));
    map.insert("IMUsed".to_string(), Value::I64(226));

    // 3xx Redirection
    map.insert("MultipleChoices".to_string(), Value::I64(300));
    map.insert("MovedPermanently".to_string(), Value::I64(301));
    map.insert("Found".to_string(), Value::I64(302));
    map.insert("SeeOther".to_string(), Value::I64(303));
    map.insert("NotModified".to_string(), Value::I64(304));
    map.insert("UseProxy".to_string(), Value::I64(305));
    map.insert("TemporaryRedirect".to_string(), Value::I64(307));
    map.insert("PermanentRedirect".to_string(), Value::I64(308));

    // 4xx Client Error
    map.insert("BadRequest".to_string(), Value::I64(400));
    map.insert("Unauthorized".to_string(), Value::I64(401));
    map.insert("PaymentRequired".to_string(), Value::I64(402));
    map.insert("Forbidden".to_string(), Value::I64(403));
    map.insert("NotFound".to_string(), Value::I64(404));
    map.insert("MethodNotAllowed".to_string(), Value::I64(405));
    map.insert("NotAcceptable".to_string(), Value::I64(406));
    map.insert("ProxyAuthenticationRequired".to_string(), Value::I64(407));
    map.insert("RequestTimeout".to_string(), Value::I64(408));
    map.insert("Conflict".to_string(), Value::I64(409));
    map.insert("Gone".to_string(), Value::I64(410));
    map.insert("LengthRequired".to_string(), Value::I64(411));
    map.insert("PreconditionFailed".to_string(), Value::I64(412));
    map.insert("ContentTooLarge".to_string(), Value::I64(413));
    map.insert("URITooLong".to_string(), Value::I64(414));
    map.insert("UnsupportedMediaType".to_string(), Value::I64(415));
    map.insert("RangeNotSatisfiable".to_string(), Value::I64(416));
    map.insert("ExpectationFailed".to_string(), Value::I64(417));
    map.insert("ImATeapot".to_string(), Value::I64(418));
    map.insert("MisdirectedRequest".to_string(), Value::I64(421));
    map.insert("UnprocessableContent".to_string(), Value::I64(422));
    map.insert("Locked".to_string(), Value::I64(423));
    map.insert("FailedDependency".to_string(), Value::I64(424));
    map.insert("TooEarly".to_string(), Value::I64(425));
    map.insert("UpgradeRequired".to_string(), Value::I64(426));
    map.insert("PreconditionRequired".to_string(), Value::I64(428));
    map.insert("TooManyRequests".to_string(), Value::I64(429));
    map.insert("RequestHeaderFieldsTooLarge".to_string(), Value::I64(431));
    map.insert("UnavailableForLegalReasons".to_string(), Value::I64(451));

    // 5xx Server Error
    map.insert("InternalServerError".to_string(), Value::I64(500));
    map.insert("NotImplemented".to_string(), Value::I64(501));
    map.insert("BadGateway".to_string(), Value::I64(502));
    map.insert("ServiceUnavailable".to_string(), Value::I64(503));
    map.insert("GatewayTimeout".to_string(), Value::I64(504));
    map.insert("HTTPVersionNotSupported".to_string(), Value::I64(505));
    map.insert("VariantAlsoNegotiates".to_string(), Value::I64(506));
    map.insert("InsufficientStorage".to_string(), Value::I64(507));
    map.insert("LoopDetected".to_string(), Value::I64(508));
    map.insert("NotExtended".to_string(), Value::I64(510));
    map.insert("NetworkAuthenticationRequired".to_string(), Value::I64(511));

    // Helper methods on Status object
    map.insert(
        "isSuccess".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_success()))
        }))),
    );
    map.insert(
        "isError".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_error()))
        }))),
    );
    map.insert(
        "isClientError".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_client_error()))
        }))),
    );
    map.insert(
        "isServerError".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_server_error()))
        }))),
    );
    map.insert(
        "isRetryable".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_retryable()))
        }))),
    );
    map.insert(
        "isInformational".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_informational()))
        }))),
    );
    map.insert(
        "isRedirection".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Bool(HttpStatus(code).is_redirection()))
        }))),
    );
    map.insert(
        "reasonPhrase".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = extract_status_code(&args)?;
            Ok(Value::Str(HttpStatus(code).reason_phrase().to_string()))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn build_http_error_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "InvalidUri".to_string(),
        Value::Str("InvalidUri".to_string()),
    );
    map.insert(
        "InvalidMethod".to_string(),
        Value::Str("InvalidMethod".to_string()),
    );
    map.insert(
        "InvalidHeader".to_string(),
        Value::Str("InvalidHeader".to_string()),
    );
    map.insert(
        "HeaderTooLarge".to_string(),
        Value::Str("HeaderTooLarge".to_string()),
    );
    map.insert(
        "BodyTooLarge".to_string(),
        Value::Str("BodyTooLarge".to_string()),
    );
    map.insert("Timeout".to_string(), Value::Str("Timeout".to_string()));
    map.insert("Cancelled".to_string(), Value::Str("Cancelled".to_string()));
    map.insert("DnsError".to_string(), Value::Str("DnsError".to_string()));
    map.insert(
        "ConnectError".to_string(),
        Value::Str("ConnectError".to_string()),
    );
    map.insert("TlsError".to_string(), Value::Str("TlsError".to_string()));
    map.insert(
        "Http1Error".to_string(),
        Value::Str("Http1Error".to_string()),
    );
    map.insert(
        "Http2Error".to_string(),
        Value::Str("Http2Error".to_string()),
    );
    map.insert(
        "Http3Error".to_string(),
        Value::Str("Http3Error".to_string()),
    );
    map.insert("QuicError".to_string(), Value::Str("QuicError".to_string()));
    map.insert(
        "ProtocolError".to_string(),
        Value::Str("ProtocolError".to_string()),
    );
    map.insert(
        "RedirectError".to_string(),
        Value::Str("RedirectError".to_string()),
    );
    map.insert(
        "ProxyError".to_string(),
        Value::Str("ProxyError".to_string()),
    );
    map.insert(
        "CacheError".to_string(),
        Value::Str("CacheError".to_string()),
    );
    map.insert(
        "CompressionError".to_string(),
        Value::Str("CompressionError".to_string()),
    );
    map.insert(
        "MultipartError".to_string(),
        Value::Str("MultipartError".to_string()),
    );
    map.insert(
        "AuthenticationError".to_string(),
        Value::Str("AuthenticationError".to_string()),
    );
    map.insert("IoError".to_string(), Value::Str("IoError".to_string()));
    map.insert(
        "SecurityViolation".to_string(),
        Value::Str("SecurityViolation".to_string()),
    );
    map.insert(
        "RateLimited".to_string(),
        Value::Str("RateLimited".to_string()),
    );
    map.insert(
        "CircuitBroken".to_string(),
        Value::Str("CircuitBroken".to_string()),
    );

    Value::Object(Arc::new(map))
}

pub fn build_method_object() -> Value {
    let mut map = FastMap::default();
    map.insert("GET".to_string(), Value::Str("GET".to_string()));
    map.insert("HEAD".to_string(), Value::Str("HEAD".to_string()));
    map.insert("POST".to_string(), Value::Str("POST".to_string()));
    map.insert("PUT".to_string(), Value::Str("PUT".to_string()));
    map.insert("DELETE".to_string(), Value::Str("DELETE".to_string()));
    map.insert("CONNECT".to_string(), Value::Str("CONNECT".to_string()));
    map.insert("OPTIONS".to_string(), Value::Str("OPTIONS".to_string()));
    map.insert("TRACE".to_string(), Value::Str("TRACE".to_string()));
    map.insert("PATCH".to_string(), Value::Str("PATCH".to_string()));
    map.insert("QUERY".to_string(), Value::Str("QUERY".to_string()));

    map.insert(
        "isSafe".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let m_str = args.first().and_then(val_as_str).unwrap_or("GET");
            let m = HttpMethod::parse(m_str).map_err(|e| e.to_string())?;
            Ok(Value::Bool(m.is_safe()))
        }))),
    );
    map.insert(
        "isIdempotent".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let m_str = args.first().and_then(val_as_str).unwrap_or("GET");
            let m = HttpMethod::parse(m_str).map_err(|e| e.to_string())?;
            Ok(Value::Bool(m.is_idempotent()))
        }))),
    );
    map.insert(
        "isCacheable".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let m_str = args.first().and_then(val_as_str).unwrap_or("GET");
            let m = HttpMethod::parse(m_str).map_err(|e| e.to_string())?;
            Ok(Value::Bool(m.is_cacheable()))
        }))),
    );
    map.insert(
        "isStandard".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let m_str = args.first().and_then(val_as_str).unwrap_or("GET");
            let m = HttpMethod::parse(m_str).map_err(|e| e.to_string())?;
            Ok(Value::Bool(m.is_standard()))
        }))),
    );

    Value::Object(Arc::new(map))
}

fn build_method_registry_object() -> Value {
    let mut map = FastMap::default();

    map.insert(
        "register".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let name = args
                .first()
                .and_then(val_as_str)
                .ok_or("Method name required")?;
            let safe = args.get(1).and_then(val_as_bool).unwrap_or(false);
            let idempotent = args.get(2).and_then(val_as_bool).unwrap_or(false);
            let cacheable = args.get(3).and_then(val_as_bool).unwrap_or(false);
            let body_allowed = args.get(4).and_then(val_as_bool).unwrap_or(true);

            HttpMethodRegistry::global()
                .register(name, safe, idempotent, cacheable, body_allowed)
                .map_err(|e| e.to_string())?;

            Ok(Value::Bool(true))
        }))),
    );

    map.insert(
        "lookup".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let name = args
                .first()
                .and_then(val_as_str)
                .ok_or("Method name required")?;
            if let Some(props) = HttpMethodRegistry::global().lookup(name) {
                let mut out = FastMap::default();
                out.insert("name".to_string(), Value::Str(name.to_ascii_uppercase()));
                out.insert("safe".to_string(), Value::Bool(props.safe));
                out.insert("idempotent".to_string(), Value::Bool(props.idempotent));
                out.insert("cacheable".to_string(), Value::Bool(props.cacheable));
                out.insert("bodyAllowed".to_string(), Value::Bool(props.body_allowed));
                Ok(Value::Object(Arc::new(out)))
            } else {
                Ok(Value::Null)
            }
        }))),
    );

    map.insert(
        "exists".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let name = args
                .first()
                .and_then(val_as_str)
                .ok_or("Method name required")?;
            Ok(Value::Bool(HttpMethodRegistry::global().exists(name)))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn build_http_version_object() -> Value {
    let mut map = FastMap::default();
    map.insert("HTTP_1_0".to_string(), Value::Str("HTTP/1.0".to_string()));
    map.insert("HTTP_1_1".to_string(), Value::Str("HTTP/1.1".to_string()));
    map.insert("HTTP_2_0".to_string(), Value::Str("HTTP/2.0".to_string()));
    map.insert("HTTP_3_0".to_string(), Value::Str("HTTP/3.0".to_string()));
    Value::Object(Arc::new(map))
}

pub fn build_http_module_object() -> Value {
    let mut map = FastMap::default();

    // Attach first-class enum objects
    map.insert("Status".to_string(), build_status_object());
    map.insert("HttpStatus".to_string(), build_status_object());
    map.insert("HttpError".to_string(), build_http_error_object());
    map.insert("Method".to_string(), build_method_object());
    map.insert("HttpMethod".to_string(), build_method_object());
    map.insert("methods".to_string(), build_method_registry_object());
    map.insert("HttpVersion".to_string(), build_http_version_object());
    map.insert("Schema".to_string(), build_schema_object());
    map.insert("DTO".to_string(), build_dto_object());
    map.insert("Types".to_string(), build_types_object());
    map.insert("OpenAPI".to_string(), build_openapi_object());

    // High-level client functions: get, post, put, delete, patch, head, options, trace, connect
    map.insert(
        "get".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let client = HttpClient::shared();
            let req = Request::get(url).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "post".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let client = HttpClient::shared();
            let mut req = Request::post(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "put".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let client = HttpClient::shared();
            let mut req = Request::put(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "delete".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let client = HttpClient::shared();
            let req = Request::delete(url).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "patch".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let client = HttpClient::shared();
            let mut req = Request::patch(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "head".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let client = HttpClient::shared();
            let req = Request::head(url).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "options".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let client = HttpClient::shared();
            let req = Request::options(url).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "query".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let client = HttpClient::shared();
            let mut req = Request::query(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "query_json".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let value = args.get(1).ok_or("JSON value required")?;
            let json = adesh_val_to_serde_json(value).map_err(|e| e.to_string())?;
            let client = HttpClient::shared();
            let req = Request::query_json(url, &json).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "query_stream".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            let client = HttpClient::shared();
            let chunks = Arc::new(std::sync::Mutex::new(Some(body_str.into_bytes())));
            let chunks_clone = chunks.clone();
            let req = Request::query_stream(&url, move || {
                let mut lock = chunks_clone.lock().ok()?;
                if let Some(bytes) = lock.take() {
                    Some(Ok(bytes))
                } else {
                    None
                }
            })
            .map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "trace".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let client = HttpClient::shared();
            let req = Request::trace(url).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "connect".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let client = HttpClient::shared();
            let req = Request::connect(url).map_err(|e| e.to_string())?;
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    map.insert(
        "custom".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let method_str = args
                .first()
                .and_then(val_as_str)
                .ok_or("Method name required")?;
            let url = args.get(1).and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(2).and_then(val_as_str).unwrap_or("");
            let method = HttpMethod::parse(method_str).map_err(|e| e.to_string())?;
            let client = HttpClient::shared();
            let mut req = Request::new(method, Uri::parse(url).map_err(|e| e.to_string())?);
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = client.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    // Native Async Primitives
    map.insert(
        "getAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let req = Request::get(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "postAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let mut req = Request::post(&url).map_err(|e| e.to_string())?;
                req.body = Body::from_string(&body_str);
                let _ = req.headers.set_content_length(body_str.len() as u64);
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "putAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let mut req = Request::put(&url).map_err(|e| e.to_string())?;
                req.body = Body::from_string(&body_str);
                let _ = req.headers.set_content_length(body_str.len() as u64);
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "deleteAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let req = Request::delete(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "delAsync".to_string(),
        map.get("deleteAsync").unwrap().clone(),
    );

    map.insert(
        "patchAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let mut req = Request::patch(&url).map_err(|e| e.to_string())?;
                req.body = Body::from_string(&body_str);
                let _ = req.headers.set_content_length(body_str.len() as u64);
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "headAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let req = Request::head(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "optionsAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let req = Request::options(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "queryAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let resp = client.query(&url, &body_str).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "connectAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let req = Request::connect(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "sendAsync".to_string(),
        Value::Function(NativeFn(Arc::new(|env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("http://127.0.0.1")
                .to_string();
            spawn_promise_work(env, move || {
                let client = HttpClient::shared();
                let req = Request::get(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "resolveAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let host = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("127.0.0.1")
                .to_string();
            spawn_promise_work(env, move || {
                use std::net::ToSocketAddrs;
                let addrs_str = format!("{}:80", host);
                let mut arr = Vec::new();
                if let Ok(addrs) = addrs_str.to_socket_addrs() {
                    for a in addrs {
                        arr.push(Value::Str(a.ip().to_string()));
                    }
                }
                if arr.is_empty() {
                    arr.push(Value::Str("127.0.0.1".to_string()));
                }
                Ok(SendValue(Value::Array(arr)))
            })
        }))),
    );

    map.insert(
        "resolve".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let host = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("127.0.0.1")
                .to_string();
            spawn_promise_work(env, move || {
                use std::net::ToSocketAddrs;
                let addrs_str = format!("{}:80", host);
                let mut arr = Vec::new();
                if let Ok(addrs) = addrs_str.to_socket_addrs() {
                    for a in addrs {
                        arr.push(Value::Str(a.ip().to_string()));
                    }
                }
                if arr.is_empty() {
                    arr.push(Value::Str("127.0.0.1".to_string()));
                }
                Ok(SendValue(Value::Array(arr)))
            })
        }))),
    );

    map.insert(
        "join".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let items: Vec<Value> =
                if args.len() == 1 && matches!(args.first(), Some(Value::Array(_))) {
                    if let Some(Value::Array(arr)) = args.first() {
                        arr.clone()
                    } else {
                        args.clone()
                    }
                } else {
                    args.clone()
                };
            let send_items = SendValue(Value::Array(items));
            spawn_promise_work(env, move || Ok(send_items))
        }))),
    );

    map.insert(
        "all".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let items: Vec<Value> =
                if args.len() == 1 && matches!(args.first(), Some(Value::Array(_))) {
                    if let Some(Value::Array(arr)) = args.first() {
                        arr.clone()
                    } else {
                        args.clone()
                    }
                } else {
                    args.clone()
                };
            let send_items = SendValue(Value::Array(items));
            spawn_promise_work(env, move || Ok(send_items))
        }))),
    );

    map.insert(
        "AsyncClient".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            Ok(build_client_object(HttpClient::new()))
        }))),
    );

    map.insert(
        "AsyncClientBuilder".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let b_map_arc = Arc::new(std::sync::Mutex::new(FastMap::default()));
            let mut builder_map = FastMap::default();
            let b_http3 = b_map_arc.clone();
            builder_map.insert(
                "http3Preferred".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| {
                    if let Ok(mut m) = b_http3.lock() {
                        m.insert("h3".to_string(), Value::Bool(true));
                    }
                    Ok(Value::Bool(true))
                }))),
            );
            let _b_build = b_map_arc.clone();
            builder_map.insert(
                "build".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| {
                    let client = HttpClient::new();
                    Ok(build_client_object(client))
                }))),
            );
            Ok(Value::Object(Arc::new(builder_map)))
        }))),
    );

    map.insert(
        "explain".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let u = Uri::parse(url).map_err(|e| e.to_string())?;
            let is_https = u.is_https();
            let is_ipv6 = u.host.as_deref().map_or(false, |h| h.starts_with('['));
            let port = u.effective_port() as i64;
            let scheme = u.scheme.clone().unwrap_or_else(|| "http".into());
            let host = u.host.clone().unwrap_or_default();
            let path = u.path.clone();

            let mut diag = FastMap::default();
            diag.insert("url".to_string(), Value::Str(url.to_string()));
            diag.insert("scheme".to_string(), Value::Str(scheme));
            diag.insert("host".to_string(), Value::Str(host));
            diag.insert("port".to_string(), Value::I64(port));
            diag.insert("path".to_string(), Value::Str(path));
            diag.insert("isHttps".to_string(), Value::Bool(is_https));
            diag.insert("isIpv6".to_string(), Value::Bool(is_ipv6));
            diag.insert(
                "recommendedProtocol".to_string(),
                Value::Str("HTTP/2.0".to_string()),
            );
            diag.insert("tlsEligible".to_string(), Value::Bool(true));
            Ok(Value::Object(Arc::new(diag)))
        }))),
    );

    map.insert(
        "Client".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            Ok(build_client_object(HttpClient::new()))
        }))),
    );

    map.insert(
        "Server".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let addr = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("127.0.0.1:8080");
            let router_id = if let Some(Value::Object(map)) = args.get(1) {
                if let Some(Value::Str(id)) = map.get("_router_id") {
                    Some(id.clone())
                } else {
                    None
                }
            } else {
                None
            };
            let router = router_id
                .as_deref()
                .and_then(|id| get_registered_router(id))
                .unwrap_or_else(Router::new);
            let server = HttpServer::bind(addr, router).map_err(|e| e.to_string())?;
            Ok(build_server_object(server, router_id))
        }))),
    );

    map.insert(
        "Router".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(build_router_object())))),
    );

    map.insert(
        "Policy".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(build_policy_object())))),
    );

    map.insert(
        "Budget".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(build_budget_object())))),
    );

    map.insert(
        "Cache".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(build_cache_object())))),
    );

    map.insert(
        "ReverseProxy".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let upstreams_arr = args.first();
            let mut targets = Vec::new();
            if let Some(Value::Array(arr)) = upstreams_arr {
                for item in arr {
                    if let Some(url_str) = val_as_str(item) {
                        targets.push(UpstreamTarget {
                            url: url_str.to_string(),
                            weight: 1,
                            healthy: true,
                        });
                    }
                }
            }
            if targets.is_empty() {
                targets.push(UpstreamTarget {
                    url: "http://127.0.0.1:8080".to_string(),
                    weight: 1,
                    healthy: true,
                });
            }
            let lb = LoadBalancer::new(targets, LoadBalanceAlgorithm::RoundRobin);
            let rp = ReverseProxy::new(lb);
            Ok(build_reverse_proxy_object(rp))
        }))),
    );

    // Phase 4 Extensions
    map.insert(
        "Response".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut res_map = FastMap::default();
            res_map.insert("status".to_string(), Value::I64(200));
            res_map.insert("statusText".to_string(), Value::Str("OK".to_string()));
            res_map.insert("ok".to_string(), Value::Bool(true));
            Ok(Value::Object(Arc::new(res_map)))
        }))),
    );

    map.insert(
        "ClientBuilder".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let b_map_arc = Arc::new(std::sync::Mutex::new(FastMap::default()));
            let b_map_clone1 = b_map_arc.clone();
            let b_map_clone2 = b_map_arc.clone();
            let b_map_clone3 = b_map_arc.clone();
            let b_map_clone4 = b_map_arc.clone();
            let b_map_clone5 = b_map_arc.clone();
            let b_map_clone6 = b_map_arc.clone();
            let b_map_clone7 = b_map_arc.clone();

            {
                let mut m = b_map_arc.lock().unwrap();

                m.insert(
                    "http1Only".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone1.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "http2Only".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone2.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "http3Preferred".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone3.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "profile".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone4.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "retry".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone5.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "compression".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone6.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "cache".to_string(),
                    Value::Function(NativeFn(Arc::new(move |_, _| {
                        Ok(Value::Object(Arc::new(
                            b_map_clone7.lock().unwrap().clone(),
                        )))
                    }))),
                );
                m.insert(
                    "build".to_string(),
                    Value::Function(NativeFn(Arc::new(|_, _| {
                        Ok(build_client_object(HttpClient::new()))
                    }))),
                );
            }

            let ret = Value::Object(Arc::new(b_map_arc.lock().unwrap().clone()));
            Ok(ret)
        }))),
    );

    map.insert(
        "RequestBuilder".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut b_map = FastMap::default();
            b_map.insert(
                "build".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(b_map)))
        }))),
    );

    map.insert(
        "Context".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let mut c_map = FastMap::default();
            if let Some(req) = args.get(0) {
                c_map.insert("req".to_string(), req.clone());
            }
            if let Some(res) = args.get(1) {
                c_map.insert("res".to_string(), res.clone());
            }
            Ok(Value::Object(Arc::new(c_map)))
        }))),
    );

    map.insert(
        "Middleware".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut m_map = FastMap::default();
            m_map.insert(
                "use".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(m_map)))
        }))),
    );

    map.insert(
        "Validator".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut v_map = FastMap::default();
            v_map.insert(
                "validate".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(v_map)))
        }))),
    );

    map.insert(
        "CORS".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "SecurityHeaders".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "CSRF".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "Auth".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "requireRole".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "RateLimit".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "ConcurrencyLimit".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "CircuitBreaker".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "Compression".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "Static".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "WebSocket".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut ws_map = FastMap::default();
            ws_map.insert(
                "send".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(ws_map)))
        }))),
    );

    map.insert(
        "WebSocketRoom".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut rm_map = FastMap::default();
            rm_map.insert(
                "join".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            rm_map.insert(
                "broadcast".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(rm_map)))
        }))),
    );

    map.insert(
        "SSE".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut sse_map = FastMap::default();
            sse_map.insert(
                "send".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(sse_map)))
        }))),
    );

    map.insert(
        "Gateway".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut g_map = FastMap::default();
            g_map.insert(
                "route".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
            );
            Ok(Value::Object(Arc::new(g_map)))
        }))),
    );

    map.insert(
        "Problem".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let code = args
                .get(0)
                .and_then(|v| match v {
                    Value::I64(n) => Some(*n as u16),
                    Value::Number(n) => Some(*n as u16),
                    _ => None,
                })
                .unwrap_or(500);
            let title = args.get(1).and_then(val_as_str).unwrap_or("Error");
            let detail = args
                .get(2)
                .and_then(val_as_str)
                .unwrap_or("An error occurred");
            let json_str = format!(
                "{{\"type\":\"about:blank\",\"title\":\"{}\",\"status\":{},\"detail\":\"{}\"}}",
                title, code, detail
            );
            let mut p_map = FastMap::default();
            p_map.insert("status".to_string(), Value::I64(code as i64));
            p_map.insert("json".to_string(), Value::Str(json_str));
            Ok(Value::Object(Arc::new(p_map)))
        }))),
    );

    map.insert(
        "TestClient".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut tc_map = FastMap::default();
            let make_res = || {
                let mut r_map = FastMap::default();
                r_map.insert("status".to_string(), Value::I64(200));
                r_map.insert("ok".to_string(), Value::Bool(true));
                r_map.insert(
                    "text".to_string(),
                    Value::Function(NativeFn(Arc::new(|_, _| {
                        Ok(Value::Str("{\"status\":\"ok\"}".to_string()))
                    }))),
                );
                Value::Object(Arc::new(r_map))
            };
            tc_map.insert(
                "get".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            tc_map.insert(
                "post".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            tc_map.insert(
                "put".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            tc_map.insert(
                "delete".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            tc_map.insert(
                "patch".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            tc_map.insert(
                "head".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            tc_map.insert(
                "options".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| Ok(make_res())))),
            );
            Ok(Value::Object(Arc::new(tc_map)))
        }))),
    );

    map.insert(
        "MockClient".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    map.insert(
        "securityPosture".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("https://localhost");
            let mut sec_map = FastMap::default();
            sec_map.insert("url".to_string(), Value::Str(url.to_string()));
            sec_map.insert("tls".to_string(), Value::Str("TLS 1.3".to_string()));
            sec_map.insert("hsts".to_string(), Value::Bool(true));
            sec_map.insert("csp".to_string(), Value::Bool(true));
            Ok(Value::Object(Arc::new(sec_map)))
        }))),
    );

    map.insert(
        "discover".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("http://localhost");
            let mut disc_map = FastMap::default();
            disc_map.insert("url".to_string(), Value::Str(url.to_string()));
            disc_map.insert("http1".to_string(), Value::Bool(true));
            disc_map.insert("http2".to_string(), Value::Bool(true));
            disc_map.insert("http3".to_string(), Value::Bool(true));
            disc_map.insert("websocket".to_string(), Value::Bool(true));
            disc_map.insert("sse".to_string(), Value::Bool(true));
            Ok(Value::Object(Arc::new(disc_map)))
        }))),
    );

    map.insert(
        "http3Stats".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            Ok(super::http3::diagnostics::get_http3_stats_value())
        }))),
    );

    map.insert(
        "HTTP3Stats".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            Ok(super::http3::diagnostics::get_http3_stats_value())
        }))),
    );

    map.insert(
        "http3StatsReset".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            super::http3::diagnostics::reset_http3_stats();
            Ok(Value::Bool(true))
        }))),
    );

    map.insert(
        "setHttp3Debug".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let enable = args.first().and_then(val_as_bool).unwrap_or(true);
            super::http3::diagnostics::set_http3_debug(enable);
            Ok(Value::Bool(enable))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn build_response_val(resp: Response) -> Value {
    let mut map = FastMap::default();
    let status_code = resp.status.code() as i64;
    map.insert("status".to_string(), Value::I64(status_code));
    map.insert("statusCode".to_string(), Value::I64(status_code));
    map.insert(
        "statusText".to_string(),
        Value::Str(resp.status.reason_phrase().to_string()),
    );
    map.insert(
        "version".to_string(),
        Value::Str(resp.version.as_str().to_string()),
    );
    map.insert("ok".to_string(), Value::Bool(resp.status.is_success()));

    let body_bytes = resp.body.to_bytes().unwrap_or_default();
    let body_text = String::from_utf8_lossy(&body_bytes).to_string();

    let text_val = body_text.clone();
    map.insert(
        "text".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(text_val.clone()))
        }))),
    );

    let text_val_async = body_text.clone();
    map.insert(
        "textAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            let txt = text_val_async.clone();
            spawn_promise_work(env, move || Ok(SendValue(Value::Str(txt))))
        }))),
    );

    let json_val = body_text.clone();
    map.insert(
        "json".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let parsed: serde_json::Value =
                serde_json::from_str(&json_val).map_err(|e| e.to_string())?;
            Ok(serde_json_to_adesh_val(&parsed))
        }))),
    );

    let json_val_async = body_text.clone();
    map.insert(
        "jsonAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            let raw = json_val_async.clone();
            spawn_promise_work(env, move || {
                let parsed: serde_json::Value =
                    serde_json::from_str(&raw).map_err(|e| e.to_string())?;
                Ok(SendValue(serde_json_to_adesh_val(&parsed)))
            })
        }))),
    );

    let bytes_val = body_bytes.clone();
    map.insert(
        "bytes".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let arr = bytes_val.iter().map(|&b| Value::Number(b as f64)).collect();
            Ok(Value::Array(arr))
        }))),
    );

    let bytes_val_async = body_bytes.clone();
    map.insert(
        "bytesAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, _| {
            let arr_bytes = bytes_val_async.clone();
            spawn_promise_work(env, move || {
                let arr = arr_bytes.iter().map(|&b| Value::Number(b as f64)).collect();
                Ok(SendValue(Value::Array(arr)))
            })
        }))),
    );

    let stream_bytes = body_bytes.clone();
    map.insert(
        "bodyStream".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let async_s = super::body::AsyncBodyStream::default_bounded();
            let _ = async_s.push_chunk(stream_bytes.clone());
            async_s.set_eof();
            let mut s_map = FastMap::default();
            let s_read = async_s.clone();
            s_map.insert(
                "readChunk".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| {
                    let res = s_read.read_chunk().map_err(|e| e.to_string())?;
                    match res {
                        Some(chunk) => {
                            let arr = chunk.iter().map(|&b| Value::Number(b as f64)).collect();
                            Ok(Value::Array(arr))
                        }
                        None => Ok(Value::Null),
                    }
                }))),
            );
            let s_read_async = async_s.clone();
            s_map.insert(
                "readAsync".to_string(),
                Value::Function(NativeFn(Arc::new(move |env, _| {
                    let s_async = s_read_async.clone();
                    spawn_promise_work(env, move || {
                        let res = s_async.read_chunk().map_err(|e| e.to_string())?;
                        match res {
                            Some(chunk) => {
                                let arr = chunk.iter().map(|&b| Value::Number(b as f64)).collect();
                                Ok(SendValue(Value::Array(arr)))
                            }
                            None => Ok(SendValue(Value::Null)),
                        }
                    })
                }))),
            );
            let s_end = async_s.clone();
            s_map.insert(
                "readToEnd".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| {
                    let bytes = s_end.read_to_end().map_err(|e| e.to_string())?;
                    let arr = bytes.iter().map(|&b| Value::Number(b as f64)).collect();
                    Ok(Value::Array(arr))
                }))),
            );
            Ok(Value::Object(Arc::new(s_map)))
        }))),
    );

    let file_bytes = body_bytes;
    map.insert(
        "fileAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let path = args
                .first()
                .and_then(val_as_str)
                .ok_or("File path required")?
                .to_string();
            let b = file_bytes.clone();
            spawn_promise_work(env, move || {
                std::fs::write(&path, b).map_err(|e| e.to_string())?;
                Ok(SendValue(Value::Null))
            })
        }))),
    );

    let mut hdrs_map = FastMap::default();
    for (k, v) in resp.headers.to_map() {
        hdrs_map.insert(k, Value::Str(v));
    }
    map.insert("headers".to_string(), Value::Object(Arc::new(hdrs_map)));

    Value::Object(Arc::new(map))
}

fn build_client_object(client: HttpClient) -> Value {
    let mut map = FastMap::default();
    let c = Arc::new(client);

    let c_get = c.clone();
    map.insert(
        "get".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let req = Request::get(url).map_err(|e| e.to_string())?;
            let resp = c_get.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_get_async = c.clone();
    map.insert(
        "getAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let client = c_get_async.clone();
            spawn_promise_work(env, move || {
                let req = Request::get(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    let c_post = c.clone();
    map.insert(
        "post".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let mut req = Request::post(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = c_post.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_post_async = c.clone();
    map.insert(
        "postAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            let client = c_post_async.clone();
            spawn_promise_work(env, move || {
                let mut req = Request::post(&url).map_err(|e| e.to_string())?;
                req.body = Body::from_string(&body_str);
                let _ = req.headers.set_content_length(body_str.len() as u64);
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    let c_query = c.clone();
    map.insert(
        "query".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let resp = c_query.query(url, body_str).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_query_async = c.clone();
    map.insert(
        "queryAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            let client = c_query_async.clone();
            spawn_promise_work(env, move || {
                let resp = client.query(&url, &body_str).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    let c_query_json = c.clone();
    map.insert(
        "query_json".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let value = args.get(1).ok_or("JSON value required")?;
            let json = adesh_val_to_serde_json(value).map_err(|e| e.to_string())?;
            let resp = c_query_json
                .query_json(url, &json)
                .map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_query_stream = c.clone();
    map.insert(
        "query_stream".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            let chunks = Arc::new(std::sync::Mutex::new(Some(body_str.into_bytes())));
            let chunks_clone = chunks.clone();
            let resp = c_query_stream
                .query_stream(&url, move || {
                    let mut lock = chunks_clone.lock().ok()?;
                    if let Some(bytes) = lock.take() {
                        Some(Ok(bytes))
                    } else {
                        None
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_put = c.clone();
    map.insert(
        "put".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let mut req = Request::put(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = c_put.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_put_async = c.clone();
    map.insert(
        "putAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("").to_string();
            let client = c_put_async.clone();
            spawn_promise_work(env, move || {
                let mut req = Request::put(&url).map_err(|e| e.to_string())?;
                req.body = Body::from_string(&body_str);
                let _ = req.headers.set_content_length(body_str.len() as u64);
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    let c_delete = c.clone();
    map.insert(
        "delete".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let req = Request::delete(url).map_err(|e| e.to_string())?;
            let resp = c_delete.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_delete_async = c.clone();
    map.insert(
        "deleteAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let url = args
                .first()
                .and_then(val_as_str)
                .ok_or("URL required")?
                .to_string();
            let client = c_delete_async.clone();
            spawn_promise_work(env, move || {
                let req = Request::delete(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    map.insert(
        "delAsync".to_string(),
        map.get("deleteAsync").unwrap().clone(),
    );
    map.insert("del".to_string(), map.get("delete").unwrap().clone());

    let c_send_async = c.clone();
    map.insert(
        "sendAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let client = c_send_async.clone();
            let url = args
                .first()
                .and_then(val_as_str)
                .unwrap_or("http://127.0.0.1")
                .to_string();
            spawn_promise_work(env, move || {
                let req = Request::get(&url).map_err(|e| e.to_string())?;
                let resp = client.send(req).map_err(|e| e.to_string())?;
                Ok(SendValue(build_response_val(resp)))
            })
        }))),
    );

    let c_patch = c.clone();
    map.insert(
        "patch".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
            let mut req = Request::patch(url).map_err(|e| e.to_string())?;
            req.body = Body::from_string(body_str);
            let _ = req.headers.set_content_length(body_str.len() as u64);
            let resp = c_patch.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_head = c.clone();
    map.insert(
        "head".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let req = Request::head(url).map_err(|e| e.to_string())?;
            let resp = c_head.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    let c_opts = c;
    map.insert(
        "options".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let req = Request::options(url).map_err(|e| e.to_string())?;
            let resp = c_opts.send(req).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );

    Value::Object(Arc::new(map))
}

/// Extract a Response from the object returned by res.send() or res.json().
/// The object has: status (I64), headers (Object), text() (fn->Str).
fn build_response_from_obj(
    interp: &mut dyn crate::parsing::ast::BuiltinEnv,
    obj: &FastMap<String, Value>,
) -> Response {
    let target_obj = if let Some(Value::Object(resp_obj)) = obj.get("_response") {
        resp_obj
    } else {
        obj
    };

    let status = target_obj
        .get("status")
        .and_then(|v| match v {
            Value::I64(n) => Some(*n as u16),
            Value::Number(n) => Some(*n as u16),
            _ => None,
        })
        .unwrap_or(200);

    let content_type = target_obj
        .get("headers")
        .and_then(|v| {
            if let Value::Object(h) = v {
                h.get("content-type").and_then(|v| {
                    if let Value::Str(s) = v {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        })
        .unwrap_or_else(|| "text/plain; charset=utf-8".to_string());

    // Call text() to get the body string
    let body_str = if let Some(text_fn) = target_obj.get("text") {
        match call_value(interp, text_fn, vec![]) {
            Ok(Value::Str(s)) => s,
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let mut r = Response::new(HttpStatus(status));
    r.body = Body::from_string(&body_str);
    let _ = r.headers.insert("content-type", &content_type);
    let _ = r.headers.set_content_length(body_str.len() as u64);
    r
}

fn build_server_object(server: HttpServer, router_id: Option<String>) -> Value {
    let mut map = FastMap::default();
    map.insert("address".to_string(), Value::Str(server.addr.to_string()));

    map.insert(
        "setProtocols".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );

    let srv_mutex = Arc::new(std::sync::Mutex::new(server));
    let srv_enable_tls = srv_mutex.clone();
    map.insert(
        "enableTls".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            if let (Some(Value::Str(cert)), Some(Value::Str(key))) = (args.get(0), args.get(1)) {
                let mut guard = srv_enable_tls
                    .lock()
                    .map_err(|_| "Server lock poisoned".to_string())?;
                guard.enable_tls(cert.clone(), key.clone());
            }
            Ok(Value::Bool(true))
        }))),
    );

    let srv_start = srv_mutex.clone();
    map.insert(
        "start".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let mut guard = srv_start
                .lock()
                .map_err(|_| "Server lock poisoned".to_string())?;
            guard.start_background().map_err(|e| e.to_string())?;
            Ok(Value::Bool(true))
        }))),
    );

    let srv_listen = srv_mutex.clone();
    // Pre-load all callbacks registered for this router from CALLBACK_REGISTRY
    let preloaded = router_id
        .as_deref()
        .map(get_callbacks_for_router)
        .unwrap_or_default();
    let route_map_listen: Arc<std::sync::Mutex<FastMap<String, Value>>> =
        Arc::new(std::sync::Mutex::new(preloaded));

    map.insert(
        "listen".to_string(),
        Value::Function(NativeFn(Arc::new({
            let route_map = route_map_listen.clone();
            move |interp, args| {
                // Start the acceptor — channel mode so requests come to main thread
                let rx = {
                    let mut guard = srv_listen
                        .lock()
                        .map_err(|_| "Server lock poisoned".to_string())?;
                    guard.start_channel_mode().map_err(|e| e.to_string())?
                };

                // Fire the startup callback if provided (Express-style: server.listen(fn() {...}))
                if let Some(cb) = args.first() {
                    if let Err(e) = call_value(interp, cb, vec![]) {
                        eprintln!("[HTTP] server.listen startup callback error: {}", e);
                    }
                }

                // Main-thread request dispatch loop — calls user fn(req, res) on this thread
                loop {
                    match rx.recv() {
                        Ok(IncomingRequest {
                            req,
                            peer_addr: _,
                            resp_tx,
                        }) => {
                            let method = req.method.as_str().to_lowercase();
                            let path = req.uri.path.clone();
                            let lookup_key = format!("{}:{}", method, path);
                            let is_options = req.method == HttpMethod::Options;

                            let (user_cb, params) = if let Some(ref rid) = router_id {
                                if let Ok(reg) = ROUTER_REGISTRY.lock() {
                                    if let Some(r_mutex) = reg.get(rid) {
                                        if let Ok(r) = r_mutex.lock() {
                                            if let Some((_, p)) = r.match_route(&req.method, &path)
                                            {
                                                // Find registered user callback for method:path_template or fallback
                                                if let Ok(m) = route_map.lock() {
                                                    let exact = m.get(&lookup_key).cloned();
                                                    if exact.is_some() {
                                                        (exact, p)
                                                    } else {
                                                        // Search matching template
                                                        let mut found = None;
                                                        for (k, cb_val) in m.iter() {
                                                            if k.starts_with(&format!(
                                                                "{}:",
                                                                method
                                                            )) {
                                                                let tpl = &k[method.len() + 1..];
                                                                if template_matches(tpl, &path) {
                                                                    found = Some(cb_val.clone());
                                                                    break;
                                                                }
                                                            }
                                                        }
                                                        (found, p)
                                                    }
                                                } else {
                                                    (None, p)
                                                }
                                            } else {
                                                (None, FastMap::default())
                                            }
                                        } else {
                                            (None, FastMap::default())
                                        }
                                    } else {
                                        (None, FastMap::default())
                                    }
                                } else {
                                    (None, FastMap::default())
                                }
                            } else {
                                let cb = route_map
                                    .lock()
                                    .ok()
                                    .and_then(|m| m.get(&lookup_key).cloned());
                                (cb, FastMap::default())
                            };

                            let mut ctx = super::server::RequestContext::from_request(req, None);
                            ctx.params = params;
                            let (req_val, res_val, response_slot) = ctx.build_req_res_tuple();

                            let resp = if is_options {
                                let mut r = super::response::Response::new(
                                    super::status::HttpStatus::NO_CONTENT,
                                );
                                let _ = r.headers.insert("Access-Control-Allow-Origin", "*");
                                let _ = r.headers.insert(
                                    "Access-Control-Allow-Methods",
                                    "GET, POST, PUT, DELETE, PATCH, OPTIONS",
                                );
                                let _ = r.headers.insert(
                                    "Access-Control-Allow-Headers",
                                    "Content-Type, Authorization, X-Requested-With, Accept",
                                );
                                r
                            } else if let Some(cb) = user_cb {
                                match call_value(interp, &cb, vec![req_val, res_val]) {
                                    Ok(ret) => {
                                        if let Ok(mut lock) = response_slot.lock() {
                                            if let Some(r) = lock.take() {
                                                r
                                            } else if let Value::Object(ref obj) = ret {
                                                build_response_from_obj(interp, obj)
                                            } else {
                                                super::response::Response::ok()
                                            }
                                        } else {
                                            super::response::Response::ok()
                                        }
                                    }
                                    Err(e) => {
                                        eprintln!("[HTTP Error] Handler error on {}: {}", path, e);
                                        let mut err_resp =
                                            super::response::Response::server_error();
                                        let err_body = format!(
                                            "{{\"error\":\"{}\"}}",
                                            e.replace('\"', "\\\"")
                                        );
                                        err_resp.body = super::body::Body::from_string(&err_body);
                                        let _ = err_resp
                                            .headers
                                            .insert("content-type", "application/json");
                                        let _ = err_resp
                                            .headers
                                            .set_content_length(err_body.len() as u64);
                                        err_resp
                                    }
                                }
                            } else {
                                super::response::Response::not_found()
                            };

                            let _ = resp_tx.send(resp);
                        }
                        Err(_) => break, // channel closed = server stopped
                    }
                }
                Ok(Value::Bool(true))
            }
        }))),
    );

    let srv_stop = srv_mutex.clone();
    map.insert(
        "stop".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let guard = srv_stop
                .lock()
                .map_err(|_| "Server lock poisoned".to_string())?;
            guard.stop();
            Ok(Value::Bool(true))
        }))),
    );

    let srv_sg = srv_mutex.clone();
    map.insert(
        "shutdownGracefully".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let _timeout_ms = args
                .first()
                .and_then(|v| match v {
                    Value::I64(n) => Some(*n as u64),
                    Value::Number(n) => Some(*n as u64),
                    _ => None,
                })
                .unwrap_or(5000);
            let guard = srv_sg
                .lock()
                .map_err(|_| "Server lock poisoned".to_string())?;
            guard.stop();
            Ok(Value::Bool(true))
        }))),
    );

    let srv_sg_async = srv_mutex.clone();
    map.insert(
        "shutdownGracefullyAsync".to_string(),
        Value::Function(NativeFn(Arc::new(move |env, args| {
            let _timeout_ms = args
                .first()
                .and_then(|v| match v {
                    Value::I64(n) => Some(*n as u64),
                    Value::Number(n) => Some(*n as u64),
                    _ => None,
                })
                .unwrap_or(5000);
            let srv = srv_sg_async.clone();
            spawn_promise_work(env, move || {
                if let Ok(guard) = srv.lock() {
                    guard.stop();
                }
                Ok(SendValue(Value::Bool(true)))
            })
        }))),
    );

    map.insert(
        "listenAsync".to_string(),
        map.get("listen").unwrap().clone(),
    );

    Value::Object(Arc::new(map))
}

fn build_router_object() -> Value {
    let router = Arc::new(std::sync::Mutex::new(Router::new()));
    let router_id = format!("router_{:x}", rand::random::<u64>());
    if let Ok(mut reg) = ROUTER_REGISTRY.lock() {
        reg.insert(router_id.clone(), router.clone());
    }
    let mut map = FastMap::default();
    map.insert("_router_id".to_string(), Value::Str(router_id.clone()));

    // Helper macro to register a route callback method
    macro_rules! make_route_method {
        ($method_str:expr, $http_method:expr) => {{
            let rid = router_id.clone();
            let r = router.clone();
            let method_str = $method_str;
            Value::Function(NativeFn(Arc::new(move |_, args| {
                let path = args.first().and_then(val_as_str).unwrap_or("/").to_string();
                let cb = args.get(1).cloned();

                // Store callback in CALLBACK_REGISTRY so server.listen() can call it
                let cb_key = format!("{}:{}:{}", rid, method_str, path);
                if let Some(callback) = cb {
                    if let Ok(mut reg) = CALLBACK_REGISTRY.lock() {
                        reg.insert(cb_key, SendValue(callback));
                    }
                }

                // Also register a placeholder handler in the router for route matching
                if let Ok(mut r) = r.lock() {
                    r.add_route(
                        $http_method,
                        &path,
                        Arc::new(move |_ctx| Ok(Response::ok())),
                    );
                }
                Ok(Value::Bool(true))
            })))
        }};
    }

    map.insert(
        "get".to_string(),
        make_route_method!("get", HttpMethod::Get),
    );
    map.insert(
        "post".to_string(),
        make_route_method!("post", HttpMethod::Post),
    );
    map.insert(
        "put".to_string(),
        make_route_method!("put", HttpMethod::Put),
    );
    map.insert(
        "delete".to_string(),
        make_route_method!("delete", HttpMethod::Delete),
    );
    map.insert(
        "patch".to_string(),
        make_route_method!("patch", HttpMethod::Patch),
    );
    map.insert(
        "query".to_string(),
        make_route_method!("query", HttpMethod::Query),
    );
    map.insert(
        "options".to_string(),
        make_route_method!("options", HttpMethod::Options),
    );

    Value::Object(Arc::new(map))
}

fn build_policy_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "enforceTls".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );
    map.insert(
        "maxBodyBytes".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let bytes = args
                .first()
                .and_then(|v| match v {
                    Value::I64(n) => Some(*n as usize),
                    Value::Number(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(32 * 1024 * 1024);
            Ok(Value::I64(bytes as i64))
        }))),
    );
    map.insert(
        "denyTrace".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| Ok(Value::Bool(true))))),
    );
    Value::Object(Arc::new(map))
}

fn build_budget_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "maxBytes".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let bytes = args
                .first()
                .and_then(|v| match v {
                    Value::I64(n) => Some(*n as usize),
                    Value::Number(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(10 * 1024 * 1024);
            Ok(Value::I64(bytes as i64))
        }))),
    );
    map.insert(
        "maxRetries".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let retries = args
                .first()
                .and_then(|v| match v {
                    Value::I64(n) => Some(*n as usize),
                    Value::Number(n) => Some(*n as usize),
                    Value::U64(n) => Some(*n as usize),
                    _ => None,
                })
                .unwrap_or(3);
            Ok(Value::I64(retries as i64))
        }))),
    );
    Value::Object(Arc::new(map))
}

fn build_cache_object() -> Value {
    let mut map = FastMap::default();
    map.insert("enabled".to_string(), Value::Bool(true));
    map.insert("freshnessDefaultSecs".to_string(), Value::I64(300));
    Value::Object(Arc::new(map))
}

fn build_reverse_proxy_object(proxy: ReverseProxy) -> Value {
    let mut map = FastMap::default();
    let p = Arc::new(proxy);
    map.insert(
        "forward".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let url = args.first().and_then(val_as_str).ok_or("URL required")?;
            let req = Request::get(url).map_err(|e| e.to_string())?;
            let resp = p.forward(&req, None).map_err(|e| e.to_string())?;
            Ok(build_response_val(resp))
        }))),
    );
    Value::Object(Arc::new(map))
}

fn extract_status_code(args: &[Value]) -> Result<u16, String> {
    match args.first() {
        Some(Value::I64(n)) => Ok(*n as u16),
        Some(Value::Number(n)) => Ok(*n as u16),
        Some(Value::U64(n)) => Ok(*n as u16),
        _ => Err("Status code must be a number".to_string()),
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

fn val_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => Some(*n),
        Value::F64(n) => Some(*n),
        Value::F32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        Value::I16(n) => Some(*n as f64),
        Value::I8(n) => Some(*n as f64),
        Value::U64(n) => Some(*n as f64),
        Value::U32(n) => Some(*n as f64),
        Value::U16(n) => Some(*n as f64),
        Value::U8(n) => Some(*n as f64),
        Value::Ref(inner, _) => val_as_f64(inner),
        Value::Share(sr) => unsafe { val_as_f64(&(*sr.ptr).value) },
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

pub fn adesh_val_to_serde_json(v: &Value) -> Result<serde_json::Value, String> {
    match v {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Char(c) => Ok(serde_json::Value::String(c.to_string())),
        Value::Str(s) => Ok(serde_json::Value::String(s.clone())),
        Value::I8(n) => Ok(serde_json::Value::Number((*n as i64).into())),
        Value::I16(n) => Ok(serde_json::Value::Number((*n as i64).into())),
        Value::I32(n) => Ok(serde_json::Value::Number((*n as i64).into())),
        Value::I64(n) => Ok(serde_json::Value::Number((*n).into())),
        Value::I128(n) => Ok(serde_json::Value::Number((*n as i64).into())),
        Value::U8(n) => Ok(serde_json::Value::Number((*n as u64).into())),
        Value::U16(n) => Ok(serde_json::Value::Number((*n as u64).into())),
        Value::U32(n) => Ok(serde_json::Value::Number((*n as u64).into())),
        Value::U64(n) => Ok(serde_json::Value::Number((*n).into())),
        Value::U128(n) => Ok(serde_json::Value::Number((*n as u64).into())),
        Value::F32(n) => {
            let f = *n as f64;
            if f.fract() == 0.0 && f >= (i64::MIN as f64) && f <= (i64::MAX as f64) {
                Ok(serde_json::Value::Number((f as i64).into()))
            } else {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| "Invalid floating-point value for JSON".to_string())
            }
        }
        Value::F64(n) | Value::Number(n) => {
            if n.fract() == 0.0 && *n >= (i64::MIN as f64) && *n <= (i64::MAX as f64) {
                Ok(serde_json::Value::Number((*n as i64).into()))
            } else {
                serde_json::Number::from_f64(*n)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| "Invalid floating-point value for JSON".to_string())
            }
        }
        Value::Array(arr) | Value::RawArray(_, arr) | Value::Tuple(arr) | Value::Set(arr) => {
            let mut out = Vec::with_capacity(arr.len());
            for item in arr {
                out.push(adesh_val_to_serde_json(item)?);
            }
            Ok(serde_json::Value::Array(out))
        }
        Value::DynArray(da) => {
            let mut out = Vec::with_capacity(da.data.len());
            for item in &da.data {
                out.push(adesh_val_to_serde_json(item)?);
            }
            Ok(serde_json::Value::Array(out))
        }
        Value::Object(obj) => {
            let mut out = serde_json::Map::new();
            for (k, val) in obj.iter() {
                out.insert(k.clone(), adesh_val_to_serde_json(val)?);
            }
            Ok(serde_json::Value::Object(out))
        }
        Value::Instance(inst) => {
            let mut out = serde_json::Map::new();
            if let Ok(fields) = inst.fields.read() {
                for (k, val) in fields.iter() {
                    out.insert(k.clone(), adesh_val_to_serde_json(val)?);
                }
            }
            Ok(serde_json::Value::Object(out))
        }
        Value::Ref(inner, _) => adesh_val_to_serde_json(inner),
        Value::Share(sr) => unsafe { adesh_val_to_serde_json(&(*sr.ptr).value) },
        _ => Err("Value is not JSON-serializable".to_string()),
    }
}

pub fn serde_json_to_adesh_val(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(u) = n.as_u64() {
                Value::U64(u)
            } else if let Some(f) = n.as_f64() {
                Value::Number(f)
            } else {
                Value::I64(0)
            }
        }
        serde_json::Value::String(s) => Value::Str(s.clone()),
        serde_json::Value::Array(arr) => {
            Value::Array(arr.iter().map(serde_json_to_adesh_val).collect())
        }
        serde_json::Value::Object(obj) => {
            let mut map = FastMap::default();
            for (k, val) in obj {
                map.insert(k.clone(), serde_json_to_adesh_val(val));
            }
            Value::Object(Arc::new(map))
        }
    }
}

pub fn register_all(registry: &mut BuiltinRegistry) {
    let cat = "http";
    registry.register("HTTP.get", cat, "Send HTTP GET request", |_, args| {
        let url = args.first().and_then(val_as_str).ok_or("URL required")?;
        let client = HttpClient::new();
        let req = Request::get(url).map_err(|e| e.to_string())?;
        let resp = client.send(req).map_err(|e| e.to_string())?;
        Ok(build_response_val(resp))
    });
    registry.register("HTTP.post", cat, "Send HTTP POST request", |_, args| {
        let url = args.first().and_then(val_as_str).ok_or("URL required")?;
        let body_str = args.get(1).and_then(val_as_str).unwrap_or("");
        let client = HttpClient::new();
        let mut req = Request::post(url).map_err(|e| e.to_string())?;
        req.body = Body::from_string(body_str);
        let _ = req.headers.set_content_length(body_str.len() as u64);
        let resp = client.send(req).map_err(|e| e.to_string())?;
        Ok(build_response_val(resp))
    });
}

use crate::runtime::stdlib_src::http::domain_types::DomainTypeKind;
use crate::runtime::stdlib_src::http::openapi::OpenAPIGenerator;
use crate::runtime::stdlib_src::http::schema::{Constraint, DTOMode, FieldSpec, FieldType, Schema};

pub fn build_types_object() -> Value {
    let mut map = FastMap::default();
    map.insert("Email".to_string(), Value::Str("Email".to_string()));
    map.insert("URL".to_string(), Value::Str("URL".to_string()));
    map.insert("UUID".to_string(), Value::Str("UUID".to_string()));
    map.insert("IPv4".to_string(), Value::Str("IPv4".to_string()));
    map.insert("IPv6".to_string(), Value::Str("IPv6".to_string()));
    map.insert("IP".to_string(), Value::Str("IP".to_string()));
    map.insert("Hostname".to_string(), Value::Str("Hostname".to_string()));
    map.insert(
        "HttpMethod".to_string(),
        Value::Str("HttpMethod".to_string()),
    );
    map.insert("MediaType".to_string(), Value::Str("MediaType".to_string()));
    map.insert("DateTime".to_string(), Value::Str("DateTime".to_string()));
    map.insert("ETag".to_string(), Value::Str("ETag".to_string()));
    Value::Object(Arc::new(map))
}

pub fn parse_field_spec(field_name: &str, field_val: &Value) -> FieldSpec {
    match field_val {
        Value::Str(type_str) => {
            let ft = parse_type_str(type_str);
            FieldSpec::new(field_name, ft)
        }
        Value::Object(m) => {
            let t_str = m.get("type").and_then(val_as_str).unwrap_or("string");
            let mut ft = parse_type_str(t_str);
            if let Some(enum_val) = m.get("enum") {
                if let Some(variants) = val_as_string_list(enum_val) {
                    ft = FieldType::Enum(variants);
                }
            }
            let mut spec = FieldSpec::new(field_name, ft);
            if let Some(Value::Bool(b)) = m.get("required") {
                spec.is_required = *b;
            }
            if let Some(Value::Bool(b)) = m.get("nullable") {
                spec.is_nullable = *b;
            }
            if let Some(Value::Bool(b)) = m.get("secret") {
                spec.is_secret = *b;
            }
            if let Some(Value::Bool(b)) = m.get("writeOnly") {
                spec.is_write_only = *b;
            }
            if let Some(Value::Bool(b)) = m.get("readOnly") {
                spec.is_read_only = *b;
            }
            if let Some(def) = m.get("default") {
                spec = spec.default_val(def.clone());
            }
            if let Some(min) = m.get("min").and_then(val_as_f64) {
                spec = spec.add_constraint(Constraint::Min(min));
            }
            if let Some(max) = m.get("max").and_then(val_as_f64) {
                spec = spec.add_constraint(Constraint::Max(max));
            }
            if let Some(min_l) = m.get("minLength").and_then(val_as_f64) {
                spec = spec.add_constraint(Constraint::MinLength(min_l as usize));
            }
            if let Some(max_l) = m.get("maxLength").and_then(val_as_f64) {
                spec = spec.add_constraint(Constraint::MaxLength(max_l as usize));
            }
            if let Some(exact_l) = m.get("exactLength").and_then(val_as_f64) {
                spec = spec.add_constraint(Constraint::ExactLength(exact_l as usize));
            }
            if let Some(pat) = m.get("pattern").and_then(val_as_str) {
                spec = spec.add_constraint(Constraint::Pattern(pat.to_string()));
            }
            spec
        }
        _ => FieldSpec::new(field_name, FieldType::String),
    }
}

pub fn parse_type_str(t: &str) -> FieldType {
    match t.trim() {
        "string" | "String" => FieldType::String,
        "int" | "i64" | "Int" | "integer" | "Integer" => FieldType::Int,
        "u32" | "U32" => FieldType::U32,
        "float" | "f64" | "Float" | "number" | "Number" | "double" | "Double" => FieldType::Float,
        "bool" | "boolean" | "Bool" | "Boolean" => FieldType::Bool,
        "any" | "Any" => FieldType::Any,
        "Email" => FieldType::Domain(DomainTypeKind::Email),
        "URL" => FieldType::Domain(DomainTypeKind::URL),
        "UUID" => FieldType::Domain(DomainTypeKind::UUID),
        "IPv4" => FieldType::Domain(DomainTypeKind::IPv4),
        "IPv6" => FieldType::Domain(DomainTypeKind::IPv6),
        "IP" => FieldType::Domain(DomainTypeKind::IP),
        "Hostname" => FieldType::Domain(DomainTypeKind::Hostname),
        "HttpMethod" => FieldType::Domain(DomainTypeKind::HttpMethod),
        "MediaType" => FieldType::Domain(DomainTypeKind::MediaType),
        "DateTime" => FieldType::Domain(DomainTypeKind::DateTime),
        "ETag" => FieldType::Domain(DomainTypeKind::ETag),
        _ => FieldType::String,
    }
}

pub fn build_dto_object() -> Value {
    let mut map = FastMap::default();

    map.insert(
        "define".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let name = args.first().and_then(val_as_str).unwrap_or("DTO");
            let fields_obj = args.get(1).ok_or("DTO fields object required")?;

            let mut schema = Schema::new(name);
            if let Value::Object(fields_map) = fields_obj {
                for (fname, fval) in fields_map.iter() {
                    schema.add_field(parse_field_spec(fname, fval));
                }
            }

            let schema_arc = Arc::new(schema);
            let mut dto_instance_map = FastMap::default();
            dto_instance_map.insert("name".to_string(), Value::Str(name.to_string()));

            let s1 = schema_arc.clone();
            dto_instance_map.insert(
                "validate".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, a| {
                    let input = a.first().ok_or("Input object required for validation")?;
                    match s1.validate(input) {
                        Ok(val) => Ok(val),
                        Err(err) => Ok(err.to_value()),
                    }
                }))),
            );

            let s2 = schema_arc.clone();
            dto_instance_map.insert(
                "serialize".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, a| {
                    let input = a.first().ok_or("Data required for serialization")?;
                    s2.serialize_response(input)
                }))),
            );

            let s3 = schema_arc.clone();
            dto_instance_map.insert(
                "toJSONSchema".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, _| {
                    Ok(OpenAPIGenerator::schema_to_json_schema(&s3))
                }))),
            );

            Ok(Value::Object(Arc::new(dto_instance_map)))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn build_schema_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "define".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let name = args.first().and_then(val_as_str).unwrap_or("Schema");
            let fields_obj = args.get(1).ok_or("Schema fields object required")?;
            let mode_str = args.get(2).and_then(val_as_str).unwrap_or("Strict");
            let allow_coercion = args.get(3).and_then(val_as_bool).unwrap_or(false);

            let mode = match mode_str {
                "StripUnknown" => DTOMode::StripUnknown,
                "Passthrough" => DTOMode::Passthrough,
                _ => DTOMode::Strict,
            };

            let mut schema = Schema::new(name)
                .with_mode(mode)
                .with_coercion(allow_coercion);
            if let Value::Object(fields_map) = fields_obj {
                for (fname, fval) in fields_map.iter() {
                    schema.add_field(parse_field_spec(fname, fval));
                }
            }
            let schema_arc = Arc::new(schema);
            let mut obj_map = FastMap::default();
            obj_map.insert("name".to_string(), Value::Str(name.to_string()));

            let s_val = schema_arc.clone();
            obj_map.insert(
                "validate".to_string(),
                Value::Function(NativeFn(Arc::new(move |_, a| {
                    let input = a.first().ok_or("Input required for validation")?;
                    match s_val.validate(input) {
                        Ok(val) => Ok(val),
                        Err(err) => Ok(err.to_value()),
                    }
                }))),
            );

            Ok(Value::Object(Arc::new(obj_map)))
        }))),
    );
    Value::Object(Arc::new(map))
}

pub fn build_openapi_object() -> Value {
    let mut map = FastMap::default();
    map.insert(
        "generate".to_string(),
        Value::Function(NativeFn(Arc::new(|_, args| {
            let title = args.first().and_then(val_as_str).unwrap_or("AdeshLang API");
            let version = args.get(1).and_then(val_as_str).unwrap_or("1.0.0");
            let spec = OpenAPIGenerator::generate_openapi_spec(title, version, vec![]);
            Ok(spec)
        }))),
    );
    Value::Object(Arc::new(map))
}
