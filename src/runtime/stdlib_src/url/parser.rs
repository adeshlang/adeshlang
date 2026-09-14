//! URL Parsing & Relative Resolution Engine (RFC 3986 / WHATWG) for AdeshLang.

use super::ipv4_ipv6::IPAddress;
use super::query::QueryParams;

pub const DEFAULT_MAX_URL_LENGTH: usize = 65536;
pub const DEFAULT_MAX_PARAMETERS: usize = 1000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedURLComponents {
    pub scheme: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub host: Option<String>,
    pub ip_address: Option<IPAddress>,
    pub port: Option<u16>,
    pub path: String,
    pub query: QueryParams,
    pub fragment: Option<String>,
    pub is_opaque: bool,
}

pub fn default_port_for_scheme(scheme: &str) -> Option<u16> {
    match scheme.to_lowercase().as_str() {
        "http" | "ws" => Some(80),
        "https" | "wss" => Some(443),
        "ftp" => Some(21),
        "ssh" => Some(22),
        "gopher" => Some(70),
        _ => None,
    }
}

pub fn parse_url(input: &str) -> Result<ParsedURLComponents, String> {
    parse_url_with_limits(input, DEFAULT_MAX_URL_LENGTH, DEFAULT_MAX_PARAMETERS)
}

pub fn parse_url_with_limits(
    input: &str,
    max_url_length: usize,
    max_parameters: usize,
) -> Result<ParsedURLComponents, String> {
    if input.len() > max_url_length {
        return Err(format!(
            "URL length {} exceeds maximum allowed limit of {} bytes",
            input.len(),
            max_url_length
        ));
    }

    let input = input.trim();
    if input.is_empty() {
        return Err("Cannot parse empty URL string".to_string());
    }

    // Extract Scheme
    let (scheme, rest) = if let Some(colon_pos) = input.find(':') {
        let potential_scheme = &input[..colon_pos];
        if is_valid_scheme(potential_scheme) {
            (potential_scheme.to_lowercase(), &input[colon_pos + 1..])
        } else {
            return Err(format!("Invalid scheme prefix '{}'", potential_scheme));
        }
    } else {
        return Err("URL missing scheme specifier (e.g. 'http:')".to_string());
    };

    // Special opaque schemes (data:, mailto:, urn:)
    if scheme == "data" || scheme == "mailto" || scheme == "urn" {
        let (path_and_query, fragment) = split_fragment(rest);
        let (path, query_str) = split_query(path_and_query);
        let query = QueryParams::parse(query_str.unwrap_or(""));
        return Ok(ParsedURLComponents {
            scheme,
            username: None,
            password: None,
            host: None,
            ip_address: None,
            port: None,
            path: path.to_string(),
            query,
            fragment: fragment.map(|s| s.to_string()),
            is_opaque: true,
        });
    }

    // Hierarchical schemes expect "//" after scheme
    let authority_and_rest = if rest.starts_with("//") {
        &rest[2..]
    } else {
        rest
    };

    let (authority, path_query_frag) = match authority_and_rest.find('/') {
        Some(pos) => (&authority_and_rest[..pos], &authority_and_rest[pos..]),
        None => match authority_and_rest
            .find('?')
            .or_else(|| authority_and_rest.find('#'))
        {
            Some(pos) => (&authority_and_rest[..pos], &authority_and_rest[pos..]),
            None => (authority_and_rest, ""),
        },
    };

    let (path_and_query, fragment) = split_fragment(path_query_frag);
    let (raw_path, query_str) = split_query(path_and_query);

    let (username, password, host_str, port) = parse_authority(authority)?;

    let ip_address = if let Some(ref h) = host_str {
        IPAddress::parse(h).ok()
    } else {
        None
    };

    let normalized_path = normalize_dot_segments(if raw_path.is_empty() { "/" } else { raw_path });

    let query = QueryParams::parse(query_str.unwrap_or(""));
    if query.len() > max_parameters {
        return Err(format!(
            "URL parameter count {} exceeds limit {}",
            query.len(),
            max_parameters
        ));
    }

    Ok(ParsedURLComponents {
        scheme,
        username,
        password,
        host: host_str,
        ip_address,
        port,
        path: normalized_path,
        query,
        fragment: fragment.map(|s| s.to_string()),
        is_opaque: false,
    })
}

fn is_valid_scheme(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let first = s.bytes().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return false;
    }
    s.bytes()
        .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'-' || b == b'.')
}

fn parse_authority(
    auth: &str,
) -> Result<(Option<String>, Option<String>, Option<String>, Option<u16>), String> {
    if auth.is_empty() {
        return Ok((None, None, None, None));
    }

    let (userinfo, host_port) = if let Some(at_pos) = auth.rfind('@') {
        (Some(&auth[..at_pos]), &auth[at_pos + 1..])
    } else {
        (None, auth)
    };

    let (username, password) = if let Some(uinfo) = userinfo {
        if let Some((u, p)) = uinfo.split_once(':') {
            (Some(u.to_string()), Some(p.to_string()))
        } else {
            (Some(uinfo.to_string()), None)
        }
    } else {
        (None, None)
    };

    let (host, port) = if host_port.starts_with('[') {
        // IPv6 bracketed address
        if let Some(close_bracket) = host_port.find(']') {
            let h = &host_port[..=close_bracket];
            let rest = &host_port[close_bracket + 1..];
            if rest.starts_with(':') {
                let p_str = &rest[1..];
                let p = p_str
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port number '{}'", p_str))?;
                (Some(h.to_string()), Some(p))
            } else {
                (Some(h.to_string()), None)
            }
        } else {
            return Err("Unclosed IPv6 bracket in host authority".to_string());
        }
    } else if let Some((h, p_str)) = host_port.rsplit_once(':') {
        if !h.contains(':') {
            let p = p_str
                .parse::<u16>()
                .map_err(|_| format!("Invalid port number '{}'", p_str))?;
            (Some(h.to_string()), Some(p))
        } else {
            // Likely raw IPv6 without brackets
            (Some(host_port.to_string()), None)
        }
    } else {
        (Some(host_port.to_string()), None)
    };

    Ok((username, password, host, port))
}

fn split_fragment(s: &str) -> (&str, Option<&str>) {
    if let Some(pos) = s.find('#') {
        (&s[..pos], Some(&s[pos + 1..]))
    } else {
        (s, None)
    }
}

fn split_query(s: &str) -> (&str, Option<&str>) {
    if let Some(pos) = s.find('?') {
        (&s[..pos], Some(&s[pos + 1..]))
    } else {
        (s, None)
    }
}

/// Normalize RFC 3986 dot-segments (`/a/b/../c` -> `/a/c`).
pub fn normalize_dot_segments(path: &str) -> String {
    let is_absolute = path.starts_with('/');
    let has_trailing_slash = path.ends_with('/') || path.ends_with("/.") || path.ends_with("/..");
    let segments = path.split('/');
    let mut stack = Vec::new();

    for seg in segments {
        match seg {
            "" | "." => {}
            ".." => {
                stack.pop();
            }
            normal => {
                stack.push(normal);
            }
        }
    }

    let mut res = String::new();
    if is_absolute {
        res.push('/');
    }
    res.push_str(&stack.join("/"));
    if has_trailing_slash && !res.ends_with('/') {
        res.push('/');
    }
    res
}

/// RFC 3986 Section 5 Relative Resolution.
pub fn resolve_relative(
    base: &ParsedURLComponents,
    relative_input: &str,
) -> Result<ParsedURLComponents, String> {
    let rel_input = relative_input.trim();
    if rel_input.is_empty() {
        return Ok(base.clone());
    }

    // Check if relative input has explicit scheme
    if let Ok(rel_parsed) = parse_url(rel_input) {
        if !rel_parsed.scheme.is_empty() && rel_parsed.scheme != base.scheme {
            return Ok(rel_parsed);
        }
    }

    let (rel_path_query_frag, rel_frag) = split_fragment(rel_input);
    let (rel_path_raw, rel_query_str) = split_query(rel_path_query_frag);

    let frag = rel_frag
        .map(|s| s.to_string())
        .or_else(|| base.fragment.clone());

    if rel_path_raw.is_empty() && rel_query_str.is_some() {
        // Query-only reference
        let mut resolved = base.clone();
        resolved.query = QueryParams::parse(rel_query_str.unwrap());
        resolved.fragment = frag;
        return Ok(resolved);
    }

    let (target_path, target_query) = if rel_path_raw.starts_with('/') {
        // Absolute path reference
        (
            normalize_dot_segments(rel_path_raw),
            rel_query_str.map(QueryParams::parse).unwrap_or_default(),
        )
    } else {
        // Relative path reference
        let base_dir = if let Some(last_slash) = base.path.rfind('/') {
            &base.path[..=last_slash]
        } else {
            "/"
        };
        let combined = format!("{}{}", base_dir, rel_path_raw);
        (
            normalize_dot_segments(&combined),
            rel_query_str.map(QueryParams::parse).unwrap_or_default(),
        )
    };

    let mut resolved = base.clone();
    resolved.path = target_path;
    resolved.query = target_query;
    resolved.fragment = frag;
    Ok(resolved)
}
