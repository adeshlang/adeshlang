use super::request::Request;

#[derive(Debug, Clone, Default)]
pub struct SecurityAuditReport {
    pub findings: Vec<String>,
    pub is_secure: bool,
}

pub fn audit_request_security(req: &Request) -> SecurityAuditReport {
    let mut findings = Vec::new();

    if !req.uri.is_https() {
        findings.push(
            "Insecure transport: Request is using plaintext HTTP instead of HTTPS.".to_string(),
        );
    }

    if req.headers.contains("authorization") && !req.uri.is_https() {
        findings.push(
            "Critical: Sensitive 'Authorization' header sent over unencrypted channel.".to_string(),
        );
    }

    if req.method.as_str() == "TRACE" {
        findings.push(
            "Warning: TRACE method is susceptible to Cross-Site Tracing (XST) attacks.".to_string(),
        );
    }

    let is_secure = findings.is_empty();
    SecurityAuditReport {
        findings,
        is_secure,
    }
}
