#![allow(dead_code)]

//! DNS Resolver Core Implementation.

use super::cache::DNSCache;
use super::message::DNSMessage;
use super::name::DNSName;
use super::policy::DNSPolicy;
use super::records::{DNSRecord, DNSType, RData};
use super::transport::DNSTransport;
use crate::runtime::stdlib_src::net::ip::IPAddress;
use std::sync::Arc;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::Duration;

static NEXT_TX_ID: AtomicU16 = AtomicU16::new(100);

pub struct ResolverConfig {
    pub servers: Vec<String>,
    pub timeout: Duration,
    pub retries: usize,
    pub use_cache: bool,
    pub search_domains: Vec<String>,
    pub policy: DNSPolicy,
}

impl Default for ResolverConfig {
    fn default() -> Self {
        Self {
            servers: vec!["1.1.1.1".to_string(), "8.8.8.8".to_string()],
            timeout: Duration::from_secs(3),
            retries: 2,
            use_cache: true,
            search_domains: Vec::new(),
            policy: DNSPolicy::default(),
        }
    }
}

pub struct Resolver {
    config: ResolverConfig,
    cache: Arc<DNSCache>,
}

impl Resolver {
    pub fn new(config: ResolverConfig) -> Self {
        Self {
            config,
            cache: Arc::new(DNSCache::new(1024)),
        }
    }

    pub fn default_resolver() -> Self {
        Self::new(ResolverConfig::default())
    }

    pub fn query(&self, domain: &str, rtype: DNSType) -> Result<Vec<DNSRecord>, String> {
        let domain_lower = domain.to_lowercase();
        if domain_lower == "localhost" || domain_lower == "127.0.0.1" || domain_lower == "::1" {
            let name = DNSName::parse(domain)?;
            if rtype == DNSType::A {
                if let Ok(ip) = IPAddress::parse("127.0.0.1") {
                    return Ok(vec![DNSRecord {
                        name,
                        rtype: DNSType::A,
                        rclass: super::records::DNSClass::IN,
                        ttl: 300,
                        rdata: RData::A(ip),
                    }]);
                }
            } else if rtype == DNSType::AAAA {
                if let Ok(ip) = IPAddress::parse("::1") {
                    return Ok(vec![DNSRecord {
                        name,
                        rtype: DNSType::AAAA,
                        rclass: super::records::DNSClass::IN,
                        ttl: 300,
                        rdata: RData::AAAA(ip),
                    }]);
                }
            }
        }

        let name = DNSName::parse(domain)?;

        // Check local cache if enabled
        if self.config.use_cache {
            if let Some(cached_records) = self.cache.get(&name, rtype) {
                return Ok(cached_records);
            }
        }

        let id = NEXT_TX_ID.fetch_add(1, Ordering::SeqCst);
        let msg = DNSMessage::new_query(name.clone(), rtype, id);

        let mut last_err = String::new();

        for server in &self.config.servers {
            for _attempt in 0..=self.config.retries {
                match DNSTransport::query_udp(server, &msg, self.config.timeout) {
                    Ok(resp) => {
                        // Apply security policy validation to IP records
                        let mut filtered_answers = Vec::new();
                        for rec in &resp.answers {
                            match &rec.rdata {
                                RData::A(ip) | RData::AAAA(ip) => {
                                    if let Err(e) = self.config.policy.validate_ip(ip) {
                                        return Err(e);
                                    }
                                }
                                _ => {}
                            }
                            filtered_answers.push(rec.clone());
                        }

                        let mut final_msg = resp.clone();
                        final_msg.answers = filtered_answers.clone();

                        if self.config.use_cache {
                            self.cache.put(name.clone(), rtype, &final_msg);
                        }

                        return Ok(filtered_answers);
                    }
                    Err(e) => {
                        last_err = e;
                    }
                }
            }
        }

        Err(format!(
            "DNS query for '{}' (type {}) failed after retries: {}",
            domain, rtype, last_err
        ))
    }

    pub fn resolve(&self, domain: &str) -> Result<Vec<IPAddress>, String> {
        let mut ips = Vec::new();

        // Query A records
        if let Ok(a_records) = self.query(domain, DNSType::A) {
            for rec in a_records {
                if let RData::A(ip) = rec.rdata {
                    ips.push(ip);
                }
            }
        }

        // Query AAAA records
        if let Ok(aaaa_records) = self.query(domain, DNSType::AAAA) {
            for rec in aaaa_records {
                if let RData::AAAA(ip) = rec.rdata {
                    ips.push(ip);
                }
            }
        }

        if ips.is_empty() {
            Err(format!(
                "Could not resolve hostname '{}' to any IP address",
                domain
            ))
        } else {
            Ok(ips)
        }
    }

    pub fn reverse_lookup(&self, ip: &IPAddress) -> Result<String, String> {
        let arpa_domain = match ip {
            IPAddress::V4(addr) => {
                let octets = addr.octets();
                format!(
                    "{}.{}.{}.{}.in-addr.arpa",
                    octets[3], octets[2], octets[1], octets[0]
                )
            }
            IPAddress::V6(addr, _) => {
                let octets = addr.octets();
                let mut nibbles = Vec::new();
                for b in octets.iter().rev() {
                    nibbles.push(format!("{:x}", b & 0x0F));
                    nibbles.push(format!("{:x}", (b >> 4) & 0x0F));
                }
                format!("{}.ip6.arpa", nibbles.join("."))
            }
        };

        let records = self.query(&arpa_domain, DNSType::PTR)?;
        for rec in records {
            if let RData::PTR(name) = rec.rdata {
                return Ok(name.to_string_canonical());
            }
        }

        Err(format!("No PTR record found for IP address '{:?}'", ip))
    }
}
