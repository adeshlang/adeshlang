use super::super::errors::{HttpError, HttpErrorKind};
use super::super::http1::encoder::encode_request;
use super::super::http1::parser::Http1Parser;
use super::super::request::Request;
use super::connection::{WebSocketMessage, WebSocketStream};
use super::handshake::{
    encode_switching_protocols, generate_websocket_key, handle_server_handshake_with_protocol,
    negotiate_subprotocol, validate_client_upgrade_response, validate_origin,
};
use crate::runtime::stdlib_src::tls::cert::TlsTrustStore;
use crate::runtime::stdlib_src::tls::connection::TlsConnection;
use crate::runtime::stdlib_src::tls::policy::TlsSecurityPolicy;
use crate::runtime::stdlib_src::url::url_object::URL;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::time::Duration;

pub const STATE_CONNECTING: i32 = 0;
pub const STATE_HANDSHAKING: i32 = 1;
pub const STATE_OPEN: i32 = 2;
pub const STATE_CLOSING: i32 = 3;
pub const STATE_CLOSED: i32 = 4;
pub const STATE_FAILED: i32 = 5;

#[derive(Clone, Debug)]
pub struct WsClientOptions {
    pub headers: HashMap<String, String>,
    pub protocols: Vec<String>,
    pub origin: Option<String>,
    pub proxy_url: Option<String>,
    pub connect_timeout: Duration,
    pub handshake_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub max_message_size: usize,
    pub max_frame_size: usize,
    pub automatic_pong: bool,
    pub compression: bool,
    pub verify_tls: bool,
    pub tls_ca_pem: Option<Vec<u8>>,
}

impl Default for WsClientOptions {
    fn default() -> Self {
        Self {
            headers: HashMap::new(),
            protocols: Vec::new(),
            origin: None,
            proxy_url: None,
            connect_timeout: Duration::from_millis(10_000),
            handshake_timeout: Duration::from_millis(10_000),
            idle_timeout: Some(Duration::from_millis(60_000)),
            max_message_size: 16 * 1024 * 1024,
            max_frame_size: 4 * 1024 * 1024,
            automatic_pong: true,
            compression: false,
            verify_tls: true,
            tls_ca_pem: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct WsServerOptions {
    pub bind_address: String,
    pub port: u16,
    pub max_message_size: usize,
    pub max_frame_size: usize,
    pub handshake_timeout: Duration,
    pub idle_timeout: Option<Duration>,
    pub automatic_pong: bool,
    pub compression: bool,
    pub protocols: Vec<String>,
    pub origins: Vec<String>,
    pub tls_cert_pem: Option<Vec<u8>>,
    pub tls_key_pem: Option<Vec<u8>>,
}

impl Default for WsServerOptions {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1".to_string(),
            port: 8099,
            max_message_size: 16 * 1024 * 1024,
            max_frame_size: 4 * 1024 * 1024,
            handshake_timeout: Duration::from_millis(5_000),
            idle_timeout: Some(Duration::from_millis(60_000)),
            automatic_pong: true,
            compression: false,
            protocols: Vec::new(),
            origins: Vec::new(),
            tls_cert_pem: None,
            tls_key_pem: None,
        }
    }
}

enum WsIo {
    Tcp(TcpStream),
    Tls(TlsConnection),
}

impl Read for WsIo {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            WsIo::Tcp(stream) => Read::read(stream, buf),
            WsIo::Tls(stream) => Read::read(stream, buf),
        }
    }
}

impl Write for WsIo {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            WsIo::Tcp(stream) => Write::write(stream, buf),
            WsIo::Tls(stream) => Write::write(stream, buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            WsIo::Tcp(stream) => Write::flush(stream),
            WsIo::Tls(stream) => Write::flush(stream),
        }
    }
}

pub struct WsSession {
    pub stream: WebSocketStream,
    pub state: i32,
    pub subprotocol: Option<String>,
    pub is_client: bool,
}

impl WsSession {
    pub fn send_text(&mut self, text: &str) -> Result<(), HttpError> {
        self.ensure_open_for_send()?;
        self.stream.send_text(text)
    }

    pub fn send_binary(&mut self, data: Vec<u8>) -> Result<(), HttpError> {
        self.ensure_open_for_send()?;
        self.stream.send_binary(data)
    }

    pub fn send_ping(&mut self, payload: Vec<u8>) -> Result<(), HttpError> {
        self.ensure_open_for_send()?;
        self.stream.send_ping(payload)
    }

    pub fn send_pong(&mut self, payload: Vec<u8>) -> Result<(), HttpError> {
        self.ensure_open_for_send()?;
        self.stream.send_pong(payload)
    }

    pub fn close(&mut self, code: u16, reason: &str) -> Result<(), HttpError> {
        if self.state == STATE_CLOSED || self.state == STATE_CLOSING {
            return Ok(());
        }
        self.state = STATE_CLOSING;
        let result = self.stream.close(code, reason);
        self.state = STATE_CLOSED;
        result
    }

    pub fn receive(&mut self) -> Result<WebSocketMessage, HttpError> {
        if self.state != STATE_OPEN && self.state != STATE_CLOSING {
            return Ok(WebSocketMessage::Close {
                code: 1000,
                reason: "Connection closed".to_string(),
            });
        }
        match self.stream.receive_message() {
            Ok(WebSocketMessage::Close { code, reason }) => {
                self.state = STATE_CLOSED;
                Ok(WebSocketMessage::Close { code, reason })
            }
            Ok(msg) => Ok(msg),
            Err(err) => {
                self.state = STATE_FAILED;
                Err(err)
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.state == STATE_OPEN
    }

    fn ensure_open_for_send(&self) -> Result<(), HttpError> {
        if self.state != STATE_OPEN {
            return Err(HttpError::new(
                HttpErrorKind::ConnectError,
                "Cannot send message on non-open connection",
            ));
        }
        Ok(())
    }
}

pub fn connect_uri(uri: &str, options: &WsClientOptions) -> Result<WsSession, HttpError> {
    let url = URL::parse(uri).map_err(|e| HttpError::new(HttpErrorKind::InvalidUri, e))?;
    if url.scheme() != "ws" && url.scheme() != "wss" {
        return Err(HttpError::new(
            HttpErrorKind::InvalidUri,
            "Invalid WebSocket scheme, expected ws:// or wss://",
        ));
    }

    let host = url.host().unwrap_or("").to_string();
    if host.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::InvalidUri,
            "WebSocket URL is missing a host",
        ));
    }
    let port = url.effective_port().unwrap_or(if url.scheme() == "wss" {
        443
    } else {
        80
    });
    let tcp = if let Some(ref proxy_uri) = options.proxy_url {
        let p_url = URL::parse(proxy_uri).map_err(|e| {
            HttpError::new(HttpErrorKind::InvalidUri, format!("Invalid proxy_url: {e}"))
        })?;
        let p_host = p_url.host().unwrap_or("").to_string();
        let p_port = p_url.effective_port().unwrap_or(8080);
        let mut p_tcp = tcp_connect_host(&p_host, p_port, options.connect_timeout)?;
        apply_socket_options(&p_tcp, options.handshake_timeout, options.idle_timeout)?;

        let connect_req = format!(
            "CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\nUser-Agent: AdeshLang-WebSocket\r\n\r\n"
        );
        p_tcp.write_all(connect_req.as_bytes()).map_err(io_err)?;
        p_tcp.flush().map_err(io_err)?;

        let mut resp_buf = [0u8; 1024];
        let n = p_tcp.read(&mut resp_buf).map_err(io_err)?;
        let resp_str = String::from_utf8_lossy(&resp_buf[..n]);
        if !resp_str.contains("200") {
            return Err(HttpError::new(
                HttpErrorKind::ConnectError,
                format!("Proxy CONNECT tunneling failed: {resp_str}"),
            ));
        }
        p_tcp
    } else {
        let tcp = tcp_connect_host(&host, port, options.connect_timeout)?;
        apply_socket_options(&tcp, options.handshake_timeout, options.idle_timeout)?;
        tcp
    };

    let io = if url.scheme() == "wss" {
        let mut policy = TlsSecurityPolicy::default();
        policy.verify_certificates = options.verify_tls;
        policy.verify_hostname = options.verify_tls;
        if let Some(ca) = &options.tls_ca_pem {
            policy.custom_ca_pems.push(ca.clone());
        }
        let tls = TlsConnection::wrap_client(
            tcp,
            &host,
            &policy,
            &TlsTrustStore::default(),
            None,
        )
        .map_err(|e| HttpError::new(HttpErrorKind::TlsError, e.to_string()))?;
        WsIo::Tls(tls)
    } else {
        WsIo::Tcp(tcp)
    };

    client_handshake(io, &url, None, options)
}

pub fn handshake_existing(
    io: WsIoOwned,
    uri: &str,
    client_key: Option<&str>,
    options: &WsClientOptions,
) -> Result<WsSession, HttpError> {
    let url = URL::parse(uri).map_err(|e| HttpError::new(HttpErrorKind::InvalidUri, e))?;
    client_handshake(io.into_io(), &url, client_key, options)
}

pub enum WsIoOwned {
    Tcp(TcpStream),
    Tls(TlsConnection),
}

impl WsIoOwned {
    fn into_io(self) -> WsIo {
        match self {
            WsIoOwned::Tcp(stream) => WsIo::Tcp(stream),
            WsIoOwned::Tls(stream) => WsIo::Tls(stream),
        }
    }
}

pub fn bind_listener(options: &WsServerOptions) -> Result<TcpListener, HttpError> {
    let addr = socket_addr(&options.bind_address, options.port);
    let listener = TcpListener::bind(&addr).map_err(|e| {
        HttpError::new(
            HttpErrorKind::ConnectError,
            format!("Failed to listen on {addr}: {e}"),
        )
    })?;
    Ok(listener)
}

pub fn accept_session(
    listener: &TcpListener,
    options: &WsServerOptions,
) -> Result<WsSession, HttpError> {
    let (tcp, _) = listener.accept().map_err(|e| {
        HttpError::new(
            HttpErrorKind::ConnectError,
            format!("WebSocket accept failed: {e}"),
        )
    })?;
    let _ = tcp.set_nonblocking(false);
    apply_socket_options(&tcp, options.handshake_timeout, options.idle_timeout)?;
    let io = if let (Some(cert), Some(key)) = (&options.tls_cert_pem, &options.tls_key_pem) {
        let tls = TlsConnection::wrap_server(tcp, cert, key, &TlsSecurityPolicy::default())
            .map_err(|e| HttpError::new(HttpErrorKind::TlsError, e.to_string()))?;
        WsIo::Tls(tls)
    } else {
        WsIo::Tcp(tcp)
    };
    server_handshake(io, options)
}

pub fn echo_until_close(session: &mut WsSession) -> Result<(), HttpError> {
    loop {
        match session.receive()? {
            WebSocketMessage::Text(text) => {
                session.send_text(&text)?;
            }
            WebSocketMessage::Binary(data) => {
                session.send_binary(data)?;
            }
            WebSocketMessage::Ping(_) | WebSocketMessage::Pong(_) => {}
            WebSocketMessage::Close { .. } => return Ok(()),
        }
    }
}

fn client_handshake(
    mut io: WsIo,
    url: &URL,
    client_key: Option<&str>,
    options: &WsClientOptions,
) -> Result<WsSession, HttpError> {
    let key = client_key
        .map(ToString::to_string)
        .unwrap_or_else(generate_websocket_key);
    let mut req = Request::get(&http_uri_from_ws(url))?;
    req.headers.insert("upgrade", "websocket")?;
    req.headers.insert("connection", "Upgrade")?;
    req.headers.insert("sec-websocket-key", &key)?;
    req.headers.insert("sec-websocket-version", "13")?;
    if let Some(origin) = &options.origin {
        req.headers.insert("origin", origin)?;
    }
    if !options.protocols.is_empty() {
        req.headers
            .insert("sec-websocket-protocol", &options.protocols.join(", "))?;
    }
    for (name, value) in &options.headers {
        let lower = name.to_ascii_lowercase();
        if lower == "host"
            || lower == "upgrade"
            || lower == "connection"
            || lower == "sec-websocket-key"
            || lower == "sec-websocket-version"
        {
            continue;
        }
        req.headers.insert(name, value)?;
    }

    let wire = encode_request(&req)?;
    io.write_all(&wire).map_err(io_err)?;
    io.flush().map_err(io_err)?;

    let buf = read_until_headers(&mut io)?;
    let parser = Http1Parser::new();
    let (resp, consumed) = parser.parse_response(&buf)?;
    let leftover = buf[consumed..].to_vec();
    let protocol = validate_client_upgrade_response(
        resp.status,
        &resp.headers,
        &key,
        &options.protocols,
    )?;

    Ok(finish_session(
        io,
        leftover,
        true,
        protocol,
        options.max_frame_size,
        options.max_message_size,
        options.automatic_pong,
        options.compression,
    ))
}

fn server_handshake(mut io: WsIo, options: &WsServerOptions) -> Result<WsSession, HttpError> {
    let buf = read_until_headers(&mut io)?;
    let parser = Http1Parser::new();
    let (req, consumed) = parser.parse_request(&buf)?;
    let leftover = buf[consumed..].to_vec();

    validate_origin(&req, &options.origins)?;
    let protocol = negotiate_subprotocol(&req, &options.protocols);
    let resp = handle_server_handshake_with_protocol(&req, protocol.as_deref())?;
    let wire = encode_switching_protocols(&resp);
    io.write_all(&wire).map_err(io_err)?;
    io.flush().map_err(io_err)?;

    Ok(finish_session(
        io,
        leftover,
        false,
        protocol,
        options.max_frame_size,
        options.max_message_size,
        options.automatic_pong,
        options.compression,
    ))
}

fn finish_session(
    io: WsIo,
    leftover: Vec<u8>,
    is_client: bool,
    subprotocol: Option<String>,
    max_frame_size: usize,
    max_message_size: usize,
    automatic_pong: bool,
    compression: bool,
) -> WsSession {
    let mut stream = WebSocketStream::with_limits(
        Box::new(io),
        is_client,
        max_frame_size,
        max_message_size,
    );
    stream.read_buffer = leftover;
    stream.automatic_pong = automatic_pong;
    stream.compression_enabled = compression;
    WsSession {
        stream,
        state: STATE_OPEN,
        subprotocol,
        is_client,
    }
}

fn read_until_headers<S: Read>(stream: &mut S) -> Result<Vec<u8>, HttpError> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        if find_double_crlf(&buf).is_some() {
            return Ok(buf);
        }
        if buf.len() > 64 * 1024 {
            return Err(HttpError::new(
                HttpErrorKind::HeaderTooLarge,
                "WebSocket handshake headers exceed 64 KiB",
            ));
        }
        let n = stream.read(&mut tmp).map_err(io_err)?;
        if n == 0 {
            return Err(HttpError::new(
                HttpErrorKind::ConnectError,
                "Connection closed during WebSocket handshake",
            ));
        }
        buf.extend_from_slice(&tmp[..n]);
    }
}

fn find_double_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|window| window == b"\r\n\r\n")
}

fn tcp_connect_host(host: &str, port: u16, timeout: Duration) -> Result<TcpStream, HttpError> {
    let addr = socket_addr(host, port);
    let addrs = addr.to_socket_addrs().map_err(|e| {
        HttpError::new(
            HttpErrorKind::DnsError,
            format!("Host resolution failed for {addr}: {e}"),
        )
    })?;
    let mut last_err = None;
    for socket_addr in addrs {
        match TcpStream::connect_timeout(&socket_addr, timeout) {
            Ok(stream) => return Ok(stream),
            Err(e) => last_err = Some(e),
        }
    }
    Err(HttpError::new(
        HttpErrorKind::ConnectError,
        format!(
            "Failed to connect to {host}:{port}: {}",
            last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "no addresses".to_string())
        ),
    ))
}

fn apply_socket_options(
    stream: &TcpStream,
    handshake_timeout: Duration,
    idle_timeout: Option<Duration>,
) -> Result<(), HttpError> {
    stream
        .set_nodelay(true)
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;
    let timeout = idle_timeout.or(Some(handshake_timeout));
    stream
        .set_read_timeout(timeout)
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;
    stream
        .set_write_timeout(Some(handshake_timeout))
        .map_err(|e| HttpError::new(HttpErrorKind::IoError, e.to_string()))?;
    Ok(())
}

fn socket_addr(host: &str, port: u16) -> String {
    let clean_host = host.trim_start_matches('[').trim_end_matches(']');
    if clean_host.contains(':') {
        format!("[{clean_host}]:{port}")
    } else {
        format!("{clean_host}:{port}")
    }
}

fn http_uri_from_ws(url: &URL) -> String {
    let scheme = if url.scheme() == "wss" {
        "https"
    } else {
        "http"
    };
    let host = url.host().unwrap_or("localhost");
    let mut out = if host.contains(':') && !host.starts_with('[') {
        format!("{scheme}://[{host}]")
    } else {
        format!("{scheme}://{host}")
    };
    if let Some(port) = url.port() {
        out.push(':');
        out.push_str(&port.to_string());
    } else if let Some(port) = url.effective_port() {
        if Some(port) != url.default_port() {
            out.push(':');
            out.push_str(&port.to_string());
        }
    }
    let path = url.path();
    if path.is_empty() {
        out.push('/');
    } else {
        if !path.starts_with('/') {
            out.push('/');
        }
        out.push_str(path);
    }
    let query = url.query();
    if !query.is_empty() {
        out.push('?');
        out.push_str(query.trim_start_matches('?'));
    }
    out
}

fn io_err(e: std::io::Error) -> HttpError {
    HttpError::new(HttpErrorKind::IoError, format!("WebSocket I/O error: {e}"))
}
