use super::super::errors::{HttpError, HttpErrorKind};
use super::super::headers::Headers;
use super::super::method::HttpMethod;
use super::super::request::Request;
use super::super::response::Response;
use super::super::status::HttpStatus;
use base64::Engine;

pub const WS_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
const WEBSOCKET_VERSION: &str = "13";

pub fn generate_websocket_key() -> String {
    let rand_bytes: [u8; 16] = rand::random();
    base64::engine::general_purpose::STANDARD.encode(rand_bytes)
}

pub fn calculate_websocket_accept(key: &str) -> String {
    let combined = format!("{}{}", key.trim(), WS_GUID);
    let hash = sha1_digest(combined.as_bytes());
    base64::engine::general_purpose::STANDARD.encode(hash)
}

pub fn handle_server_handshake(req: &Request) -> Result<Response, HttpError> {
    handle_server_handshake_with_protocol(req, None)
}

pub fn handle_server_handshake_with_protocol(
    req: &Request,
    protocol: Option<&str>,
) -> Result<Response, HttpError> {
    let key = validate_server_upgrade_request(req)?;
    let accept = calculate_websocket_accept(&key);

    let mut resp = Response::new(HttpStatus::SWITCHING_PROTOCOLS);
    resp.headers.insert("upgrade", "websocket")?;
    resp.headers.insert("connection", "Upgrade")?;
    resp.headers.insert("sec-websocket-accept", &accept)?;
    if let Some(protocol) = protocol {
        resp.headers.insert("sec-websocket-protocol", protocol)?;
    }

    Ok(resp)
}

pub fn requested_subprotocols(req: &Request) -> Vec<String> {
    req.headers
        .get("sec-websocket-protocol")
        .map(|raw| {
            raw.split(',')
                .map(|token| token.trim().to_string())
                .filter(|token| !token.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

pub fn negotiate_subprotocol(req: &Request, offered: &[String]) -> Option<String> {
    if offered.is_empty() {
        return None;
    }
    requested_subprotocols(req)
        .into_iter()
        .find(|requested| offered.iter().any(|offered| offered == requested))
}

pub fn validate_origin(req: &Request, allowed: &[String]) -> Result<(), HttpError> {
    if allowed.is_empty() {
        return Ok(());
    }
    let origin = req.headers.get("origin").unwrap_or("").trim();
    if origin.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            "Missing Origin header required by WebSocket server policy",
        )
        .with_status(HttpStatus::FORBIDDEN.code()));
    }
    if allowed.iter().any(|allowed| allowed == origin) {
        Ok(())
    } else {
        Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            format!("WebSocket origin {origin} is not allowed"),
        )
        .with_status(HttpStatus::FORBIDDEN.code()))
    }
}

pub fn encode_switching_protocols(resp: &Response) -> Vec<u8> {
    let mut buf = format!("{} {}\r\n", resp.version, resp.status).into_bytes();
    for (name, values) in resp.headers.iter() {
        for value in values {
            buf.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
        }
    }
    buf.extend_from_slice(b"\r\n");
    buf
}

pub fn validate_server_upgrade_request(req: &Request) -> Result<String, HttpError> {
    if req.method != HttpMethod::Get {
        return Err(HttpError::new(
            HttpErrorKind::ProtocolError,
            "WebSocket upgrade requires HTTP GET",
        )
        .with_status(HttpStatus::METHOD_NOT_ALLOWED.code()));
    }

    ensure_header_token(&req.headers, "connection", "upgrade")?;
    ensure_header_token(&req.headers, "upgrade", "websocket")?;

    let version = req.headers.get("sec-websocket-version").ok_or_else(|| {
        HttpError::new(
            HttpErrorKind::InvalidHeader,
            "Missing Sec-WebSocket-Version in WebSocket upgrade request",
        )
        .with_status(HttpStatus::UPGRADE_REQUIRED.code())
    })?;
    if version.trim() != WEBSOCKET_VERSION {
        return Err(HttpError::new(
            HttpErrorKind::ProtocolError,
            format!("Unsupported WebSocket version: {}", version.trim()),
        )
        .with_status(HttpStatus::UPGRADE_REQUIRED.code()));
    }

    if !req.headers.contains("host") {
        return Err(HttpError::new(
            HttpErrorKind::InvalidHeader,
            "Missing Host header in WebSocket upgrade request",
        )
        .with_status(HttpStatus::BAD_REQUEST.code()));
    }

    let key = req.headers.get("sec-websocket-key").ok_or_else(|| {
        HttpError::new(
            HttpErrorKind::InvalidHeader,
            "Missing Sec-WebSocket-Key in WebSocket upgrade request",
        )
        .with_status(HttpStatus::BAD_REQUEST.code())
    })?;
    validate_websocket_key(key)?;

    Ok(key.trim().to_string())
}

pub fn validate_client_upgrade_response(
    status: HttpStatus,
    headers: &Headers,
    expected_key: &str,
    requested_protocols: &[String],
) -> Result<Option<String>, HttpError> {
    if status != HttpStatus::SWITCHING_PROTOCOLS {
        return Err(HttpError::new(
            HttpErrorKind::ProtocolError,
            format!("Expected HTTP 101 Switching Protocols, received {}", status),
        )
        .with_status(status.code()));
    }

    ensure_header_token(headers, "connection", "upgrade")?;
    ensure_header_token(headers, "upgrade", "websocket")?;

    let accept = headers.get("sec-websocket-accept").ok_or_else(|| {
        HttpError::new(
            HttpErrorKind::InvalidHeader,
            "Missing Sec-WebSocket-Accept in WebSocket upgrade response",
        )
    })?;
    let expected_accept = calculate_websocket_accept(expected_key);
    if accept.trim() != expected_accept {
        return Err(HttpError::new(
            HttpErrorKind::ProtocolError,
            "Sec-WebSocket-Accept mismatch in WebSocket upgrade response",
        ));
    }

    let negotiated_protocol = headers.get("sec-websocket-protocol").map(str::trim);
    match negotiated_protocol {
        Some(protocol) if requested_protocols.iter().any(|p| p == protocol) => {
            Ok(Some(protocol.to_string()))
        }
        Some(protocol) => Err(HttpError::new(
            HttpErrorKind::ProtocolError,
            format!("Server selected unexpected WebSocket subprotocol: {protocol}"),
        )),
        None => Ok(None),
    }
}

fn ensure_header_token(
    headers: &Headers,
    name: &str,
    expected_token: &str,
) -> Result<(), HttpError> {
    let raw = headers.get(name).ok_or_else(|| {
        HttpError::new(
            HttpErrorKind::InvalidHeader,
            format!("Missing required header: {name}"),
        )
    })?;

    let found = raw
        .split(',')
        .map(|token| token.trim())
        .any(|token| token.eq_ignore_ascii_case(expected_token));
    if !found {
        return Err(HttpError::new(
            HttpErrorKind::ProtocolError,
            format!("Required token {expected_token} missing from {name} header"),
        ));
    }

    Ok(())
}

fn validate_websocket_key(key: &str) -> Result<(), HttpError> {
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(key.trim())
        .map_err(|_| {
            HttpError::new(
                HttpErrorKind::InvalidHeader,
                "Sec-WebSocket-Key is not valid base64",
            )
        })?;
    if decoded.len() != 16 {
        return Err(HttpError::new(
            HttpErrorKind::InvalidHeader,
            "Sec-WebSocket-Key must decode to exactly 16 bytes",
        ));
    }
    Ok(())
}

// Clean, memory-safe, zero-dependency RFC 3174 SHA-1 implementation for WebSocket handshakes.
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h: [u32; 5] = [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];

    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in msg.chunks_exact(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h[0];
        let mut b = h[1];
        let mut c = h[2];
        let mut d = h[3];
        let mut e = h[4];

        for (i, word) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(*word);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }

    let mut out = [0u8; 20];
    for (i, val) in h.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    out
}
