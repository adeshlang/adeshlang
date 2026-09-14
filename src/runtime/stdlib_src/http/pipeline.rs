use std::sync::Arc;

use super::errors::HttpError;
use super::request::Request;
use super::response::Response;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MiddlewarePhase {
    Security,
    Policy,
    Authentication,
    Cache,
    Retry,
    Redirect,
    Proxy,
    Dns,
    Connection,
    Tls,
    Protocol,
    Digest,
    Observability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MiddlewarePriority {
    pub phase: MiddlewarePhase,
    pub order: i32,
}

impl MiddlewarePriority {
    pub fn new(phase: MiddlewarePhase, order: i32) -> Self {
        Self { phase, order }
    }
}

pub trait HttpMiddleware: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> MiddlewarePriority;

    fn before_request(&self, _request: &mut Request) -> Result<(), HttpError> {
        Ok(())
    }

    fn after_response(
        &self,
        _request: &Request,
        _response: &mut Response,
    ) -> Result<(), HttpError> {
        Ok(())
    }

    fn on_error(&self, _request: &Request, _error: &HttpError) {}
}

pub type ClientMiddleware = dyn HttpMiddleware;
pub type ServerMiddleware = dyn HttpMiddleware;

pub struct MiddlewarePipeline {
    middlewares: Vec<Arc<dyn HttpMiddleware>>,
}

impl MiddlewarePipeline {
    pub fn new() -> Self {
        Self {
            middlewares: Vec::new(),
        }
    }

    pub fn add<M: HttpMiddleware + 'static>(&mut self, middleware: M) {
        self.middlewares.push(Arc::new(middleware));
        self.middlewares.sort_by_key(|m| {
            let p = m.priority();
            (p.phase, p.order)
        });
    }

    pub fn ordered_names(&self) -> Vec<String> {
        self.middlewares
            .iter()
            .map(|m| m.name().to_string())
            .collect()
    }

    pub fn run_before_request(&self, request: &mut Request) -> Result<(), HttpError> {
        for middleware in &self.middlewares {
            middleware.before_request(request)?;
        }
        Ok(())
    }

    pub fn run_after_response(
        &self,
        request: &Request,
        response: &mut Response,
    ) -> Result<(), HttpError> {
        for middleware in self.middlewares.iter().rev() {
            middleware.after_response(request, response)?;
        }
        Ok(())
    }

    pub fn run_error(&self, request: &Request, error: &HttpError) {
        for middleware in &self.middlewares {
            middleware.on_error(request, error);
        }
    }
}

impl Default for MiddlewarePipeline {
    fn default() -> Self {
        Self::new()
    }
}
