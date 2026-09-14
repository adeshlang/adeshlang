use super::super::errors::HttpError;
use super::super::method::HttpMethod;
use super::super::response::Response;
use super::context::RequestContext;
use crate::utils::collections::FastMap;
use std::sync::Arc;

pub type HandlerFn = Arc<dyn Fn(&mut RequestContext) -> Result<Response, HttpError> + Send + Sync>;

#[derive(Clone, Default)]
struct RouteNode {
    segment: String,
    is_param: bool,
    is_wildcard: bool,
    handlers: FastMap<HttpMethod, HandlerFn>,
    children: Vec<RouteNode>,
}

impl RouteNode {
    fn new(segment: String) -> Self {
        let is_param = segment.starts_with(':');
        let is_wildcard = segment.starts_with('*');
        Self {
            segment,
            is_param,
            is_wildcard,
            handlers: FastMap::default(),
            children: Vec::new(),
        }
    }
}

#[derive(Clone, Default)]
pub struct Router {
    root: RouteNode,
}

impl Router {
    pub fn new() -> Self {
        Self {
            root: RouteNode::new("".to_string()),
        }
    }

    pub fn add_route(&mut self, method: HttpMethod, path: &str, handler: HandlerFn) {
        let segments: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let mut curr = &mut self.root;

        for seg in segments {
            let idx = if let Some(i) = curr.children.iter().position(|c| c.segment == seg) {
                i
            } else {
                curr.children.push(RouteNode::new(seg.to_string()));
                curr.children.len() - 1
            };
            curr = &mut curr.children[idx];
        }

        curr.handlers.insert(method, handler);
    }

    pub fn allowed_methods_for_path(&self, path: &str) -> Vec<HttpMethod> {
        let segments: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let mut allowed = Vec::new();
        self.collect_methods_recursive(&self.root, &segments, 0, &mut allowed);
        if allowed.contains(&HttpMethod::Get) && !allowed.contains(&HttpMethod::Head) {
            allowed.push(HttpMethod::Head);
        }
        if !allowed.is_empty() && !allowed.contains(&HttpMethod::Options) {
            allowed.push(HttpMethod::Options);
        }
        allowed
    }

    fn collect_methods_recursive(
        &self,
        node: &RouteNode,
        segments: &[&str],
        depth: usize,
        allowed: &mut Vec<HttpMethod>,
    ) {
        if depth == segments.len() {
            for m in node.handlers.keys() {
                if !allowed.contains(m) {
                    allowed.push(m.clone());
                }
            }
            return;
        }

        let seg = segments[depth];
        for child in &node.children {
            if child.segment == seg || child.is_param || child.is_wildcard {
                self.collect_methods_recursive(child, segments, depth + 1, allowed);
            }
        }
    }

    pub fn match_route(
        &self,
        method: &HttpMethod,
        path: &str,
    ) -> Option<(HandlerFn, FastMap<String, String>)> {
        let segments: Vec<&str> = path
            .trim_matches('/')
            .split('/')
            .filter(|s| !s.is_empty())
            .collect();
        let mut params = FastMap::default();

        if let Some(handler) = self.match_recursive(&self.root, &segments, 0, method, &mut params) {
            Some((handler, params))
        } else if *method == HttpMethod::Options {
            // Automatic OPTIONS fallback for CORS preflight
            let allowed = self.allowed_methods_for_path(path);
            if !allowed.is_empty() {
                let allow_str = allowed
                    .iter()
                    .map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                let dummy_handler: HandlerFn = Arc::new(move |_ctx| {
                    let mut resp = Response::new(super::super::status::HttpStatus::NO_CONTENT);
                    let _ = resp.headers.insert("allow", &allow_str);
                    let _ = resp.headers.insert("access-control-allow-origin", "*");
                    let _ = resp.headers.insert(
                        "access-control-allow-methods",
                        "GET, POST, PUT, DELETE, PATCH, OPTIONS",
                    );
                    let _ = resp.headers.insert("access-control-allow-headers", "*");
                    Ok(resp)
                });
                Some((dummy_handler, params))
            } else {
                None
            }
        } else if *method == HttpMethod::Head {
            // Automatic HEAD fallback to GET handler
            if let Some(handler) =
                self.match_recursive(&self.root, &segments, 0, &HttpMethod::Get, &mut params)
            {
                Some((handler, params))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn match_recursive(
        &self,
        node: &RouteNode,
        segments: &[&str],
        depth: usize,
        method: &HttpMethod,
        params: &mut FastMap<String, String>,
    ) -> Option<HandlerFn> {
        if depth == segments.len() {
            return node.handlers.get(method).cloned();
        }

        let seg = segments[depth];

        // 1. Try exact match
        for child in &node.children {
            if !child.is_param && !child.is_wildcard && child.segment == seg {
                if let Some(h) = self.match_recursive(child, segments, depth + 1, method, params) {
                    return Some(h);
                }
            }
        }

        // 2. Try param match (:id or :id<int> or :id<uuid>)
        for child in &node.children {
            if child.is_param {
                let mut raw = &child.segment[1..];
                let mut constraint = "";
                if let Some(pos) = raw.find('<') {
                    if raw.ends_with('>') {
                        constraint = &raw[pos + 1..raw.len() - 1];
                        raw = &raw[..pos];
                    }
                }
                let param_name = raw.to_string();

                // Check constraint
                let valid = match constraint {
                    "int" => seg.parse::<i64>().is_ok(),
                    "uuid" => seg.len() == 36 && seg.contains('-'),
                    _ => true,
                };

                if valid {
                    params.insert(param_name.clone(), seg.to_string());
                    if let Some(h) =
                        self.match_recursive(child, segments, depth + 1, method, params)
                    {
                        return Some(h);
                    }
                    params.remove(&param_name);
                }
            }
        }

        // 3. Try wildcard match (*path)
        for child in &node.children {
            if child.is_wildcard {
                let wildcard_val = segments[depth..].join("/");
                let param_name = if child.segment.len() > 1 {
                    child.segment[1..].to_string()
                } else {
                    "wildcard".to_string()
                };
                params.insert(param_name, wildcard_val);
                return child.handlers.get(method).cloned();
            }
        }

        None
    }
}
