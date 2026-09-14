//! Service Discovery abstraction over SRV and TXT records.

use super::records::{DNSType, RData};
use super::resolver::Resolver;

#[derive(Debug, Clone)]
pub struct ServiceEndpoint {
    pub host: String,
    pub port: u16,
    pub priority: u16,
    pub weight: u16,
}

pub struct ServiceDiscovery;

impl ServiceDiscovery {
    pub fn discover(
        service_name: &str,
        resolver: &Resolver,
    ) -> Result<Vec<ServiceEndpoint>, String> {
        let records = resolver.query(service_name, DNSType::SRV)?;
        let mut endpoints = Vec::new();

        for rec in records {
            if let RData::SRV {
                priority,
                weight,
                port,
                target,
            } = rec.rdata
            {
                endpoints.push(ServiceEndpoint {
                    host: target.to_string_canonical(),
                    port,
                    priority,
                    weight,
                });
            }
        }

        if endpoints.is_empty() {
            return Err(format!(
                "No SRV service endpoints discovered for '{}'",
                service_name
            ));
        }

        // Sort by priority ascending
        endpoints.sort_by_key(|e| e.priority);
        Ok(endpoints)
    }
}
