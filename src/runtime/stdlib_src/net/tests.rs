//! Unit tests for AdeshLang Net Standard Library

#[cfg(test)]
mod tests {
    use super::super::ip::IPAddress;
    use super::super::policy::NetworkPolicy;
    use super::super::socket_addr::SocketAddress;

    #[test]
    fn test_ipv4_parsing_and_classification() {
        let ip = IPAddress::parse("127.0.0.1").unwrap();
        assert!(ip.is_v4());
        assert!(ip.is_loopback());
        assert!(!ip.is_private());

        let private_ip = IPAddress::parse("192.168.1.100").unwrap();
        assert!(private_ip.is_private());
        assert!(!private_ip.is_loopback());

        let doc_ip = IPAddress::parse("192.0.2.1").unwrap();
        assert!(doc_ip.is_documentation());
    }

    #[test]
    fn test_ipv6_parsing_and_zone() {
        let ip = IPAddress::parse("::1").unwrap();
        assert!(ip.is_v6());
        assert!(ip.is_loopback());

        let zone_ip = IPAddress::parse("fe80::1%eth0").unwrap();
        assert!(zone_ip.is_v6());
        assert!(zone_ip.is_link_local());
        assert_eq!(zone_ip.to_canonical_string(), "fe80::1%eth0");
    }

    #[test]
    fn test_socket_address_parsing() {
        let sa4 = SocketAddress::parse("127.0.0.1:8080").unwrap();
        assert_eq!(sa4.port, 8080);
        assert_eq!(sa4.to_canonical_string(), "127.0.0.1:8080");

        let sa6 = SocketAddress::parse("[::1]:443").unwrap();
        assert_eq!(sa6.port, 443);
        assert_eq!(sa6.to_canonical_string(), "[::1]:443");

        // Unbracketed IPv6 with port should fail
        assert!(SocketAddress::parse("2001:db8::1:8080").is_err());
    }

    #[test]
    fn test_network_policy_validation() {
        let policy = NetworkPolicy::new().deny_private_networks().deny_loopback();

        let loopback = IPAddress::parse("127.0.0.1").unwrap();
        assert!(policy.validate_address(&loopback).is_err());

        let private_ip = IPAddress::parse("10.0.0.1").unwrap();
        assert!(policy.validate_address(&private_ip).is_err());

        let public_ip = IPAddress::parse("8.8.8.8").unwrap();
        assert!(policy.validate_address(&public_ip).is_ok());
    }
}
