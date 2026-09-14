use super::super::errors::{HttpError, HttpErrorKind};
use super::super::http1::connection::{Http1Connection, ReadWriteSendSync};
use super::super::http2::connection::Http2Connection;
use super::super::pipeline::MiddlewarePipeline;
use super::super::request::Request;
use super::super::response::Response;
use super::super::version::HttpVersionPolicy;
use super::alt_svc::AltSvcCache;
use super::cookie_jar::{Cookie, CookieJar};
use super::dns_racing::happy_eyeballs_connect;
use super::pool::{ConnectionPool, PoolKey};
use super::redirects::{RedirectEngine, RedirectPolicy};
use super::retries::RetryPolicy;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct HttpClient {
    pub version_policy: HttpVersionPolicy,
    pub connect_timeout: Duration,
    pub request_timeout: Duration,
    pub redirect_engine: Arc<RedirectEngine>,
    pub retry_policy: RetryPolicy,
    pub cookie_jar: Arc<CookieJar>,
    pub pool: ConnectionPool,
    pub alt_svc: Arc<AltSvcCache>,
    pub middleware_pipeline: Arc<MiddlewarePipeline>,
}

use once_cell::sync::Lazy;

static GLOBAL_HTTP_CLIENT: Lazy<HttpClient> = Lazy::new(HttpClient::new);

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            version_policy: HttpVersionPolicy::Auto,
            connect_timeout: Duration::from_secs(10),
            request_timeout: Duration::from_secs(30),
            redirect_engine: Arc::new(RedirectEngine::new(RedirectPolicy::Follow(10))),
            retry_policy: RetryPolicy::default(),
            cookie_jar: Arc::new(CookieJar::new()),
            pool: ConnectionPool::new(16, Duration::from_secs(90)),
            alt_svc: Arc::new(AltSvcCache::new()),
            middleware_pipeline: Arc::new(MiddlewarePipeline::new()),
        }
    }

    pub fn shared() -> &'static HttpClient {
        &GLOBAL_HTTP_CLIENT
    }

    pub fn send(&self, mut req: Request) -> Result<Response, HttpError> {
        self.middleware_pipeline.run_before_request(&mut req)?;

        let mut redirect_count = 0;

        loop {
            // Attach cookies
            if let Some(cookie_hdr) = self.cookie_jar.get_header_for_uri(&req.uri) {
                if !req.headers.contains("cookie") {
                    let _ = req.headers.insert("cookie", &cookie_hdr);
                }
            }

            let mut attempt = 0;
            let resp = loop {
                let res = self.execute_single_request(&req);
                if let Some(delay) = self.retry_policy.should_retry(&req, &res, attempt) {
                    std::thread::sleep(delay);
                    attempt += 1;
                    continue;
                }
                match res {
                    Ok(resp) => break resp,
                    Err(err) => {
                        self.middleware_pipeline.run_error(&req, &err);
                        return Err(err);
                    }
                }
            };

            // Store any Set-Cookie headers
            let host_default = req.uri.host.as_deref().unwrap_or("");
            for set_cookie_str in resp.headers.get_all("set-cookie") {
                if let Ok(cookie) = Cookie::parse(set_cookie_str, host_default) {
                    self.cookie_jar.add(cookie);
                }
            }

            // Check redirects
            if let Some(next_req) =
                self.redirect_engine
                    .should_redirect(&req, &resp, redirect_count)?
            {
                req = next_req;
                redirect_count += 1;
                continue;
            }

            let mut final_resp = resp;
            if let Err(err) = self
                .middleware_pipeline
                .run_after_response(&req, &mut final_resp)
            {
                self.middleware_pipeline.run_error(&req, &err);
                return Err(err);
            }

            return Ok(final_resp);
        }
    }

    pub fn query(&self, url: &str, body: &str) -> Result<Response, HttpError> {
        let req = Request::query_with_body(url, body.to_string())?;
        self.send(req)
    }

    pub fn query_json(&self, url: &str, value: &serde_json::Value) -> Result<Response, HttpError> {
        let req = Request::query_json(url, value)?;
        self.send(req)
    }

    pub fn query_stream<F>(&self, url: &str, stream: F) -> Result<Response, HttpError>
    where
        F: Fn() -> Option<Result<Vec<u8>, HttpError>> + Send + Sync + 'static,
    {
        let req = Request::query_stream(url, stream)?;
        self.send(req)
    }

    fn execute_single_request(&self, req: &Request) -> Result<Response, HttpError> {
        let host =
            req.uri.host.as_deref().ok_or_else(|| {
                HttpError::new(HttpErrorKind::InvalidUri, "Target URI missing host")
            })?;
        let port = req.uri.effective_port();
        let is_https = req.uri.is_https();

        let pool_key = PoolKey {
            scheme: req.uri.scheme.clone().unwrap_or_else(|| "http".into()),
            host: host.to_string(),
            port,
            alpn: None,
        };

        // If HTTP/2 is forced or negotiated via ALPN:
        if self.version_policy == HttpVersionPolicy::Http2Only {
            let socket = happy_eyeballs_connect(host, port, self.connect_timeout)?;
            if is_https {
                let mut policy =
                    crate::runtime::stdlib_src::tls::policy::TlsSecurityPolicy::default();
                policy.alpn_protocols = vec![b"h2".to_vec()];
                let trust_store = crate::runtime::stdlib_src::tls::cert::TlsTrustStore::default();
                let tls_stream =
                    crate::runtime::stdlib_src::tls::connection::TlsConnection::wrap_client(
                        socket,
                        host,
                        &policy,
                        &trust_store,
                        None,
                    )
                    .map_err(|e| HttpError::new(HttpErrorKind::TlsError, e.to_string()))?;
                let mut h2_conn = Http2Connection::new(Box::new(tls_stream), true);
                return h2_conn.send_request(req);
            } else {
                let mut h2_conn = Http2Connection::new(Box::new(socket), true);
                return h2_conn.send_request(req);
            }
        }

        // Try reusing pooled connection for HTTP/1.1
        if let Some(mut pooled_conn) = self.pool.get(&pool_key) {
            match pooled_conn.send_request(req) {
                Ok(resp) => {
                    if resp.headers.is_keep_alive() {
                        self.pool.put(pool_key, pooled_conn);
                    }
                    return Ok(resp);
                }
                Err(_) => {
                    // Stale connection reaped; fallback to fresh connection
                }
            }
        }

        // Fresh TCP socket
        let socket = happy_eyeballs_connect(host, port, self.connect_timeout)?;

        let stream: Box<dyn ReadWriteSendSync> = if is_https {
            let policy = crate::runtime::stdlib_src::tls::policy::TlsSecurityPolicy::default();
            let trust_store = crate::runtime::stdlib_src::tls::cert::TlsTrustStore::default();
            let tls_stream =
                crate::runtime::stdlib_src::tls::connection::TlsConnection::wrap_client(
                    socket,
                    host,
                    &policy,
                    &trust_store,
                    None,
                )
                .map_err(|e| HttpError::new(HttpErrorKind::TlsError, e.to_string()))?;
            Box::new(tls_stream)
        } else {
            Box::new(socket)
        };

        let mut conn = Http1Connection::new(stream);
        let resp = conn.send_request(req)?;

        if resp.headers.is_keep_alive() {
            self.pool.put(pool_key, conn);
        }

        Ok(resp)
    }
}
