use super::super::errors::HttpError;
use super::super::request::Request;
use super::super::response::Response;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_factor: f64,
    pub retry_idempotent_only: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(5),
            backoff_factor: 2.0,
            retry_idempotent_only: true,
        }
    }
}

impl RetryPolicy {
    pub fn should_retry(
        &self,
        req: &Request,
        result: &Result<Response, HttpError>,
        attempt: usize,
    ) -> Option<Duration> {
        if attempt >= self.max_retries {
            return None;
        }

        // Check idempotency rule: only safe/idempotent or requests with Idempotency-Key
        let has_idempotency_key = req.headers.contains("idempotency-key");
        if self.retry_idempotent_only && !req.method.is_idempotent() && !has_idempotency_key {
            return None;
        }

        let is_retryable = match result {
            Ok(resp) => resp.status.is_retryable(),
            Err(err) => err.is_retryable(),
        };

        if !is_retryable {
            return None;
        }

        // Calculate exponential backoff with jitter
        let mult = self.backoff_factor.powi(attempt as i32);
        let millis = (self.initial_delay.as_millis() as f64 * mult) as u64;
        let clamped = Duration::from_millis(millis).min(self.max_delay);

        Some(clamped)
    }
}
