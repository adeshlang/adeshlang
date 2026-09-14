#![allow(dead_code)]

//! DNSSEC Status and verification types.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DNSSECStatus {
    Secure,
    Insecure,
    Bogus,
    Indeterminate,
    Unsupported,
}

#[derive(Debug, Clone)]
pub struct DNSSECConfig {
    pub enabled: bool,
}

impl Default for DNSSECConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}
