use super::super::client::HttpClient;
use super::super::errors::{HttpError, HttpErrorKind};
use super::super::request::Request;
use super::super::response::Response;
use super::super::uri::Uri;
use super::load_balancer::LoadBalancer;

pub struct ReverseProxy {
    pub load_balancer: LoadBalancer,
    pub client: HttpClient,
}

impl ReverseProxy {
    pub fn new(load_balancer: LoadBalancer) -> Self {
        Self {
            load_balancer,
            client: HttpClient::new(),
        }
    }

    pub fn forward(
        &self,
        incoming_req: &Request,
        client_ip: Option<&str>,
    ) -> Result<Response, HttpError> {
        let target = self.load_balancer.next_target().ok_or_else(|| {
            HttpError::new(HttpErrorKind::ProxyError, "No healthy upstreams available")
        })?;

        let target_uri = Uri::parse(&target.url)?;
        let mut forward_req = Request::new(incoming_req.method.clone(), target_uri);
        forward_req.body = incoming_req.body.clone();

        // Copy incoming headers
        for (k, vals) in incoming_req.headers.iter() {
            if k != "host" {
                for v in vals {
                    let _ = forward_req.headers.append(k, v);
                }
            }
        }

        // Add X-Forwarded-For and X-Forwarded-Proto
        if let Some(ip) = client_ip {
            let _ = forward_req.headers.append("x-forwarded-for", ip);
        }
        let proto = if incoming_req.uri.is_https() {
            "https"
        } else {
            "http"
        };
        let _ = forward_req.headers.insert("x-forwarded-proto", proto);

        self.client.send(forward_req)
    }
}
