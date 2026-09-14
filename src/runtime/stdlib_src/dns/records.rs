#![allow(dead_code)]

//! DNS Record Types & Data structures.

use super::name::DNSName;
use crate::runtime::stdlib_src::net::ip::IPAddress;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum DNSType {
    A = 1,
    NS = 2,
    CNAME = 5,
    SOA = 6,
    PTR = 12,
    MX = 15,
    TXT = 16,
    AAAA = 28,
    SRV = 33,
    NAPTR = 35,
    OPT = 41,
    DS = 43,
    RRSIG = 46,
    NSEC = 47,
    DNSKEY = 48,
    NSEC3 = 50,
    TLSA = 52,
    SVCB = 64,
    HTTPS = 65,
    CAA = 257,
    ANY = 255,
    Unknown(u16),
}

impl DNSType {
    pub fn from_u16(val: u16) -> Self {
        match val {
            1 => DNSType::A,
            2 => DNSType::NS,
            5 => DNSType::CNAME,
            6 => DNSType::SOA,
            12 => DNSType::PTR,
            15 => DNSType::MX,
            16 => DNSType::TXT,
            28 => DNSType::AAAA,
            33 => DNSType::SRV,
            35 => DNSType::NAPTR,
            41 => DNSType::OPT,
            43 => DNSType::DS,
            46 => DNSType::RRSIG,
            47 => DNSType::NSEC,
            48 => DNSType::DNSKEY,
            50 => DNSType::NSEC3,
            52 => DNSType::TLSA,
            64 => DNSType::SVCB,
            65 => DNSType::HTTPS,
            257 => DNSType::CAA,
            255 => DNSType::ANY,
            other => DNSType::Unknown(other),
        }
    }

    pub fn to_u16(&self) -> u16 {
        match *self {
            DNSType::A => 1,
            DNSType::NS => 2,
            DNSType::CNAME => 5,
            DNSType::SOA => 6,
            DNSType::PTR => 12,
            DNSType::MX => 15,
            DNSType::TXT => 16,
            DNSType::AAAA => 28,
            DNSType::SRV => 33,
            DNSType::NAPTR => 35,
            DNSType::OPT => 41,
            DNSType::DS => 43,
            DNSType::RRSIG => 46,
            DNSType::NSEC => 47,
            DNSType::DNSKEY => 48,
            DNSType::NSEC3 => 50,
            DNSType::TLSA => 52,
            DNSType::SVCB => 64,
            DNSType::HTTPS => 65,
            DNSType::CAA => 257,
            DNSType::ANY => 255,
            DNSType::Unknown(val) => val,
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "A" => Some(DNSType::A),
            "NS" => Some(DNSType::NS),
            "CNAME" => Some(DNSType::CNAME),
            "SOA" => Some(DNSType::SOA),
            "PTR" => Some(DNSType::PTR),
            "MX" => Some(DNSType::MX),
            "TXT" => Some(DNSType::TXT),
            "AAAA" => Some(DNSType::AAAA),
            "SRV" => Some(DNSType::SRV),
            "NAPTR" => Some(DNSType::NAPTR),
            "OPT" => Some(DNSType::OPT),
            "DS" => Some(DNSType::DS),
            "RRSIG" => Some(DNSType::RRSIG),
            "NSEC" => Some(DNSType::NSEC),
            "DNSKEY" => Some(DNSType::DNSKEY),
            "NSEC3" => Some(DNSType::NSEC3),
            "TLSA" => Some(DNSType::TLSA),
            "SVCB" => Some(DNSType::SVCB),
            "HTTPS" => Some(DNSType::HTTPS),
            "CAA" => Some(DNSType::CAA),
            "ANY" => Some(DNSType::ANY),
            _ => None,
        }
    }
}

impl fmt::Display for DNSType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DNSType::A => write!(f, "A"),
            DNSType::NS => write!(f, "NS"),
            DNSType::CNAME => write!(f, "CNAME"),
            DNSType::SOA => write!(f, "SOA"),
            DNSType::PTR => write!(f, "PTR"),
            DNSType::MX => write!(f, "MX"),
            DNSType::TXT => write!(f, "TXT"),
            DNSType::AAAA => write!(f, "AAAA"),
            DNSType::SRV => write!(f, "SRV"),
            DNSType::NAPTR => write!(f, "NAPTR"),
            DNSType::OPT => write!(f, "OPT"),
            DNSType::DS => write!(f, "DS"),
            DNSType::RRSIG => write!(f, "RRSIG"),
            DNSType::NSEC => write!(f, "NSEC"),
            DNSType::DNSKEY => write!(f, "DNSKEY"),
            DNSType::NSEC3 => write!(f, "NSEC3"),
            DNSType::TLSA => write!(f, "TLSA"),
            DNSType::SVCB => write!(f, "SVCB"),
            DNSType::HTTPS => write!(f, "HTTPS"),
            DNSType::CAA => write!(f, "CAA"),
            DNSType::ANY => write!(f, "ANY"),
            DNSType::Unknown(code) => write!(f, "TYPE{}", code),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum DNSClass {
    IN = 1,
    CS = 2,
    CH = 3,
    HS = 4,
    ANY = 255,
    Unknown(u16),
}

impl DNSClass {
    pub fn from_u16(val: u16) -> Self {
        match val {
            1 => DNSClass::IN,
            2 => DNSClass::CS,
            3 => DNSClass::CH,
            4 => DNSClass::HS,
            255 => DNSClass::ANY,
            other => DNSClass::Unknown(other),
        }
    }

    pub fn to_u16(&self) -> u16 {
        match *self {
            DNSClass::IN => 1,
            DNSClass::CS => 2,
            DNSClass::CH => 3,
            DNSClass::HS => 4,
            DNSClass::ANY => 255,
            DNSClass::Unknown(val) => val,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RData {
    A(IPAddress),
    AAAA(IPAddress),
    CNAME(DNSName),
    NS(DNSName),
    PTR(DNSName),
    MX {
        priority: u16,
        exchange: DNSName,
    },
    TXT(Vec<String>),
    SOA {
        mname: DNSName,
        rname: DNSName,
        serial: u32,
        refresh: u32,
        retry: u32,
        expire: u32,
        minimum: u32,
    },
    SRV {
        priority: u16,
        weight: u16,
        port: u16,
        target: DNSName,
    },
    CAA {
        flags: u8,
        tag: String,
        value: String,
    },
    NAPTR {
        order: u16,
        preference: u16,
        flags: String,
        services: String,
        regexp: String,
        replacement: DNSName,
    },
    TLSA {
        cert_usage: u8,
        selector: u8,
        matching_type: u8,
        data: Vec<u8>,
    },
    SVCB {
        priority: u16,
        target: DNSName,
        params: Vec<(u16, Vec<u8>)>,
    },
    HTTPS {
        priority: u16,
        target: DNSName,
        params: Vec<(u16, Vec<u8>)>,
    },
    DNSKEY {
        flags: u16,
        protocol: u8,
        algorithm: u8,
        public_key: Vec<u8>,
    },
    DS {
        key_tag: u16,
        algorithm: u8,
        digest_type: u8,
        digest: Vec<u8>,
    },
    RRSIG {
        type_covered: u16,
        algorithm: u8,
        labels: u8,
        original_ttl: u32,
        expiration: u32,
        inception: u32,
        key_tag: u16,
        signer_name: DNSName,
        signature: Vec<u8>,
    },
    NSEC {
        next_domain: DNSName,
        types: Vec<u16>,
    },
    NSEC3 {
        hash_alg: u8,
        flags: u8,
        iterations: u16,
        salt: Vec<u8>,
        next_hashed_owner: Vec<u8>,
        types: Vec<u16>,
    },
    Unknown {
        type_code: u16,
        data: Vec<u8>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DNSRecord {
    pub name: DNSName,
    pub rtype: DNSType,
    pub rclass: DNSClass,
    pub ttl: u32,
    pub rdata: RData,
}
