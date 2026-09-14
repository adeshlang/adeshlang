use super::name::DNSName;
use super::policy::DNSPolicy;
use super::records::{DNSClass, DNSRecord, DNSType, RData};
use super::wire::{DNSWireDecoder, DNSWireEncoder};
use crate::runtime::stdlib_src::net::ip::IPAddress;

#[test]
fn test_dns_name_parsing() {
    let name = DNSName::parse("example.com.").unwrap();
    assert_eq!(name.to_string_canonical(), "example.com");
    assert_eq!(name.to_fqdn(), "example.com.");
    assert_eq!(name.labels().len(), 2);

    let root = DNSName::parse(".").unwrap();
    assert!(root.is_root());
    assert_eq!(root.to_string_canonical(), ".");

    assert!(DNSName::parse("").is_err());
    assert!(DNSName::parse("a..b.com").is_err());
}

#[test]
fn test_wire_format_encode_decode_roundtrip() {
    let name = DNSName::parse("test.example.com").unwrap();
    let record = DNSRecord {
        name: name.clone(),
        rtype: DNSType::A,
        rclass: DNSClass::IN,
        ttl: 300,
        rdata: RData::A(IPAddress::parse("192.168.1.1").unwrap()),
    };

    let msg = crate::runtime::stdlib_src::dns::message::DNSMessage {
        header: crate::runtime::stdlib_src::dns::message::DNSHeader {
            id: 0x1234,
            qr: true,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true,
            ra: true,
            z: false,
            ad: false,
            cd: false,
            rcode: 0,
            qdcount: 1,
            ancount: 1,
            nscount: 0,
            arcount: 0,
        },
        questions: vec![crate::runtime::stdlib_src::dns::message::DNSQuestion {
            name,
            qtype: DNSType::A,
            qclass: DNSClass::IN,
        }],
        answers: vec![record],
        authorities: Vec::new(),
        additionals: Vec::new(),
    };

    let wire_bytes = DNSWireEncoder::encode(&msg).unwrap();
    let decoded = DNSWireDecoder::decode(&wire_bytes).unwrap();

    assert_eq!(decoded.header.id, 0x1234);
    assert_eq!(decoded.answers.len(), 1);
    assert_eq!(decoded.answers[0].ttl, 300);
}

#[test]
fn test_security_policy_validation() {
    let policy = DNSPolicy::new().deny_loopback().deny_private_networks();

    let loopback = IPAddress::parse("127.0.0.1").unwrap();
    assert!(policy.validate_ip(&loopback).is_err());

    let private_ip = IPAddress::parse("192.168.1.100").unwrap();
    assert!(policy.validate_ip(&private_ip).is_err());

    let public_ip = IPAddress::parse("8.8.8.8").unwrap();
    assert!(policy.validate_ip(&public_ip).is_ok());
}
