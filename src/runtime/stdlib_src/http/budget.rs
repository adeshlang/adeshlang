use super::errors::{HttpError, HttpErrorKind};
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct RequestBudget {
    pub deadline: Option<Instant>,
    pub max_bytes: Option<usize>,
    pub max_redirects: usize,
    pub max_retries: usize,
    pub bytes_consumed: usize,
}

impl RequestBudget {
    pub fn new() -> Self {
        Self {
            deadline: None,
            max_bytes: None,
            max_redirects: 10,
            max_retries: 3,
            bytes_consumed: 0,
        }
    }

    pub fn with_timeout(mut self, dur: Duration) -> Self {
        self.deadline = Some(Instant::now() + dur);
        self
    }

    pub fn with_max_bytes(mut self, bytes: usize) -> Self {
        self.max_bytes = Some(bytes);
        self
    }

    pub fn check_deadline(&self) -> Result<(), HttpError> {
        if let Some(dl) = self.deadline {
            if Instant::now() > dl {
                return Err(HttpError::new(
                    HttpErrorKind::Timeout,
                    "Request budget deadline exceeded",
                ));
            }
        }
        Ok(())
    }

    pub fn record_bytes(&mut self, bytes: usize) -> Result<(), HttpError> {
        self.bytes_consumed += bytes;
        if let Some(max_b) = self.max_bytes {
            if self.bytes_consumed > max_b {
                return Err(HttpError::new(
                    HttpErrorKind::BodyTooLarge,
                    format!(
                        "Request budget max bytes exceeded: {} > {}",
                        self.bytes_consumed, max_b
                    ),
                ));
            }
        }
        Ok(())
    }
}
