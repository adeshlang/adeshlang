#![allow(dead_code)]

//! EDNS0 Pseudo-Record and Extension options.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EDNSOption {
    pub code: u16,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EDNS0 {
    pub udp_payload_size: u16,
    pub extended_rcode: u8,
    pub version: u8,
    pub dnssec_ok: bool,
    pub options: Vec<EDNSOption>,
}

impl Default for EDNS0 {
    fn default() -> Self {
        Self {
            udp_payload_size: 4096,
            extended_rcode: 0,
            version: 0,
            dnssec_ok: true,
            options: Vec::new(),
        }
    }
}
