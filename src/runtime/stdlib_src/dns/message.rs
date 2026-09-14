#![allow(dead_code)]

//! DNS Message & Header types.

use super::name::DNSName;
use super::records::{DNSClass, DNSRecord, DNSType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DNSHeader {
    pub id: u16,
    pub qr: bool,   // false = query, true = response
    pub opcode: u8, // 0 = QUERY, etc.
    pub aa: bool,   // Authoritative Answer
    pub tc: bool,   // Truncated
    pub rd: bool,   // Recursion Desired
    pub ra: bool,   // Recursion Available
    pub z: bool,    // Reserved
    pub ad: bool,   // Authentic Data (DNSSEC)
    pub cd: bool,   // Checking Disabled (DNSSEC)
    pub rcode: u8,  // Response Code (0=NOERROR, 3=NXDOMAIN, etc.)
    pub qdcount: u16,
    pub ancount: u16,
    pub nscount: u16,
    pub arcount: u16,
}

impl Default for DNSHeader {
    fn default() -> Self {
        Self {
            id: 0,
            qr: false,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: false,
            z: false,
            ad: false,
            cd: false,
            rcode: 0,
            qdcount: 0,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DNSQuestion {
    pub name: DNSName,
    pub qtype: DNSType,
    pub qclass: DNSClass,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DNSMessage {
    pub header: DNSHeader,
    pub questions: Vec<DNSQuestion>,
    pub answers: Vec<DNSRecord>,
    pub authorities: Vec<DNSRecord>,
    pub additionals: Vec<DNSRecord>,
}

impl DNSMessage {
    pub fn new_query(name: DNSName, qtype: DNSType, id: u16) -> Self {
        let mut header = DNSHeader::default();
        header.id = id;
        header.rd = true;
        header.qdcount = 1;

        let question = DNSQuestion {
            name,
            qtype,
            qclass: DNSClass::IN,
        };

        Self {
            header,
            questions: vec![question],
            answers: Vec::new(),
            authorities: Vec::new(),
            additionals: Vec::new(),
        }
    }
}
