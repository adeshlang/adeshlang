use super::super::errors::{HttpError, HttpErrorKind};
use super::super::http1::encoder::encode_response;
use super::super::http1::parser::Http1Parser;
use super::super::http2::frames::{HTTP2_PREFACE, Http2Frame};
use super::super::http2::hpack::{HpackDecoder, HpackEncoder};
use super::super::method::HttpMethod;
use super::super::request::Request;
use super::super::response::Response;
use super::super::uri::Uri;
use super::super::version::HttpVersion;
use super::context::RequestContext;
use super::router::Router;
use std::io::{BufWriter, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A parsed incoming request sent from the acceptor thread to the main thread.
pub struct IncomingRequest {
    pub req: Request,
    pub peer_addr: SocketAddr,
    /// Sender to respond back on — zero-copy one-shot channel.
    pub resp_tx: std::sync::mpsc::SyncSender<Response>,
}

pub struct HttpServer {
    pub addr: SocketAddr,
    pub router: Arc<Router>,
    pub running: Arc<AtomicBool>,
    pub listener: Option<TcpListener>,
    pub request_tx: Option<std::sync::mpsc::SyncSender<IncomingRequest>>,
    pub request_rx: Option<std::sync::mpsc::Receiver<IncomingRequest>>,
    pub tls_cert_key: Option<(String, String)>,
}

impl HttpServer {
    pub fn bind(addr_str: &str, router: Router) -> Result<Self, HttpError> {
        // Parse the address — support "localhost:N" as a special shorthand
        let addr_str_resolved = if addr_str.starts_with("localhost:") {
            addr_str.replacen("localhost", "127.0.0.1", 1)
        } else {
            addr_str.to_string()
        };

        let addr: SocketAddr = addr_str_resolved.parse().map_err(|e| {
            HttpError::new(
                HttpErrorKind::InvalidUri,
                format!("Invalid server address: {}", e),
            )
        })?;

        // SO_REUSEADDR: don't wait for TIME_WAIT on restart
        let listener = {
            use std::net::TcpListener;
            // Build socket with SO_REUSEADDR + TCP_NODELAY via socket2 if available,
            // otherwise fall back to stdlib (SO_REUSEADDR is on by default in std)
            TcpListener::bind(addr).map_err(|e| {
                HttpError::new(
                    HttpErrorKind::IoError,
                    format!("Failed to bind {}: {}", addr, e),
                )
            })?
        };

        // Use BLOCKING mode — we will use per-connection threads with blocking I/O.
        // Non-blocking + sleep polling adds 0-5ms latency. Blocking I/O is zero-latency.
        let _ = listener.set_nonblocking(false);

        // Channel capacity: allow up to 1024 outstanding parsed requests
        let (tx, rx) = std::sync::mpsc::sync_channel::<IncomingRequest>(1024);

        Ok(Self {
            addr,
            router: Arc::new(router),
            running: Arc::new(AtomicBool::new(false)),
            listener: Some(listener),
            request_tx: Some(tx),
            request_rx: Some(rx),
            tls_cert_key: None,
        })
    }

    pub fn enable_tls(&mut self, cert_pem: String, key_pem: String) {
        self.tls_cert_key = Some((cert_pem, key_pem));
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.shutdown();
    }

    /// Start the background acceptor thread using the router's built-in handlers.
    /// Each connection gets its own thread — fully concurrent.
    pub fn start_background(&mut self) -> Result<(), HttpError> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| HttpError::new(HttpErrorKind::IoError, "Server already started"))?;
        let running = self.running.clone();
        let router = self.router.clone();

        running.store(true, Ordering::SeqCst);

        std::thread::Builder::new()
            .name("http-acceptor".to_string())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, peer_addr)) => {
                            set_tcp_nodelay(&stream);
                            let router_clone = router.clone();
                            std::thread::Builder::new()
                                .name(format!("http-conn-{}", peer_addr))
                                .spawn(move || {
                                    let mut s = stream;
                                    let _ = handle_connection_router(
                                        &mut s,
                                        peer_addr,
                                        &router_clone,
                                        &Http1Parser::new(),
                                    );
                                })
                                .ok();
                        }
                        Err(ref e) if is_transient_error(e) => continue,
                        Err(_) => break,
                    }
                }
            })
            .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

        Ok(())
    }

    /// Start the background acceptor using a channel — the main thread handles dispatch.
    /// Each connection is handled in its own thread for maximum concurrency.
    /// Returns the receiver end so the main thread can poll and call user callbacks.
    pub fn start_channel_mode(
        &mut self,
    ) -> Result<std::sync::mpsc::Receiver<IncomingRequest>, HttpError> {
        let listener = self
            .listener
            .take()
            .ok_or_else(|| HttpError::new(HttpErrorKind::IoError, "Server already started"))?;
        let running = self.running.clone();
        let tx = self
            .request_tx
            .take()
            .ok_or_else(|| HttpError::new(HttpErrorKind::IoError, "Channel already taken"))?;
        let rx = self
            .request_rx
            .take()
            .ok_or_else(|| HttpError::new(HttpErrorKind::IoError, "Receiver already taken"))?;

        running.store(true, Ordering::SeqCst);

        let udp_addr = self.addr;
        let tls_cert_key = self.tls_cert_key.clone();

        if let Some((ref cert_pem, ref key_pem)) = tls_cert_key {
            spawn_quic_server(
                udp_addr,
                tx.clone(),
                running.clone(),
                cert_pem.clone(),
                key_pem.clone(),
            );
        } else {
            let udp_tx = tx.clone();
            let running_udp = running.clone();
            std::thread::Builder::new()
                .name("http3-udp-listener".to_string())
                .spawn(move || {
                    if let Ok(socket) = std::net::UdpSocket::bind(udp_addr) {
                        let _ = socket.set_read_timeout(Some(Duration::from_millis(500)));
                        let mut buf = [0u8; 65535];
                        while running_udp.load(Ordering::Relaxed) {
                            if let Ok((_amt, src)) = socket.recv_from(&mut buf) {
                                let mut req = Request::new(
                                    super::super::method::HttpMethod::Get,
                                    super::super::uri::Uri::parse("/h3").unwrap(),
                                );
                                req.version = super::super::version::HttpVersion::Http30;
                                let (resp_tx, resp_rx) =
                                    std::sync::mpsc::sync_channel::<Response>(1);
                                let incoming = IncomingRequest {
                                    req,
                                    peer_addr: src,
                                    resp_tx,
                                };
                                if udp_tx.send(incoming).is_ok() {
                                    if let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(5)) {
                                        let mut h3_conn =
                                            super::super::http3::connection::Http3Connection::new(
                                                false,
                                            );
                                        if let Ok(h3_payload) =
                                            h3_conn.encode_response_stream(&resp)
                                        {
                                            let _ = socket.send_to(&h3_payload, src);
                                        }
                                    }
                                }
                            }
                        }
                    }
                })
                .ok();
        }

        std::thread::Builder::new()
            .name("http-acceptor".to_string())
            .spawn(move || {
                let parser = Http1Parser::new();
                while running.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, peer_addr)) => {
                            set_tcp_nodelay(&stream);
                            let tx_clone = tx.clone();
                            let parser_clone = Http1Parser::new();
                            let tls_cert_key_conn = tls_cert_key.clone();

                            std::thread::Builder::new()
                                .name(format!("http-conn-{}", peer_addr))
                                .spawn(move || {
                                    let mut s = stream;
                                    if let Some((cert_pem, key_pem)) = &tls_cert_key_conn {
                                        let mut policy = crate::runtime::stdlib_src::tls::policy::TlsSecurityPolicy::default();
                                        policy.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
                                        if let Ok(mut tls_stream) = crate::runtime::stdlib_src::tls::connection::TlsConnection::wrap_server(s, cert_pem.as_bytes(), key_pem.as_bytes(), &policy) {
                                            let mut buf = [0u8; 8192];
                                            if let Ok(n) = tls_stream.read(&mut buf) {
                                                let bytes = &buf[..n];
                                                if bytes.starts_with(HTTP2_PREFACE) {
                                                    handle_h2_connection(&mut tls_stream, peer_addr, &tx_clone, bytes);
                                                } else if let Ok((req, _)) = parser_clone.parse_request(bytes) {
                                                    let (resp_tx, resp_rx) = std::sync::mpsc::sync_channel::<Response>(1);
                                                    let incoming = IncomingRequest { req, peer_addr, resp_tx };
                                                    if tx_clone.send(incoming).is_ok() {
                                                        if let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(5)) {
                                                            if let Ok(resp_bytes) = encode_response(&resp) {
                                                                let _ = tls_stream.write_all(&resp_bytes);
                                                                let _ = tls_stream.flush();
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    } else {
                                        // Plain HTTP/1.1 & HTTP/2 fallback
                                        if let Ok(bytes) = read_request(&mut s) {
                                            if bytes.starts_with(HTTP2_PREFACE) {
                                                handle_h2_connection(&mut s, peer_addr, &tx_clone, &bytes);
                                            } else if let Ok((req, _)) = parser_clone.parse_request(&bytes) {
                                                let (resp_tx, resp_rx) = std::sync::mpsc::sync_channel::<Response>(1);
                                                let incoming = IncomingRequest { req, peer_addr, resp_tx };
                                                if tx_clone.send(incoming).is_ok() {
                                                    if let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(10)) {
                                                        if let Ok(resp_bytes) = encode_response(&resp) {
                                                            let _ = s.write_all(&resp_bytes);
                                                            let _ = s.flush();
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                })
                                .ok();
                        }
                        Err(ref e) if is_transient_error(e) => continue,
                        Err(_) => break,
                    }
                }
                drop(parser);
            })
            .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;

        Ok(rx)
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Set TCP_NODELAY to disable Nagle's algorithm — eliminates up to 200ms latency
/// on small responses (which is every HTTP response).
#[inline]
fn set_tcp_nodelay(stream: &TcpStream) {
    let _ = stream.set_nodelay(true);
    // 30s read timeout prevents hanging connections from blocking threads
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
}

/// Returns true for transient OS errors that should not terminate the accept loop.
#[inline]
fn is_transient_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
    )
}

/// Read a full HTTP/1.1 request from the stream.
/// Handles the case where the request arrives in multiple TCP segments.
fn read_request(stream: &mut TcpStream) -> Result<Vec<u8>, HttpError> {
    let mut buf = Vec::with_capacity(8192);
    let mut tmp = [0u8; 8192];

    loop {
        match stream.read(&mut tmp) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&tmp[..n]);
                // HTTP/2 prior knowledge preface check
                if buf.starts_with(super::super::http2::frames::HTTP2_PREFACE) {
                    break;
                }
                // HTTP/1.1 request header end check
                if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    let header_end = pos + 4;
                    let mut content_len = 0;
                    let header_str = String::from_utf8_lossy(&buf[..header_end]);
                    for line in header_str.lines() {
                        let lower = line.to_ascii_lowercase();
                        if lower.starts_with("content-length:") {
                            if let Ok(len) =
                                lower["content-length:".len()..].trim().parse::<usize>()
                            {
                                content_len = len;
                            }
                        }
                    }
                    if buf.len() >= header_end + content_len {
                        break;
                    }
                }
                // Stop if we have too much data (prevent DoS)
                if buf.len() > 1_048_576 {
                    break;
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                break;
            }
            Err(e) => {
                return Err(HttpError::new(HttpErrorKind::IoError, e.to_string()));
            }
        }
    }

    if buf.is_empty() {
        Err(HttpError::new(HttpErrorKind::IoError, "empty request"))
    } else {
        Ok(buf)
    }
}

/// Write an HTTP response using a buffered writer for efficiency.
#[inline]
fn write_response(stream: &mut TcpStream, resp: &Response) {
    let mut mut_resp = resp.clone();
    let _ = mut_resp.headers.insert("Access-Control-Allow-Origin", "*");
    let _ = mut_resp.headers.insert(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, PATCH, OPTIONS",
    );
    let _ = mut_resp.headers.insert(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization, X-Requested-With, Accept",
    );
    let _ = mut_resp
        .headers
        .insert("Access-Control-Allow-Credentials", "true");

    if let Ok(wire) = encode_response(&mut_resp) {
        let mut writer = BufWriter::with_capacity(8192, stream);
        let _ = writer.write_all(&wire);
        let _ = writer.flush();
    }
}

fn handle_connection_router(
    stream: &mut TcpStream,
    peer_addr: SocketAddr,
    router: &Router,
    parser: &Http1Parser,
) -> Result<(), HttpError> {
    let bytes = read_request(stream)?;
    if bytes.is_empty() {
        return Ok(());
    }

    let (req, _) = parser.parse_request(&bytes)?;
    let path = req.uri.path.clone();
    let method = req.method.clone();

    let mut ctx = RequestContext::from_request(req, Some(peer_addr));

    let resp = if method == super::super::method::HttpMethod::Options {
        Response::new(super::super::status::HttpStatus::NO_CONTENT)
    } else if let Some((handler, params)) = router.match_route(&method, &path) {
        ctx.params = params;
        handler(&mut ctx).unwrap_or_else(|e| {
            let mut err_resp = Response::server_error();
            err_resp.body = super::super::body::Body::from_string(e.to_string());
            err_resp
        })
    } else {
        let allowed = router.allowed_methods_for_path(&path);
        if !allowed.is_empty() {
            let allow_str = allowed
                .iter()
                .map(|m| m.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let mut resp = Response::new(super::super::status::HttpStatus::METHOD_NOT_ALLOWED);
            let _ = resp.headers.insert("allow", &allow_str);
            resp.body = super::super::body::Body::from_string("405 Method Not Allowed");
            resp
        } else {
            Response::not_found()
        }
    };

    write_response(stream, &resp);
    Ok(())
}

fn handle_h2_connection<S: Read + Write>(
    s: &mut S,
    peer_addr: SocketAddr,
    tx: &std::sync::mpsc::SyncSender<IncomingRequest>,
    initial_bytes: &[u8],
) {
    let server_settings = Http2Frame::Settings {
        ack: false,
        settings: vec![(0x3, 100)],
    };
    let _ = s.write_all(&server_settings.encode());
    let _ = s.flush();

    let mut buf = Vec::from(initial_bytes);
    if buf.starts_with(HTTP2_PREFACE) {
        buf.drain(..HTTP2_PREFACE.len());
    }

    let mut decoder = HpackDecoder::new(4096);
    let mut encoder = HpackEncoder::new(4096);
    let mut tmp = [0u8; 8192];

    loop {
        if buf.is_empty() {
            match s.read(&mut tmp) {
                Ok(n) if n > 0 => buf.extend_from_slice(&tmp[..n]),
                _ => break,
            }
        }
        let mut offset = 0;
        let mut processed = false;
        while offset < buf.len() {
            match Http2Frame::decode(&buf[offset..]) {
                Ok(Some((frame, consumed))) => {
                    offset += consumed;
                    processed = true;
                    match frame {
                        Http2Frame::Settings { ack, .. } => {
                            if !ack {
                                let ack_frame = Http2Frame::Settings {
                                    ack: true,
                                    settings: Vec::new(),
                                };
                                let _ = s.write_all(&ack_frame.encode());
                                let _ = s.flush();
                            }
                        }
                        Http2Frame::Headers {
                            stream_id,
                            header_block_fragment,
                            ..
                        } => {
                            let decoded_headers =
                                decoder.decode(&header_block_fragment).unwrap_or_default();
                            let mut path = "/".to_string();
                            let mut method = HttpMethod::Get;

                            for (k, v) in &decoded_headers {
                                if k == ":path" {
                                    path = v.clone();
                                } else if k == ":method" {
                                    if let Ok(m) = HttpMethod::parse(v) {
                                        method = m;
                                    }
                                }
                            }

                            let mut req = Request::new(
                                method,
                                Uri::parse(&path).unwrap_or_else(|_| Uri::parse("/").unwrap()),
                            );
                            req.version = HttpVersion::Http20;

                            let (resp_tx, resp_rx) = std::sync::mpsc::sync_channel::<Response>(1);
                            let incoming = IncomingRequest {
                                req,
                                peer_addr,
                                resp_tx,
                            };
                            if tx.send(incoming).is_ok() {
                                if let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(10)) {
                                    let status_str = resp.status.code().to_string();
                                    let resp_headers = vec![
                                        (":status", status_str.as_str()),
                                        ("content-type", "application/json"),
                                    ];
                                    let encoded_h = encoder.encode(&resp_headers);

                                    let h_frame = Http2Frame::Headers {
                                        stream_id,
                                        end_stream: false,
                                        end_headers: true,
                                        header_block_fragment: encoded_h,
                                    };
                                    let body_bytes = resp.body.to_bytes().unwrap_or_default();
                                    let d_frame = Http2Frame::Data {
                                        stream_id,
                                        end_stream: true,
                                        data: body_bytes,
                                    };
                                    let _ = s.write_all(&h_frame.encode());
                                    let _ = s.write_all(&d_frame.encode());
                                    let _ = s.flush();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => break,
            }
        }
        if offset > 0 {
            buf.drain(..offset);
        } else if !processed {
            match s.read(&mut tmp) {
                Ok(n) if n > 0 => buf.extend_from_slice(&tmp[..n]),
                _ => break,
            }
        }
    }
}

fn parse_cert_key_pem(
    cert_pem: &str,
    key_pem: &str,
) -> Result<
    (
        Vec<rustls_pki_types::CertificateDer<'static>>,
        rustls_pki_types::PrivateKeyDer<'static>,
    ),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let mut cert_reader = std::io::BufReader::new(cert_pem.as_bytes());
    let certs: Vec<rustls_pki_types::CertificateDer<'static>> =
        rustls_pemfile::certs(&mut cert_reader).collect::<Result<Vec<_>, _>>()?;

    let mut key_reader = std::io::BufReader::new(key_pem.as_bytes());
    let key = rustls_pemfile::private_key(&mut key_reader)?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No private key found"))?;

    Ok((certs, key))
}

fn create_reuse_udp_socket(
    addr: SocketAddr,
) -> Result<std::net::UdpSocket, Box<dyn std::error::Error + Send + Sync>> {
    let domain = if addr.is_ipv4() {
        socket2::Domain::IPV4
    } else {
        socket2::Domain::IPV6
    };
    let socket = socket2::Socket::new(domain, socket2::Type::DGRAM, Some(socket2::Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.set_nonblocking(true)?;
    Ok(socket.into())
}

fn spawn_quic_server(
    udp_addr: SocketAddr,
    tx: std::sync::mpsc::SyncSender<IncomingRequest>,
    running: Arc<AtomicBool>,
    cert_pem: String,
    key_pem: String,
) {
    std::thread::Builder::new()
        .name("quic-h3-listener".to_string())
        .spawn(move || {
            if let Ok((certs, key)) = parse_cert_key_pem(&cert_pem, &key_pem) {
                let mut tls_config = match rustls::ServerConfig::builder()
                    .with_no_client_auth()
                    .with_single_cert(certs, key)
                {
                    Ok(c) => c,
                    Err(_) => return,
                };
                tls_config.alpn_protocols = vec![b"h3".to_vec()];

                let quic_crypto = match quinn::crypto::rustls::QuicServerConfig::try_from(tls_config) {
                    Ok(qc) => qc,
                    Err(_) => return,
                };

                let mut transport_config = quinn::TransportConfig::default();
                if let Ok(timeout) = std::time::Duration::from_secs(10).try_into() {
                    transport_config.max_idle_timeout(Some(timeout));
                }
                transport_config.max_concurrent_bidi_streams(1000u32.into());

                let mut server_config = quinn::ServerConfig::with_crypto(std::sync::Arc::new(quic_crypto));
                server_config.transport_config(std::sync::Arc::new(transport_config));

                let rt = match tokio::runtime::Builder::new_multi_thread()
                    .enable_all()
                    .build()
                {
                    Ok(r) => r,
                    Err(_) => return,
                };

                let _guard = rt.enter();

                let std_socket = match create_reuse_udp_socket(udp_addr) {
                    Ok(s) => s,
                    Err(_) => return,
                };

                let endpoint = match quinn::Endpoint::new(quinn::EndpointConfig::default(), Some(server_config), std_socket, std::sync::Arc::new(quinn::TokioRuntime)) {
                    Ok(ep) => ep,
                    Err(_) => return,
                };

                static NEXT_H3_CONN_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

                rt.block_on(async move {
                    while running.load(Ordering::Relaxed) {
                        if let Some(incoming_conn) = endpoint.accept().await {
                            let tx_clone = tx.clone();
                            tokio::spawn(async move {
                                let conn_id = NEXT_H3_CONN_ID.fetch_add(1, Ordering::SeqCst);
                                if let Ok(conn) = incoming_conn.await {
                                    super::super::http3::diagnostics::on_connection_created(conn_id);
                                    let peer = conn.remote_address();
                                    while let Ok((mut send, mut recv)) = conn.accept_bi().await {
                                        let tx_stream = tx_clone.clone();
                                        let stream_id: u64 = send.id().into();
                                        super::super::http3::diagnostics::on_stream_opened(conn_id, stream_id);
                                        tokio::spawn(async move {
                                            let mut req_buf = Vec::new();
                                            let mut chunk = [0u8; 4096];
                                            let mut read_ok = true;
                                            loop {
                                                match recv.read(&mut chunk).await {
                                                    Ok(Some(n)) if n > 0 => {
                                                        req_buf.extend_from_slice(&chunk[..n]);
                                                        super::super::http3::diagnostics::add_bytes_received(n);
                                                        if req_buf.len() > 10_000_000 {
                                                            read_ok = false;
                                                            break;
                                                        }
                                                    }
                                                    Ok(Some(_)) => {}
                                                    Ok(None) => break,
                                                    Err(_) => {
                                                        read_ok = false;
                                                        break;
                                                    }
                                                }
                                            }

                                            if !read_ok {
                                                super::super::http3::diagnostics::on_stream_reset(stream_id);
                                                let _ = send.reset(quinn::VarInt::from_u32(0x010d));
                                                return;
                                            }

                                            let mut h3_conn = super::super::http3::connection::Http3Connection::new(false);
                                            h3_conn.active_streams.insert(stream_id, super::super::http3::streams::Http3Stream::new(stream_id));

                                            let (req, is_head) = match parse_h3_wire_request(&mut h3_conn, stream_id, &req_buf, peer) {
                                                Ok(r) => r,
                                                Err(err_code) => {
                                                    super::super::http3::diagnostics::on_stream_error(stream_id, "Parse wire request error");
                                                    super::super::http3::diagnostics::on_stream_reset(stream_id);
                                                    let _ = send.reset(quinn::VarInt::from_u32(err_code.code() as u32));
                                                    return;
                                                }
                                            };

                                            let (resp_tx, resp_rx) = std::sync::mpsc::sync_channel::<Response>(1);
                                            let incoming = IncomingRequest {
                                                req,
                                                peer_addr: peer,
                                                resp_tx,
                                            };

                                            if tx_stream.send(incoming).is_ok() {
                                                if let Ok(resp) = resp_rx.recv_timeout(Duration::from_secs(15)) {
                                                    if let Ok(payload) = h3_conn.encode_full_response(stream_id, &resp, is_head) {
                                                        if send.write_all(&payload).await.is_ok() {
                                                            super::super::http3::diagnostics::add_bytes_sent(payload.len());
                                                            let _ = send.finish();
                                                            let _ = send.stopped().await;
                                                            super::super::http3::diagnostics::on_stream_completed(stream_id);
                                                        } else {
                                                            super::super::http3::diagnostics::on_stream_reset(stream_id);
                                                            if let Some(st) = h3_conn.active_streams.get(&stream_id) {
                                                                if st.can_reset() {
                                                                    let _ = send.reset(quinn::VarInt::from_u32(0x0102));
                                                                }
                                                            }
                                                        }
                                                    } else {
                                                        super::super::http3::diagnostics::on_stream_reset(stream_id);
                                                        if let Some(st) = h3_conn.active_streams.get(&stream_id) {
                                                            if st.can_reset() {
                                                                let _ = send.reset(quinn::VarInt::from_u32(0x0102));
                                                            }
                                                        }
                                                    }
                                                } else {
                                                    super::super::http3::diagnostics::on_stream_reset(stream_id);
                                                    if let Some(st) = h3_conn.active_streams.get(&stream_id) {
                                                        if st.can_reset() {
                                                            let _ = send.reset(quinn::VarInt::from_u32(0x010c));
                                                        }
                                                    }
                                                }
                                            }
                                        });
                                    }
                                    super::super::http3::diagnostics::on_connection_closed(conn_id);
                                } else {
                                    super::super::http3::diagnostics::on_connection_error(conn_id, "Connection failed");
                                }
                            });
                        }
                    }
                });
            }
        })
        .ok();
}

fn parse_h3_wire_request(
    h3_conn: &mut super::super::http3::connection::Http3Connection,
    stream_id: u64,
    wire: &[u8],
    peer: SocketAddr,
) -> Result<(Request, bool), super::super::http3::streams::Http3ErrorCode> {
    if wire.is_empty() {
        let mut req = Request::new(
            super::super::method::HttpMethod::Get,
            super::super::uri::Uri::parse("/h3").unwrap(),
        );
        req.version = super::super::version::HttpVersion::Http30;
        return Ok((req, false));
    }

    let _ = h3_conn.process_incoming_stream_data(stream_id, wire);
    if let Some(stream) = h3_conn.active_streams.get(&stream_id) {
        let mut method = super::super::method::HttpMethod::Get;
        let mut path = "/".to_string();
        let mut scheme = "https".to_string();
        let mut authority = peer.to_string();
        let mut headers = super::super::headers::Headers::new();

        for (name, val) in &stream.incoming_headers {
            if name == ":method" {
                if let Ok(m) = super::super::method::HttpMethod::parse(val) {
                    method = m;
                }
            } else if name == ":path" {
                path = val.clone();
            } else if name == ":scheme" {
                scheme = val.clone();
            } else if name == ":authority" {
                authority = val.clone();
            } else if !name.starts_with(':') {
                let _ = headers.append(name, val);
            }
        }

        let is_head = method == super::super::method::HttpMethod::Head;
        let uri_str = format!("{}://{}{}", scheme, authority, path);
        let uri = super::super::uri::Uri::parse(&uri_str).unwrap_or_else(|_| {
            super::super::uri::Uri::parse(&path)
                .unwrap_or_else(|_| super::super::uri::Uri::parse("/").unwrap())
        });

        let mut req = Request::new(method, uri);
        req.version = super::super::version::HttpVersion::Http30;
        req.headers = headers;
        req.body = super::super::body::Body::from_bytes(stream.incoming_data.clone());

        Ok((req, is_head))
    } else {
        Err(super::super::http3::streams::Http3ErrorCode::MessageError)
    }
}
