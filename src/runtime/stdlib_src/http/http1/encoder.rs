use super::super::errors::HttpError;
use super::super::request::Request;
use super::super::response::Response;

pub fn encode_request(req: &Request) -> Result<Vec<u8>, HttpError> {
    let mut buf = Vec::new();
    let path_and_query = req.uri.path_and_query();

    // Request-Line: METHOD URI HTTP/1.1\r\n
    let req_line = format!("{} {} {}\r\n", req.method, path_and_query, req.version);
    buf.extend_from_slice(req_line.as_bytes());

    // Headers
    for (k, vals) in req.headers.iter() {
        for v in vals {
            let line = format!("{}: {}\r\n", k, v);
            buf.extend_from_slice(line.as_bytes());
        }
    }
    buf.extend_from_slice(b"\r\n");

    // Body
    let body_bytes = req.body.to_bytes()?;
    buf.extend_from_slice(&body_bytes);

    Ok(buf)
}

pub fn encode_response(resp: &Response) -> Result<Vec<u8>, HttpError> {
    let mut buf = Vec::new();

    // Status-Line: HTTP/1.1 CODE REASON\r\n
    let status_line = format!("{} {}\r\n", resp.version, resp.status);
    buf.extend_from_slice(status_line.as_bytes());

    // Headers
    let mut headers = resp.headers.clone();
    if !headers.contains("Access-Control-Allow-Origin") {
        let _ = headers.insert("Access-Control-Allow-Origin", "*");
    }
    if !headers.contains("Access-Control-Allow-Methods") {
        let _ = headers.insert("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, PATCH, OPTIONS");
    }
    if !headers.contains("Access-Control-Allow-Headers") {
        let _ = headers.insert("Access-Control-Allow-Headers", "Content-Type, Authorization, X-Requested-With, Accept");
    }
    if !headers.contains("Access-Control-Allow-Credentials") {
        let _ = headers.insert("Access-Control-Allow-Credentials", "true");
    }

    for (k, vals) in headers.iter() {
        for v in vals {
            let line = format!("{}: {}\r\n", k, v);
            buf.extend_from_slice(line.as_bytes());
        }
    }
    buf.extend_from_slice(b"\r\n");

    // Body
    let body_bytes = resp.body.to_bytes()?;
    buf.extend_from_slice(&body_bytes);

    Ok(buf)
}
