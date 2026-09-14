use super::super::errors::{HttpError, HttpErrorKind};
use super::super::headers::Headers;

/// Enforces RFC 9112 strict framing rules to prevent HTTP request smuggling attacks:
/// - Reject conflicting Content-Length and Transfer-Encoding
/// - Reject duplicate conflicting Content-Length values
/// - Reject malformed Transfer-Encoding header values
pub fn validate_framing_security(headers: &Headers) -> Result<(), HttpError> {
    let cl_vals = headers.get_all("content-length");
    let te_vals = headers.get_all("transfer-encoding");

    // 1. Smuggling check: Both Content-Length and Transfer-Encoding present
    if !cl_vals.is_empty() && !te_vals.is_empty() {
        return Err(HttpError::new(
            HttpErrorKind::SecurityViolation,
            "Request smuggling rejected: both Content-Length and Transfer-Encoding are present (CL-TE / TE-CL)",
        ));
    }

    // 2. Duplicate Content-Length check: must all be identical non-negative integers
    if cl_vals.len() > 1 {
        let first = cl_vals[0].trim();
        for val in &cl_vals[1..] {
            if val.trim() != first {
                return Err(HttpError::new(
                    HttpErrorKind::SecurityViolation,
                    "Request smuggling rejected: conflicting duplicate Content-Length headers",
                ));
            }
        }
    }

    // 3. Content-Length valid integer check
    if let Some(cl) = headers.get("content-length") {
        if cl.trim().starts_with('+')
            || cl.trim().starts_with('-')
            || cl.trim().parse::<u64>().is_err()
        {
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!("Invalid Content-Length value: {:?}", cl),
            ));
        }
    }

    // 4. Transfer-Encoding validation
    if let Some(te) = headers.get("transfer-encoding") {
        let te_lower = te.trim().to_ascii_lowercase();
        // The only standardized final transfer-coding in HTTP/1.1 is "chunked"
        if !te_lower.ends_with("chunked") {
            return Err(HttpError::new(
                HttpErrorKind::SecurityViolation,
                format!("Unsupported or malformed Transfer-Encoding: {:?}", te),
            ));
        }
    }

    Ok(())
}
