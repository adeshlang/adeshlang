use super::super::errors::HttpError;
use super::super::response::Response;
use super::context::RequestContext;
use std::sync::Arc;

pub type MiddlewareFn = Arc<
    dyn Fn(
            &mut RequestContext,
            &dyn Fn(&mut RequestContext) -> Result<Response, HttpError>,
        ) -> Result<Response, HttpError>
        + Send
        + Sync,
>;

pub struct CorsMiddleware {
    pub allow_origin: String,
    pub allow_methods: String,
    pub allow_headers: String,
    pub allow_credentials: bool,
    pub max_age: u32,
}

impl Default for CorsMiddleware {
    fn default() -> Self {
        Self {
            allow_origin: "*".to_string(),
            allow_methods: "GET, POST, PUT, DELETE, OPTIONS, PATCH, HEAD".to_string(),
            allow_headers: "Content-Type, Authorization, X-Requested-With, X-Request-ID"
                .to_string(),
            allow_credentials: true,
            max_age: 86400,
        }
    }
}

impl CorsMiddleware {
    pub fn apply(
        &self,
        ctx: &mut RequestContext,
        next: &dyn Fn(&mut RequestContext) -> Result<Response, HttpError>,
    ) -> Result<Response, HttpError> {
        if ctx.request.method == super::super::method::HttpMethod::Options {
            let mut resp = Response::no_content();
            let _ = resp
                .headers
                .insert("access-control-allow-origin", &self.allow_origin);
            let _ = resp
                .headers
                .insert("access-control-allow-methods", &self.allow_methods);
            let _ = resp
                .headers
                .insert("access-control-allow-headers", &self.allow_headers);
            let _ = resp
                .headers
                .insert("access-control-max-age", &self.max_age.to_string());
            if self.allow_credentials && self.allow_origin != "*" {
                let _ = resp
                    .headers
                    .insert("access-control-allow-credentials", "true");
            }
            return Ok(resp);
        }

        let mut resp = next(ctx)?;
        let _ = resp
            .headers
            .insert("access-control-allow-origin", &self.allow_origin);
        if self.allow_credentials && self.allow_origin != "*" {
            let _ = resp
                .headers
                .insert("access-control-allow-credentials", "true");
        }
        Ok(resp)
    }
}

pub struct SecurityHeadersMiddleware;

impl SecurityHeadersMiddleware {
    pub fn apply(
        ctx: &mut RequestContext,
        next: &dyn Fn(&mut RequestContext) -> Result<Response, HttpError>,
    ) -> Result<Response, HttpError> {
        let mut resp = next(ctx)?;
        let _ = resp.headers.insert("x-content-type-options", "nosniff");
        let _ = resp.headers.insert("x-frame-options", "DENY");
        let _ = resp
            .headers
            .insert("referrer-policy", "strict-origin-when-cross-origin");
        let _ = resp
            .headers
            .insert("cross-origin-opener-policy", "same-origin");
        Ok(resp)
    }
}
