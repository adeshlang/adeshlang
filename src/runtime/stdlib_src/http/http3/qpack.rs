use super::super::errors::{HttpError, HttpErrorKind};

// RFC 9204 Appendix A - QPACK Static Table (99 entries)
pub static QPACK_STATIC_TABLE: &[(&str, &str)] = &[
    (":authority", ""),
    (":path", "/"),
    ("age", "0"),
    ("content-disposition", ""),
    ("content-length", "0"),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("referer", ""),
    ("set-cookie", ""),
    (":method", "CONNECT"),
    (":method", "DELETE"),
    (":method", "GET"),
    (":method", "HEAD"),
    (":method", "OPTIONS"),
    (":method", "POST"),
    (":method", "PUT"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "103"),
    (":status", "200"),
    (":status", "304"),
    (":status", "404"),
    (":status", "503"),
    ("accept", "*/*"),
    ("accept", "application/dns-message"),
    ("accept-encoding", "gzip, deflate, br"),
    ("accept-ranges", "bytes"),
    ("access-control-allow-headers", "cache-control"),
    ("access-control-allow-headers", "content-type"),
    ("access-control-allow-origin", "*"),
    ("cache-control", "max-age=0"),
    ("cache-control", "max-age=2592000"),
    ("cache-control", "max-age=604800"),
    ("cache-control", "no-cache"),
    ("cache-control", "no-store"),
    ("cache-control", "public, max-age=31536000"),
    ("content-encoding", "br"),
    ("content-encoding", "gzip"),
    ("content-type", "application/dns-message"),
    ("content-type", "application/javascript"),
    ("content-type", "application/json"),
    ("content-type", "image/gif"),
    ("content-type", "image/jpeg"),
    ("content-type", "image/png"),
    ("content-type", "text/css"),
    ("content-type", "text/html; charset=utf-8"),
    ("content-type", "text/plain"),
    ("content-type", "text/plain;charset=utf-8"),
    ("range", "bytes=0-"),
    ("strict-transport-security", "max-age=31536000"),
    ("vary", "accept-encoding"),
    ("x-content-type-options", "nosniff"),
    ("x-xss-protection", "1; mode=block"),
    (":status", "100"),
    (":status", "204"),
    (":status", "206"),
    (":status", "302"),
    (":status", "400"),
    (":status", "403"),
    (":status", "421"),
    (":status", "425"),
    (":status", "500"),
    ("accept-language", ""),
    ("access-control-allow-credentials", "FALSE"),
    ("access-control-allow-credentials", "TRUE"),
    ("access-control-allow-methods", "get"),
    ("access-control-allow-methods", "get, post, options"),
    ("access-control-expose-headers", "content-length"),
    ("alt-svc", "clear"),
    ("authorization", ""),
    (
        "content-security-policy",
        "script-src 'none'; object-src 'none'; base-uri 'none'",
    ),
    ("early-data", "1"),
    ("expect-ct", ""),
    ("forwarded", ""),
    ("if-range", ""),
    ("origin", ""),
    ("purpose", "prefetch"),
    ("server", ""),
    ("timing-allow-origin", "*"),
    ("upgrade-insecure-requests", "1"),
    ("user-agent", ""),
    ("x-forwarded-for", ""),
    ("x-frame-options", "deny"),
    ("x-frame-options", "sameorigin"),
];

pub struct QpackEncoder {
    pub max_table_size: usize,
}

impl QpackEncoder {
    pub fn new(max_table_size: usize) -> Self {
        Self { max_table_size }
    }

    pub fn max_table_size(&self) -> usize {
        self.max_table_size
    }

    pub fn encode(&self, headers: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        // Encoded Field Section Prefix: Required Insert Count (0) + Delta Base (0)
        out.push(0x00);
        out.push(0x00);

        for (name, val) in headers {
            let lower_name = name.to_ascii_lowercase();
            // Check exact static match
            if let Some(idx) = QPACK_STATIC_TABLE
                .iter()
                .position(|(k, v)| *k == lower_name && *v == *val)
            {
                // Indexed Header Field from static table (starts with 11)
                encode_qpack_integer(&mut out, 0xC0, 6, idx as u64);
            } else if let Some(idx) = QPACK_STATIC_TABLE
                .iter()
                .position(|(k, _)| *k == lower_name)
            {
                // Literal with Static Name Reference (starts with 0101)
                encode_qpack_integer(&mut out, 0x50, 4, idx as u64);
                encode_qpack_string(&mut out, val);
            } else {
                // Literal with Literal Name (starts with 0010)
                encode_qpack_integer(&mut out, 0x20, 3, 0);
                encode_qpack_string(&mut out, &lower_name);
                encode_qpack_string(&mut out, val);
            }
        }
        out
    }
}

pub struct QpackDecoder {
    pub max_table_size: usize,
}

impl QpackDecoder {
    pub fn new(max_table_size: usize) -> Self {
        Self { max_table_size }
    }

    pub fn max_table_size(&self) -> usize {
        self.max_table_size
    }

    pub fn decode(&self, data: &[u8]) -> Result<Vec<(String, String)>, HttpError> {
        if data.len() < 2 {
            return Err(HttpError::new(
                HttpErrorKind::Http3Error,
                "Truncated QPACK header block",
            ));
        }

        // Skip prefix (Required Insert Count + Base)
        let mut offset = 2;
        let mut headers = Vec::new();

        while offset < data.len() {
            let b = data[offset];
            if (b & 0xC0) == 0xC0 {
                // Static indexed entry
                let (idx, bytes_read) = decode_qpack_integer(&data[offset..], 6)?;
                offset += bytes_read;
                if (idx as usize) < QPACK_STATIC_TABLE.len() {
                    let (k, v) = QPACK_STATIC_TABLE[idx as usize];
                    headers.push((k.to_string(), v.to_string()));
                }
            } else if (b & 0xF0) == 0x50 {
                // Literal with Static Name Reference
                let (idx, bytes_read) = decode_qpack_integer(&data[offset..], 4)?;
                offset += bytes_read;
                let name = if (idx as usize) < QPACK_STATIC_TABLE.len() {
                    QPACK_STATIC_TABLE[idx as usize].0.to_string()
                } else {
                    "unknown".to_string()
                };
                let (val, val_read) = decode_qpack_string(&data[offset..])?;
                offset += val_read;
                headers.push((name, val));
            } else if (b & 0xE0) == 0x20 {
                // Literal with Literal Name
                offset += 1;
                let (name, n_read) = decode_qpack_string(&data[offset..])?;
                offset += n_read;
                let (val, v_read) = decode_qpack_string(&data[offset..])?;
                offset += v_read;
                headers.push((name, val));
            } else {
                offset += 1;
            }
        }

        Ok(headers)
    }
}

fn encode_qpack_integer(out: &mut Vec<u8>, prefix_mask: u8, prefix_bits: u8, value: u64) {
    let max_prefix = (1u64 << prefix_bits) - 1;
    if value < max_prefix {
        out.push(prefix_mask | (value as u8));
    } else {
        out.push(prefix_mask | (max_prefix as u8));
        let mut val = value - max_prefix;
        while val >= 128 {
            out.push(((val % 128) as u8) | 128);
            val /= 128;
        }
        out.push(val as u8);
    }
}

fn decode_qpack_integer(data: &[u8], prefix_bits: u8) -> Result<(u64, usize), HttpError> {
    if data.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::Http3Error,
            "Truncated QPACK integer",
        ));
    }
    let max_prefix = (1u64 << prefix_bits) - 1;
    let first = (data[0] as u64) & max_prefix;
    if first < max_prefix {
        return Ok((first, 1));
    }

    let mut value = max_prefix;
    let mut shift = 0;
    let mut offset = 1;

    while offset < data.len() {
        let b = data[offset] as u64;
        offset += 1;
        value += (b & 127) << shift;
        shift += 7;
        if (b & 128) == 0 {
            return Ok((value, offset));
        }
    }

    Err(HttpError::new(
        HttpErrorKind::Http3Error,
        "Truncated multi-byte QPACK integer",
    ))
}

fn encode_qpack_string(out: &mut Vec<u8>, s: &str) {
    encode_qpack_integer(out, 0x00, 7, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn decode_qpack_string(data: &[u8]) -> Result<(String, usize), HttpError> {
    if data.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::Http3Error,
            "Truncated QPACK string",
        ));
    }
    let (length, prefix_read) = decode_qpack_integer(data, 7)?;
    let total_len = prefix_read + length as usize;

    if data.len() < total_len {
        return Err(HttpError::new(
            HttpErrorKind::Http3Error,
            "Incomplete QPACK string bytes",
        ));
    }

    let raw = &data[prefix_read..total_len];
    let string_val = String::from_utf8_lossy(raw).to_string();
    Ok((string_val, total_len))
}
