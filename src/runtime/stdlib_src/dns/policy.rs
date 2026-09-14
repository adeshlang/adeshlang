#![allow(dead_code)]

//! DNS Security Policy (SSRF defense, rebinding protection, private network restrictions).

use crate::runtime::stdlib_src::net::ip::IPAddress;

#[derive(Debug, Clone, Default)]
pub struct DNSPolicy {
    pub deny_loopback: bool,
    pub deny_private: bool,
    pub deny_link_local: bool,
    pub deny_multicast: bool,
}

impl DNSPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn deny_loopback(mut self) -> Self {
        self.deny_loopback = true;
        self
    }

    pub fn deny_private_networks(mut self) -> Self {
        self.deny_private = true;
        self
    }

    pub fn deny_link_local(mut self) -> Self {
        self.deny_link_local = true;
        self
    }

    pub fn deny_multicast(mut self) -> Self {
        self.deny_multicast = true;
        self
    }

    pub fn validate_ip(&self, ip: &IPAddress) -> Result<(), String> {
        if self.deny_loopback && ip.is_loopback() {
            return Err(format!(
                "Security Policy: Access to loopback IP '{:?}' is denied",
                ip
            ));
        }
        if self.deny_private && ip.is_private() {
            return Err(format!(
                "Security Policy: Access to private network IP '{:?}' is denied",
                ip
            ));
        }
        if self.deny_link_local && ip.is_link_local() {
            return Err(format!(
                "Security Policy: Access to link-local IP '{:?}' is denied",
                ip
            ));
        }
        if self.deny_multicast && ip.is_multicast() {
            return Err(format!(
                "Security Policy: Access to multicast IP '{:?}' is denied",
                ip
            ));
        }
        Ok(())
    }
}
