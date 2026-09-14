//! AdeshLang DNS Standard Library API Registration

use super::records::{DNSType, RData};
use super::resolver::Resolver;
use super::service::ServiceDiscovery;
use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::stdlib_src::net::ip::IPAddress;
use crate::stdlib::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::sync::Arc;

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "dns_resolve",
        "dns",
        "Resolve a hostname to list of IP addresses",
        builtin_dns_resolve,
    );
    registry.register(
        "dns_query",
        "dns",
        "Query DNS records for a hostname and type",
        builtin_dns_query,
    );
    registry.register(
        "dns_reverse_lookup",
        "dns",
        "Perform reverse DNS lookup for an IP address",
        builtin_dns_reverse_lookup,
    );
    registry.register(
        "dns_discover",
        "dns",
        "Discover services using DNS SRV lookup",
        builtin_dns_discover,
    );
}

fn builtin_dns_resolve(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("DNS.resolve requires a hostname argument".to_string());
    }

    let hostname = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("DNS.resolve hostname argument must be a string".to_string()),
    };

    let resolver = Resolver::default_resolver();
    let ips = resolver.resolve(hostname)?;

    let mut result_array = Vec::new();
    for ip in ips {
        result_array.push(Value::Str(ip.to_canonical_string()));
    }

    Ok(Value::Array(result_array))
}

fn builtin_dns_query(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.len() < 2 {
        return Err("DNS.query requires hostname and record type arguments".to_string());
    }

    let hostname = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("DNS.query hostname must be a string".to_string()),
    };

    let rtype_str = match &args[1] {
        Value::Str(s) => s.as_str(),
        _ => return Err("DNS.query record type must be a string".to_string()),
    };

    let rtype = DNSType::parse(rtype_str)
        .ok_or_else(|| format!("Invalid or unsupported DNS record type '{}'", rtype_str))?;

    let resolver = Resolver::default_resolver();
    let records = resolver.query(hostname, rtype)?;

    let mut result_array = Vec::new();
    for rec in records {
        let mut rec_obj = FastMap::default();
        rec_obj.insert(
            "name".to_string(),
            Value::Str(rec.name.to_string_canonical()),
        );
        rec_obj.insert("type".to_string(), Value::Str(rec.rtype.to_string()));
        rec_obj.insert("ttl".to_string(), Value::Number(rec.ttl as f64));

        let data_val = match rec.rdata {
            RData::A(ip) | RData::AAAA(ip) => Value::Str(ip.to_canonical_string()),
            RData::CNAME(target) | RData::NS(target) | RData::PTR(target) => {
                Value::Str(target.to_string_canonical())
            }
            RData::MX { priority, exchange } => {
                let mut mx_obj = FastMap::default();
                mx_obj.insert("priority".to_string(), Value::Number(priority as f64));
                mx_obj.insert(
                    "exchange".to_string(),
                    Value::Str(exchange.to_string_canonical()),
                );
                Value::Object(Arc::new(mx_obj))
            }
            RData::SOA {
                mname,
                rname,
                serial,
                refresh,
                retry,
                expire,
                minimum,
            } => {
                let mut soa_obj = FastMap::default();
                soa_obj.insert(
                    "primaryNs".to_string(),
                    Value::Str(mname.to_string_canonical()),
                );
                soa_obj.insert(
                    "respMailbox".to_string(),
                    Value::Str(rname.to_string_canonical()),
                );
                soa_obj.insert("serial".to_string(), Value::Number(serial as f64));
                soa_obj.insert("refresh".to_string(), Value::Number(refresh as f64));
                soa_obj.insert("retry".to_string(), Value::Number(retry as f64));
                soa_obj.insert("expire".to_string(), Value::Number(expire as f64));
                soa_obj.insert("minimumTtl".to_string(), Value::Number(minimum as f64));
                Value::Object(Arc::new(soa_obj))
            }
            RData::CAA { flags, tag, value } => {
                let mut caa_obj = FastMap::default();
                caa_obj.insert("flags".to_string(), Value::Number(flags as f64));
                caa_obj.insert("tag".to_string(), Value::Str(tag));
                caa_obj.insert("value".to_string(), Value::Str(value));
                Value::Object(Arc::new(caa_obj))
            }
            RData::TXT(strings) => {
                let txts: Vec<Value> = strings.into_iter().map(Value::Str).collect();
                Value::Array(txts)
            }
            _ => Value::Str(format!("{:?}", rec.rdata)),
        };

        rec_obj.insert("data".to_string(), data_val);
        result_array.push(Value::Object(Arc::new(rec_obj)));
    }

    Ok(Value::Array(result_array))
}

fn builtin_dns_reverse_lookup(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    if args.is_empty() {
        return Err("DNS.reverseLookup requires an IP address argument".to_string());
    }

    let ip_str = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("DNS.reverseLookup IP argument must be a string".to_string()),
    };

    let ip = IPAddress::parse(ip_str)?;
    let resolver = Resolver::default_resolver();
    let hostname = resolver.reverse_lookup(&ip)?;

    Ok(Value::Str(hostname))
}

fn builtin_dns_discover(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    if args.is_empty() {
        return Err("DNS.discover requires a service name argument".to_string());
    }

    let service_name = match &args[0] {
        Value::Str(s) => s.as_str(),
        _ => return Err("DNS.discover service name must be a string".to_string()),
    };

    let resolver = Resolver::default_resolver();
    let endpoints = ServiceDiscovery::discover(service_name, &resolver)?;

    let mut result_array = Vec::new();
    for ep in endpoints {
        let mut ep_obj = FastMap::default();
        ep_obj.insert("host".to_string(), Value::Str(ep.host));
        ep_obj.insert("port".to_string(), Value::Number(ep.port as f64));
        ep_obj.insert("priority".to_string(), Value::Number(ep.priority as f64));
        ep_obj.insert("weight".to_string(), Value::Number(ep.weight as f64));
        result_array.push(Value::Object(Arc::new(ep_obj)));
    }

    Ok(Value::Array(result_array))
}

pub fn build_dns_module_object() -> Value {
    let mut dns_obj = FastMap::default();

    dns_obj.insert(
        "resolve".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dns_resolve))),
    );
    dns_obj.insert(
        "query".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dns_query))),
    );
    dns_obj.insert(
        "reverseLookup".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dns_reverse_lookup))),
    );
    dns_obj.insert(
        "discover".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_dns_discover))),
    );

    // DNS.Policy constructor helper with chainable builder methods
    dns_obj.insert(
        "Policy".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut map = FastMap::default();
            map.insert("deny_loopback".to_string(), Value::Bool(false));
            map.insert("deny_private".to_string(), Value::Bool(false));

            map.insert(
                "denyLoopback".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _args| {
                    let mut p_map = FastMap::default();
                    p_map.insert("deny_loopback".to_string(), Value::Bool(true));
                    p_map.insert("deny_private".to_string(), Value::Bool(false));
                    Ok(Value::Object(Arc::new(p_map)))
                }))),
            );

            map.insert(
                "denyPrivateNetworks".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _args| {
                    let mut p_map = FastMap::default();
                    p_map.insert("deny_loopback".to_string(), Value::Bool(false));
                    p_map.insert("deny_private".to_string(), Value::Bool(true));
                    Ok(Value::Object(Arc::new(p_map)))
                }))),
            );

            Ok(Value::Object(Arc::new(map)))
        }))),
    );

    // DNS.Resolver constructor helper with chainable builder methods
    dns_obj.insert(
        "Resolver".to_string(),
        Value::Function(NativeFn(Arc::new(|_, _| {
            let mut res_obj = FastMap::default();
            res_obj.insert(
                "query".to_string(),
                Value::Function(NativeFn(Arc::new(builtin_dns_query))),
            );
            res_obj.insert(
                "resolve".to_string(),
                Value::Function(NativeFn(Arc::new(builtin_dns_resolve))),
            );
            res_obj.insert(
                "servers".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| {
                    let mut res = FastMap::default();
                    res.insert(
                        "query".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_query))),
                    );
                    res.insert(
                        "resolve".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_resolve))),
                    );
                    Ok(Value::Object(Arc::new(res)))
                }))),
            );
            res_obj.insert(
                "timeout".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| {
                    let mut res = FastMap::default();
                    res.insert(
                        "query".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_query))),
                    );
                    res.insert(
                        "resolve".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_resolve))),
                    );
                    Ok(Value::Object(Arc::new(res)))
                }))),
            );
            res_obj.insert(
                "retries".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| {
                    let mut res = FastMap::default();
                    res.insert(
                        "query".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_query))),
                    );
                    res.insert(
                        "resolve".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_resolve))),
                    );
                    Ok(Value::Object(Arc::new(res)))
                }))),
            );
            res_obj.insert(
                "cache".to_string(),
                Value::Function(NativeFn(Arc::new(|_, _| {
                    let mut res = FastMap::default();
                    res.insert(
                        "query".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_query))),
                    );
                    res.insert(
                        "resolve".to_string(),
                        Value::Function(NativeFn(Arc::new(builtin_dns_resolve))),
                    );
                    Ok(Value::Object(Arc::new(res)))
                }))),
            );

            Ok(Value::Object(Arc::new(res_obj)))
        }))),
    );

    let mut type_obj = FastMap::default();
    type_obj.insert("A".to_string(), Value::Str("A".to_string()));
    type_obj.insert("AAAA".to_string(), Value::Str("AAAA".to_string()));
    type_obj.insert("CNAME".to_string(), Value::Str("CNAME".to_string()));
    type_obj.insert("MX".to_string(), Value::Str("MX".to_string()));
    type_obj.insert("TXT".to_string(), Value::Str("TXT".to_string()));
    type_obj.insert("NS".to_string(), Value::Str("NS".to_string()));
    type_obj.insert("PTR".to_string(), Value::Str("PTR".to_string()));
    type_obj.insert("SOA".to_string(), Value::Str("SOA".to_string()));
    type_obj.insert("SRV".to_string(), Value::Str("SRV".to_string()));
    type_obj.insert("CAA".to_string(), Value::Str("CAA".to_string()));

    dns_obj.insert("Type".to_string(), Value::Object(Arc::new(type_obj)));

    Value::Object(Arc::new(dns_obj))
}
