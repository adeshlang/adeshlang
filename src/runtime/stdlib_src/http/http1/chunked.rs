use super::super::errors::{HttpError, HttpErrorKind};
use super::super::headers::Headers;

/// Decodes chunked transfer encoding stream and parses optional trailers
pub fn decode_chunked(data: &[u8]) -> Result<(Vec<u8>, Option<Headers>, usize), HttpError> {
    let mut offset = 0;
    let mut body = Vec::new();

    loop {
        // Find chunk size line CRLF
        let line_end = find_crlf(&data[offset..]).ok_or_else(|| {
            HttpError::new(HttpErrorKind::Http1Error, "Incomplete chunk size line")
        })?;

        let size_str = std::str::from_utf8(&data[offset..offset + line_end]).map_err(|_| {
            HttpError::new(
                HttpErrorKind::Http1Error,
                "Invalid UTF-8 in chunk size line",
            )
        })?;

        // Ignore chunk extensions after ';'
        let clean_size_str = size_str.split(';').next().unwrap_or("").trim();
        let chunk_size = usize::from_str_radix(clean_size_str, 16).map_err(|_| {
            HttpError::new(
                HttpErrorKind::Http1Error,
                format!("Invalid chunk hex size: {:?}", clean_size_str),
            )
        })?;

        offset += line_end + 2; // skip chunk line + CRLF

        if chunk_size == 0 {
            // Last-chunk reached! Now check for optional trailers
            let mut trailers = None;
            if offset + 2 <= data.len() && &data[offset..offset + 2] == b"\r\n" {
                offset += 2;
            } else {
                // Parse trailers until empty line CRLF
                let mut tr_headers = Headers::new();
                while offset < data.len() {
                    let tr_end = find_crlf(&data[offset..]).ok_or_else(|| {
                        HttpError::new(HttpErrorKind::Http1Error, "Incomplete trailer line")
                    })?;
                    if tr_end == 0 {
                        offset += 2;
                        break;
                    }
                    let tr_line =
                        std::str::from_utf8(&data[offset..offset + tr_end]).map_err(|_| {
                            HttpError::new(HttpErrorKind::Http1Error, "Invalid trailer UTF-8")
                        })?;
                    if let Some(colon) = tr_line.find(':') {
                        let name = &tr_line[..colon];
                        let val = &tr_line[colon + 1..];
                        tr_headers.append(name, val)?;
                    }
                    offset += tr_end + 2;
                }
                trailers = Some(tr_headers);
            }
            return Ok((body, trailers, offset));
        }

        if offset + chunk_size + 2 > data.len() {
            return Err(HttpError::new(
                HttpErrorKind::Http1Error,
                "Incomplete chunk data",
            ));
        }

        body.extend_from_slice(&data[offset..offset + chunk_size]);
        offset += chunk_size;

        // Verify trailing CRLF
        if &data[offset..offset + 2] != b"\r\n" {
            return Err(HttpError::new(
                HttpErrorKind::Http1Error,
                "Missing CRLF after chunk data",
            ));
        }
        offset += 2;
    }
}

pub fn encode_chunk(chunk: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let hex_len = format!("{:x}\r\n", chunk.len());
    out.extend_from_slice(hex_len.as_bytes());
    out.extend_from_slice(chunk);
    out.extend_from_slice(b"\r\n");
    out
}

pub fn encode_final_chunk(trailers: Option<&Headers>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"0\r\n");
    if let Some(trs) = trailers {
        for (k, vals) in trs.iter() {
            for v in vals {
                out.extend_from_slice(format!("{}: {}\r\n", k, v).as_bytes());
            }
        }
    }
    out.extend_from_slice(b"\r\n");
    out
}

fn find_crlf(data: &[u8]) -> Option<usize> {
    for i in 0..data.len().saturating_sub(1) {
        if data[i] == b'\r' && data[i + 1] == b'\n' {
            return Some(i);
        }
    }
    None
}
