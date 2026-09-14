//! URLPattern Matching & Parameter Extraction for AdeshLang URL.

use super::url_object::URL;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct URLPatternMatch {
    params: HashMap<String, String>,
}

impl URLPatternMatch {
    pub fn new(params: HashMap<String, String>) -> Self {
        Self { params }
    }

    pub fn get(&self, param_name: &str) -> Option<&str> {
        self.params.get(param_name).map(|s| s.as_str())
    }

    pub fn params(&self) -> &HashMap<String, String> {
        &self.params
    }
}

#[derive(Debug, Clone)]
pub struct URLPattern {
    pattern_str: String,
    scheme_pattern: Option<String>,
    host_pattern: Option<String>,
    path_segments: Vec<PatternSegment>,
}

#[derive(Debug, Clone)]
enum PatternSegment {
    Exact(String),
    Param(String),    // :id
    Wildcard(String), // * or :path*
}

impl URLPattern {
    pub fn parse(pattern: &str) -> Result<Self, String> {
        let pattern = pattern.trim();
        let (scheme_pattern, host_pattern, path_str) = if pattern.contains("://") {
            let (scheme, rest) = pattern.split_once("://").unwrap();
            let (host, path) = match rest.find('/') {
                Some(pos) => (&rest[..pos], &rest[pos..]),
                None => (rest, "/"),
            };
            (Some(scheme.to_string()), Some(host.to_string()), path)
        } else {
            (None, None, pattern)
        };

        let raw_segments: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();
        let mut segments = Vec::new();

        for seg in raw_segments {
            if seg == "*" {
                segments.push(PatternSegment::Wildcard("wildcard".to_string()));
            } else if seg.starts_with(':') {
                let name = &seg[1..];
                if name.ends_with('*') {
                    segments.push(PatternSegment::Wildcard(name[..name.len() - 1].to_string()));
                } else {
                    segments.push(PatternSegment::Param(name.to_string()));
                }
            } else {
                segments.push(PatternSegment::Exact(seg.to_string()));
            }
        }

        Ok(Self {
            pattern_str: pattern.to_string(),
            scheme_pattern,
            host_pattern,
            path_segments: segments,
        })
    }

    pub fn match_url(&self, url: &URL) -> Option<URLPatternMatch> {
        if let Some(ref sp) = self.scheme_pattern {
            if sp != "*" && sp != url.scheme() {
                return None;
            }
        }

        if let Some(ref hp) = self.host_pattern {
            if let Some(h) = url.host() {
                if hp != "*" && hp != h {
                    return None;
                }
            } else {
                return None;
            }
        }

        let url_segments = url.path_segments();
        let mut params = HashMap::new();

        let mut u_idx = 0;
        let mut p_idx = 0;

        while p_idx < self.path_segments.len() {
            match &self.path_segments[p_idx] {
                PatternSegment::Exact(expected) => {
                    if u_idx >= url_segments.len() || &url_segments[u_idx] != expected {
                        return None;
                    }
                    u_idx += 1;
                    p_idx += 1;
                }
                PatternSegment::Param(name) => {
                    if u_idx >= url_segments.len() {
                        return None;
                    }
                    params.insert(name.clone(), url_segments[u_idx].clone());
                    u_idx += 1;
                    p_idx += 1;
                }
                PatternSegment::Wildcard(name) => {
                    let rest = url_segments[u_idx..].join("/");
                    params.insert(name.clone(), rest);
                    u_idx = url_segments.len();
                    p_idx += 1;
                    break;
                }
            }
        }

        if u_idx != url_segments.len() && p_idx == self.path_segments.len() {
            return None;
        }

        Some(URLPatternMatch::new(params))
    }

    pub fn pattern_string(&self) -> &str {
        &self.pattern_str
    }
}
