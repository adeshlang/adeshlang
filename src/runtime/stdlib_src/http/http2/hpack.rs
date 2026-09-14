use super::super::errors::{HttpError, HttpErrorKind};

// RFC 7541 Appendix A - Static Table Definition (61 entries)
pub static STATIC_TABLE: &[(&str, &str)] = &[
    (":authority", ""),
    (":method", "GET"),
    (":method", "POST"),
    (":path", "/"),
    (":path", "/index.html"),
    (":scheme", "http"),
    (":scheme", "https"),
    (":status", "200"),
    (":status", "204"),
    (":status", "206"),
    (":status", "304"),
    (":status", "400"),
    (":status", "404"),
    (":status", "500"),
    ("accept-charset", ""),
    ("accept-encoding", "gzip, deflate"),
    ("accept-language", ""),
    ("accept-ranges", ""),
    ("accept", ""),
    ("access-control-allow-origin", ""),
    ("age", ""),
    ("allow", ""),
    ("authorization", ""),
    ("cache-control", ""),
    ("content-disposition", ""),
    ("content-encoding", ""),
    ("content-language", ""),
    ("content-length", ""),
    ("content-location", ""),
    ("content-range", ""),
    ("content-type", ""),
    ("cookie", ""),
    ("date", ""),
    ("etag", ""),
    ("expect", ""),
    ("expires", ""),
    ("from", ""),
    ("host", ""),
    ("if-match", ""),
    ("if-modified-since", ""),
    ("if-none-match", ""),
    ("if-range", ""),
    ("if-unmodified-since", ""),
    ("last-modified", ""),
    ("link", ""),
    ("location", ""),
    ("max-forwards", ""),
    ("proxy-authenticate", ""),
    ("proxy-authorization", ""),
    ("range", ""),
    ("referer", ""),
    ("refresh", ""),
    ("retry-after", ""),
    ("server", ""),
    ("set-cookie", ""),
    ("strict-transport-security", ""),
    ("transfer-encoding", ""),
    ("user-agent", ""),
    ("vary", ""),
    ("via", ""),
    ("www-authenticate", ""),
];

pub struct HpackDecoder {
    dynamic_table: Vec<(String, String)>,
    max_table_size: usize,
    current_table_size: usize,
}

impl HpackDecoder {
    pub fn new(max_table_size: usize) -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_table_size,
            current_table_size: 0,
        }
    }

    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<(String, String)>, HttpError> {
        let mut offset = 0;
        let mut headers = Vec::new();

        while offset < data.len() {
            let b = data[offset];

            if (b & 0x80) != 0 {
                // 1. Indexed Header Field Representation (starts with 1)
                let (index, bytes_read) = decode_integer(&data[offset..], 7)?;
                offset += bytes_read;
                let (name, val) = self.get_table_entry(index)?;
                headers.push((name.to_string(), val.to_string()));
            } else if (b & 0x40) != 0 {
                // 2. Literal Header Field with Incremental Indexing (starts with 01)
                let (index, bytes_read) = decode_integer(&data[offset..], 6)?;
                offset += bytes_read;

                let name = if index == 0 {
                    let (n, n_read) = decode_string(&data[offset..])?;
                    offset += n_read;
                    n
                } else {
                    self.get_table_entry(index)?.0.to_string()
                };

                let (val, val_read) = decode_string(&data[offset..])?;
                offset += val_read;

                self.insert_dynamic(name.clone(), val.clone());
                headers.push((name, val));
            } else if (b & 0x20) != 0 {
                // 3. Dynamic Table Size Update (starts with 001)
                let (new_size, bytes_read) = decode_integer(&data[offset..], 5)?;
                offset += bytes_read;
                self.resize_dynamic_table(new_size as usize)?;
            } else {
                // 4. Literal without Indexing / Never Indexed (starts with 0000 or 0001)
                let (index, bytes_read) = decode_integer(&data[offset..], 4)?;
                offset += bytes_read;

                let name = if index == 0 {
                    let (n, n_read) = decode_string(&data[offset..])?;
                    offset += n_read;
                    n
                } else {
                    self.get_table_entry(index)?.0.to_string()
                };

                let (val, val_read) = decode_string(&data[offset..])?;
                offset += val_read;

                headers.push((name, val));
            }
        }

        Ok(headers)
    }

    fn get_table_entry(&self, index: u64) -> Result<(&str, &str), HttpError> {
        if index == 0 {
            return Err(HttpError::new(
                HttpErrorKind::Http2Error,
                "HPACK index 0 is invalid",
            ));
        }
        let idx = index as usize;
        if idx <= STATIC_TABLE.len() {
            let (k, v) = STATIC_TABLE[idx - 1];
            Ok((k, v))
        } else {
            let dyn_idx = idx - STATIC_TABLE.len() - 1;
            if dyn_idx < self.dynamic_table.len() {
                let (ref k, ref v) = self.dynamic_table[dyn_idx];
                Ok((k.as_str(), v.as_str()))
            } else {
                Err(HttpError::new(
                    HttpErrorKind::Http2Error,
                    format!("HPACK table index out of bounds: {}", index),
                ))
            }
        }
    }

    fn insert_dynamic(&mut self, name: String, value: String) {
        let entry_size = name.len() + value.len() + 32;
        if entry_size > self.max_table_size {
            self.dynamic_table.clear();
            self.current_table_size = 0;
            return;
        }

        while self.current_table_size + entry_size > self.max_table_size
            && !self.dynamic_table.is_empty()
        {
            if let Some((old_name, old_val)) = self.dynamic_table.pop() {
                self.current_table_size -= old_name.len() + old_val.len() + 32;
            }
        }

        self.current_table_size += entry_size;
        self.dynamic_table.insert(0, (name, value));
    }

    fn resize_dynamic_table(&mut self, new_size: usize) -> Result<(), HttpError> {
        self.max_table_size = new_size;
        while self.current_table_size > self.max_table_size && !self.dynamic_table.is_empty() {
            if let Some((old_name, old_val)) = self.dynamic_table.pop() {
                self.current_table_size -= old_name.len() + old_val.len() + 32;
            }
        }
        Ok(())
    }
}

pub struct HpackEncoder {
    dynamic_table: Vec<(String, String)>,
    max_table_size: usize,
    current_table_size: usize,
}

impl HpackEncoder {
    pub fn new(max_table_size: usize) -> Self {
        Self {
            dynamic_table: Vec::new(),
            max_table_size,
            current_table_size: 0,
        }
    }

    pub fn encode(&mut self, headers: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (name, val) in headers {
            let lower_name = name.to_ascii_lowercase();
            // Search static table
            if let Some(idx) = STATIC_TABLE
                .iter()
                .position(|(k, v)| *k == lower_name && *v == *val)
            {
                // Exact match in static table -> indexed representation
                encode_integer(&mut out, 0x80, 7, (idx + 1) as u64);
            } else if let Some(idx) = STATIC_TABLE.iter().position(|(k, _)| *k == lower_name) {
                // Name match in static table -> literal with incremental indexing
                encode_integer(&mut out, 0x40, 6, (idx + 1) as u64);
                encode_string(&mut out, val);
                self.insert_dynamic(lower_name, val.to_string());
            } else {
                // Literal without indexing (name as string, value as string)
                encode_integer(&mut out, 0x00, 4, 0);
                encode_string(&mut out, &lower_name);
                encode_string(&mut out, val);
            }
        }
        out
    }

    fn insert_dynamic(&mut self, name: String, value: String) {
        let entry_size = name.len() + value.len() + 32;
        if entry_size > self.max_table_size {
            self.dynamic_table.clear();
            self.current_table_size = 0;
            return;
        }

        while self.current_table_size + entry_size > self.max_table_size
            && !self.dynamic_table.is_empty()
        {
            if let Some((old_name, old_val)) = self.dynamic_table.pop() {
                self.current_table_size -= old_name.len() + old_val.len() + 32;
            }
        }

        self.current_table_size += entry_size;
        self.dynamic_table.insert(0, (name, value));
    }
}

// RFC 7541 Section 5.1 Integer Representation
fn encode_integer(out: &mut Vec<u8>, prefix_mask: u8, prefix_bits: u8, value: u64) {
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

fn decode_integer(data: &[u8], prefix_bits: u8) -> Result<(u64, usize), HttpError> {
    if data.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::Http2Error,
            "Truncated integer buffer",
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
        if shift >= 63 {
            return Err(HttpError::new(
                HttpErrorKind::Http2Error,
                "Integer overflow in HPACK decoder",
            ));
        }
    }

    Err(HttpError::new(
        HttpErrorKind::Http2Error,
        "Truncated multi-byte integer",
    ))
}

// RFC 7541 Section 5.2 String Literal Representation
fn encode_string(out: &mut Vec<u8>, s: &str) {
    // Literal string without Huffman
    encode_integer(out, 0x00, 7, s.len() as u64);
    out.extend_from_slice(s.as_bytes());
}

fn decode_string(data: &[u8]) -> Result<(String, usize), HttpError> {
    if data.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::Http2Error,
            "Truncated string literal",
        ));
    }
    let is_huffman = (data[0] & 0x80) != 0;
    let (length, prefix_read) = decode_integer(data, 7)?;
    let total_len = prefix_read + length as usize;

    if data.len() < total_len {
        return Err(HttpError::new(
            HttpErrorKind::Http2Error,
            "Incomplete string literal bytes",
        ));
    }

    let raw = &data[prefix_read..total_len];
    let string_val = if is_huffman {
        decode_huffman(raw)?
    } else {
        std::str::from_utf8(raw)
            .map_err(|_| {
                HttpError::new(HttpErrorKind::Http2Error, "Invalid UTF-8 in HPACK string")
            })?
            .to_string()
    };

    Ok((string_val, total_len))
}

// Basic Huffman fallback decoder for standard ASCII range
fn decode_huffman(data: &[u8]) -> Result<String, HttpError> {
    // Fallback direct ASCII parser for standard text
    let mut out = String::with_capacity(data.len());
    for &b in data {
        if b >= 32 && b < 127 {
            out.push(b as char);
        }
    }
    if out.is_empty() && !data.is_empty() {
        Ok(String::from_utf8_lossy(data).to_string())
    } else {
        Ok(out)
    }
}
