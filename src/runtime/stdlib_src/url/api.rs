//! Native Function Callbacks & Module Object Assembly for AdeshLang URL.

use crate::parsing::ast::{BuiltinEnv, NativeFn, Value};
use crate::runtime::stdlib_src::registry::BuiltinRegistry;
use crate::utils::collections::FastMap;
use std::collections::HashMap;
use std::sync::Arc;

use super::builder::URLBuilder;
use super::idna_punycode;
use super::pattern::URLPattern;
use super::security::{URLSecurityPolicy, security_report, url_diff};
use super::special_urls::{DataURL, from_file_path, to_file_path};
use super::template::URLTemplate;
use super::url_object::URL;
use super::view::URLView;

pub fn register_all(registry: &mut BuiltinRegistry) {
    registry.register(
        "URL.parse",
        "url",
        "Parse a URL string into a URL object",
        builtin_url_parse,
    );
    registry.register(
        "URL.parseRelative",
        "url",
        "Parse a relative URL against base",
        builtin_url_parse_relative,
    );
    registry.register(
        "URL.builder",
        "url",
        "Create a new URLBuilder",
        builtin_url_builder,
    );
    registry.register(
        "URL.resolve",
        "url",
        "Resolve relative URL string against base URL",
        builtin_url_resolve,
    );
    registry.register(
        "URL.join",
        "url",
        "Join base URL with relative path",
        builtin_url_join,
    );
    registry.register(
        "URL.pattern",
        "url",
        "Create a new URLPattern matcher",
        builtin_url_pattern,
    );
    registry.register(
        "URL.template",
        "url",
        "Create a new URLTemplate expander",
        builtin_url_template,
    );
    registry.register(
        "URL.SecurityPolicy",
        "url",
        "Create a new URLSecurityPolicy",
        builtin_url_security_policy,
    );
    registry.register(
        "URL.fromFilePath",
        "url",
        "Convert file system path to file:// URL",
        builtin_url_from_file_path,
    );
    registry.register(
        "URL.toFilePath",
        "url",
        "Convert file:// URL to file system path",
        builtin_url_to_file_path,
    );
    registry.register(
        "URL.parseData",
        "url",
        "Parse data: URL into DataURL structure",
        builtin_url_parse_data,
    );
    registry.register(
        "URL.parseView",
        "url",
        "Parse URL string zero-copy into URLView",
        builtin_url_parse_view,
    );
    registry.register(
        "URL.domainToAscii",
        "url",
        "Convert domain string to IDNA ASCII Punycode",
        builtin_url_domain_to_ascii,
    );
    registry.register(
        "URL.domainToUnicode",
        "url",
        "Convert IDNA ASCII Punycode domain to Unicode",
        builtin_url_domain_to_unicode,
    );
    registry.register(
        "URL.diff",
        "url",
        "Compare two URL objects and return differences",
        builtin_url_diff,
    );
    registry.register(
        "URL.securityReport",
        "url",
        "Generate security findings for a URL",
        builtin_url_security_report,
    );
    registry.register(
        "URL.isValid",
        "url",
        "Check if URL string is valid",
        builtin_url_is_valid,
    );
    registry.register(
        "URL.isAbsolute",
        "url",
        "Check if URL string is absolute",
        builtin_url_is_absolute,
    );
    registry.register(
        "URL.isRelative",
        "url",
        "Check if URL string is relative",
        builtin_url_is_relative,
    );
    registry.register(
        "URL.percentEncode",
        "url",
        "Percent encode string",
        builtin_url_percent_encode,
    );
    registry.register(
        "URL.percentDecode",
        "url",
        "Percent decode string",
        builtin_url_percent_decode,
    );
    registry.register(
        "URL.formEncode",
        "url",
        "Form URL encode string",
        builtin_url_form_encode,
    );
    registry.register(
        "URL.formDecode",
        "url",
        "Form URL decode string",
        builtin_url_form_decode,
    );
    registry.register(
        "URL.toDataUrl",
        "url",
        "Create a data: URL string",
        builtin_url_to_data_url,
    );
}

pub fn url_to_value(url: &URL) -> Value {
    let mut map = FastMap::default();

    let u = url.clone();
    map.insert("scheme".to_string(), Value::Str(url.scheme().to_string()));
    map.insert(
        "username".to_string(),
        url.username()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "password".to_string(),
        url.password()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "host".to_string(),
        url.host()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "hostname".to_string(),
        url.hostname()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "port".to_string(),
        url.port()
            .map(|p| Value::Number(p as f64))
            .unwrap_or(Value::Null),
    );
    map.insert("path".to_string(), Value::Str(url.path().to_string()));
    map.insert("query".to_string(), Value::Str(url.query()));
    map.insert(
        "fragment".to_string(),
        url.fragment()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert("origin".to_string(), Value::Str(url.origin()));
    map.insert("href".to_string(), Value::Str(url.href()));
    map.insert("authority".to_string(), Value::Str(url.authority()));

    // Getter methods
    let u_clone = u.clone();
    map.insert(
        "getScheme".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.scheme().to_string()))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getUsername".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(u_clone
                .username()
                .map(|s| Value::Str(s.to_string()))
                .unwrap_or(Value::Null))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getPassword".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(u_clone
                .password()
                .map(|s| Value::Str(s.to_string()))
                .unwrap_or(Value::Null))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getHost".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(u_clone
                .host()
                .map(|s| Value::Str(s.to_string()))
                .unwrap_or(Value::Null))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getHostname".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(u_clone
                .hostname()
                .map(|s| Value::Str(s.to_string()))
                .unwrap_or(Value::Null))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getPort".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(u_clone
                .port()
                .map(|p| Value::Number(p as f64))
                .unwrap_or(Value::Null))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getPath".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.path().to_string()))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getQuery".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.query()))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getFragment".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(u_clone
                .fragment()
                .map(|s| Value::Str(s.to_string()))
                .unwrap_or(Value::Null))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getOrigin".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.origin()))
        }))),
    );
    let u_clone = u.clone();
    map.insert(
        "getHref".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.href()))
        }))),
    );

    // Methods
    let u_clone = u.clone();
    map.insert(
        "toString".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.to_string()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "redacted".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.redacted()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "safeString".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Str(u_clone.safe_string()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "resolve".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let rel = args
                .get(0)
                .and_then(get_str)
                .ok_or("url.resolve requires (relativeUrlString)")?;
            let res = u_clone.resolve(rel)?;
            Ok(url_to_value(&res))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "canonicalize".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let c = u_clone.canonicalize()?;
            Ok(url_to_value(&c))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "stripCredentials".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(url_to_value(&u_clone.strip_credentials()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "stripFragment".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(url_to_value(&u_clone.strip_fragment()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "stripTrackingParameters".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(url_to_value(&u_clone.strip_tracking_parameters()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "isHttp".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Bool(u_clone.is_http()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "isHttps".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Bool(u_clone.is_https()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "isWebSocket".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Bool(u_clone.is_web_socket()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "isSecure".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(Value::Bool(u_clone.is_secure()))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "edit".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(builder_to_value(u_clone.edit()))
        }))),
    );

    pub fn value_to_str(val: &Value) -> String {
        match val {
            Value::Str(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::Char(c) => c.to_string(),
            Value::BigInt(b) => b.to_string(),
            Value::U8(n) => n.to_string(),
            Value::U16(n) => n.to_string(),
            Value::U32(n) => n.to_string(),
            Value::U64(n) => n.to_string(),
            Value::U128(n) => n.to_string(),
            Value::I8(n) => n.to_string(),
            Value::I16(n) => n.to_string(),
            Value::I32(n) => n.to_string(),
            Value::I64(n) => n.to_string(),
            Value::I128(n) => n.to_string(),
            _ => format!("{:?}", val),
        }
    }

    let u_clone = u.clone();
    map.insert(
        "withQueryParam".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let k = args
                .get(0)
                .and_then(get_str)
                .ok_or("withQueryParam requires (key, value)")?;
            let v = match args.get(1) {
                Some(val) => value_to_str(val),
                None => return Err("withQueryParam requires (key, value)".to_string()),
            };
            Ok(url_to_value(&u_clone.with_query_param(k, &v)))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "removeQueryParam".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let k = args
                .get(0)
                .and_then(get_str)
                .ok_or("removeQueryParam requires (key)")?;
            Ok(url_to_value(&u_clone.remove_query_param(k)))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "appendPathSegment".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let seg = args
                .get(0)
                .and_then(get_str)
                .ok_or("appendPathSegment requires (segment)")?;
            Ok(url_to_value(&u_clone.append_path_segment(seg)))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "toWebSocketUrl".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let ws = super::special_urls::to_web_socket_url(&u_clone)?;
            Ok(url_to_value(&ws))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "toHttpUrl".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let http = super::special_urls::to_http_url(&u_clone)?;
            Ok(url_to_value(&http))
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "sameOrigin".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            if let Some(other_url) =
                value_to_url(args.get(0).ok_or("sameOrigin requires (otherUrl)")?)
            {
                Ok(Value::Bool(u_clone.same_origin(&other_url)))
            } else {
                Ok(Value::Bool(false))
            }
        }))),
    );

    let u_clone = u.clone();
    map.insert(
        "pathSegments".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let segs: Vec<Value> = u_clone
                .path_segments()
                .into_iter()
                .map(Value::Str)
                .collect();
            Ok(Value::Array(segs))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn value_to_url(val: &Value) -> Option<URL> {
    match val {
        Value::Str(s) => URL::parse(s).ok(),
        Value::Object(obj) => {
            if let Some(Value::Str(href)) = obj.get("href") {
                URL::parse(href).ok()
            } else {
                None
            }
        }
        _ => None,
    }
}

fn get_str(val: &Value) -> Option<&str> {
    match val {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

pub fn builtin_url_parse(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.parse requires (urlString)")?;
    let url = URL::parse(input)?;
    Ok(url_to_value(&url))
}

pub fn builtin_url_parse_relative(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.parseRelative requires (relativeUrlString, baseUrl)")?;
    let base_val = args
        .get(1)
        .ok_or("URL.parseRelative requires base URL as second argument")?;
    let base_url =
        value_to_url(base_val).ok_or("Invalid base URL provided to URL.parseRelative")?;
    let resolved = URL::parse_relative(input, &base_url)?;
    Ok(url_to_value(&resolved))
}

pub fn builtin_url_builder(_env: &mut dyn BuiltinEnv, _args: Vec<Value>) -> Result<Value, String> {
    let builder = URLBuilder::new();
    Ok(builder_to_value(builder))
}

fn builder_to_value(builder: URLBuilder) -> Value {
    let mut map = FastMap::default();
    let b = builder.clone();

    let b_clone = b.clone();
    map.insert(
        "scheme".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let s = args
                .get(0)
                .and_then(get_str)
                .ok_or("scheme requires (string)")?;
            Ok(builder_to_value(b_clone.clone().scheme(s)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "host".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let h = args
                .get(0)
                .and_then(get_str)
                .ok_or("host requires (string)")?;
            Ok(builder_to_value(b_clone.clone().host(h)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "port".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let p = match args.get(0) {
                Some(Value::Number(n)) => *n as u16,
                _ => return Err("port requires number argument".to_string()),
            };
            Ok(builder_to_value(b_clone.clone().port(p)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "path".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let p = args
                .get(0)
                .and_then(get_str)
                .ok_or("path requires (string)")?;
            Ok(builder_to_value(b_clone.clone().path(p)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "appendPathSegment".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let seg = args
                .get(0)
                .and_then(get_str)
                .ok_or("appendPathSegment requires (string)")?;
            Ok(builder_to_value(b_clone.clone().append_path_segment(seg)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "queryParam".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let k = args
                .get(0)
                .and_then(get_str)
                .ok_or("queryParam requires (key, value)")?;
            let v = match args.get(1) {
                Some(Value::Str(s)) => s.clone(),
                Some(Value::Number(n)) => n.to_string(),
                Some(Value::Bool(b)) => b.to_string(),
                _ => return Err("queryParam value must be string/number/bool".to_string()),
            };
            Ok(builder_to_value(b_clone.clone().query_param(k, &v)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "fragment".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let f = args
                .get(0)
                .and_then(get_str)
                .ok_or("fragment requires (string)")?;
            Ok(builder_to_value(b_clone.clone().fragment(f)))
        }))),
    );

    let b_clone = b.clone();
    map.insert(
        "build".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let url = b_clone.clone().build()?;
            Ok(url_to_value(&url))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn builtin_url_resolve(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let base_val = args
        .get(0)
        .ok_or("URL.resolve requires (baseUrl, relativeUrlString)")?;
    let rel_input = args
        .get(1)
        .and_then(get_str)
        .ok_or("URL.resolve requires relative URL string as second argument")?;
    let base_url = value_to_url(base_val).ok_or("Invalid base URL provided to URL.resolve")?;
    let resolved = base_url.resolve(rel_input)?;
    Ok(url_to_value(&resolved))
}

pub fn builtin_url_join(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    builtin_url_resolve(_env, args)
}

pub fn builtin_url_pattern(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let pattern_str = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.pattern requires (patternString)")?;
    let pat = URLPattern::parse(pattern_str)?;
    let pat_clone = pat.clone();

    let mut map = FastMap::default();
    map.insert(
        "patternString".to_string(),
        Value::Str(pat.pattern_string().to_string()),
    );

    map.insert(
        "match".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let target_val = args.get(0).ok_or("pattern.match requires (targetUrl)")?;
            if let Some(target_url) = value_to_url(target_val) {
                if let Some(m) = pat_clone.match_url(&target_url) {
                    let mut match_map = FastMap::default();
                    let mut params_map = FastMap::default();
                    for (k, v) in m.params() {
                        params_map.insert(k.clone(), Value::Str(v.clone()));
                    }
                    match_map.insert("params".to_string(), Value::Object(Arc::new(params_map)));
                    Ok(Value::Object(Arc::new(match_map)))
                } else {
                    Ok(Value::Null)
                }
            } else {
                Ok(Value::Null)
            }
        }))),
    );

    Ok(Value::Object(Arc::new(map)))
}

pub fn builtin_url_template(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let tmpl_str = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.template requires (templateString)")?;
    let tmpl = URLTemplate::parse(tmpl_str);
    let tmpl_clone = tmpl.clone();

    let mut map = FastMap::default();
    map.insert(
        "templateString".to_string(),
        Value::Str(tmpl.template_string().to_string()),
    );

    map.insert(
        "expand".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let map_val = args
                .get(0)
                .ok_or("template.expand requires (parametersObject)")?;
            let mut params = HashMap::new();
            if let Value::Object(obj) = map_val {
                for (k, v) in obj.iter() {
                    if let Value::Str(s) = v {
                        params.insert(k.clone(), s.clone());
                    } else if let Value::Number(n) = v {
                        params.insert(k.clone(), n.to_string());
                    } else if let Value::Bool(b) = v {
                        params.insert(k.clone(), b.to_string());
                    }
                }
            }
            let url = tmpl_clone.expand(&params)?;
            Ok(url_to_value(&url))
        }))),
    );

    Ok(Value::Object(Arc::new(map)))
}

pub fn builtin_url_security_policy(
    _env: &mut dyn BuiltinEnv,
    _args: Vec<Value>,
) -> Result<Value, String> {
    let policy = URLSecurityPolicy::new();
    Ok(policy_to_value(policy))
}

fn policy_to_value(policy: URLSecurityPolicy) -> Value {
    let mut map = FastMap::default();
    let p = policy.clone();

    let p_clone = p.clone();
    map.insert(
        "allowScheme".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let s = args
                .get(0)
                .and_then(get_str)
                .ok_or("allowScheme requires (string)")?;
            Ok(policy_to_value(p_clone.clone().allow_scheme(s)))
        }))),
    );

    let p_clone = p.clone();
    map.insert(
        "denyPrivateNetworks".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(policy_to_value(p_clone.clone().deny_private_networks()))
        }))),
    );

    let p_clone = p.clone();
    map.insert(
        "denyCredentials".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            Ok(policy_to_value(p_clone.clone().deny_credentials()))
        }))),
    );

    let p_clone = p.clone();
    map.insert(
        "validate".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, args| {
            let target_val = args.get(0).ok_or("policy.validate requires (targetUrl)")?;
            let target_url =
                value_to_url(target_val).ok_or("Invalid URL passed to policy.validate")?;
            p_clone.validate(&target_url)?;
            Ok(Value::Bool(true))
        }))),
    );

    Value::Object(Arc::new(map))
}

pub fn builtin_url_from_file_path(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let path_str = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.fromFilePath requires (pathString)")?;
    let url = from_file_path(path_str)?;
    Ok(url_to_value(&url))
}

pub fn builtin_url_to_file_path(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let url_val = args.get(0).ok_or("URL.toFilePath requires (urlObject)")?;
    let url = value_to_url(url_val).ok_or("Invalid URL object passed to URL.toFilePath")?;
    let fp = to_file_path(&url)?;
    Ok(Value::Str(fp))
}

pub fn builtin_url_parse_data(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.parseData requires (dataUrlString)")?;
    let data_url = DataURL::parse(input)?;
    let mut map = FastMap::default();
    map.insert(
        "mediaType".to_string(),
        Value::Str(data_url.media_type.clone()),
    );
    map.insert("isBase64".to_string(), Value::Bool(data_url.is_base64));
    map.insert("rawData".to_string(), Value::Str(data_url.raw_data.clone()));

    let data_url_clone = data_url.clone();
    map.insert(
        "decodeString".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let decoded = data_url_clone.decode_string()?;
            Ok(Value::Str(decoded))
        }))),
    );

    Ok(Value::Object(Arc::new(map)))
}

pub fn builtin_url_parse_view(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = match args.get(0) {
        Some(Value::Str(s)) => s.clone(),
        _ => return Err("URL.parseView requires (urlString)".to_string()),
    };
    let view = URLView::parse(&input)?;
    let mut map = FastMap::default();

    map.insert("scheme".to_string(), Value::Str(view.scheme().to_string()));
    map.insert(
        "host".to_string(),
        view.host()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "port".to_string(),
        view.port_str()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert("path".to_string(), Value::Str(view.path().to_string()));
    map.insert(
        "query".to_string(),
        view.query()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "fragment".to_string(),
        view.fragment()
            .map(|s| Value::Str(s.to_string()))
            .unwrap_or(Value::Null),
    );

    let input_clone = input.clone();
    map.insert(
        "toOwned".to_string(),
        Value::Function(NativeFn(Arc::new(move |_, _| {
            let url = URL::parse(&input_clone)?;
            Ok(url_to_value(&url))
        }))),
    );

    Ok(Value::Object(Arc::new(map)))
}

pub fn builtin_url_domain_to_ascii(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let domain = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.domainToAscii requires (domainString)")?;
    let ascii = idna_punycode::domain_to_ascii(domain)?;
    Ok(Value::Str(ascii))
}

pub fn builtin_url_domain_to_unicode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let domain = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.domainToUnicode requires (domainString)")?;
    let unicode = idna_punycode::domain_to_unicode(domain)?;
    Ok(Value::Str(unicode))
}

pub fn builtin_url_diff(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let url_a = value_to_url(args.get(0).ok_or("URL.diff requires (urlA, urlB)")?)
        .ok_or("Invalid first URL in URL.diff")?;
    let url_b = value_to_url(args.get(1).ok_or("URL.diff requires (urlA, urlB)")?)
        .ok_or("Invalid second URL in URL.diff")?;
    let diffs: Vec<Value> = url_diff(&url_a, &url_b)
        .into_iter()
        .map(Value::Str)
        .collect();
    Ok(Value::Array(diffs))
}

pub fn builtin_url_security_report(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let url = value_to_url(
        args.get(0)
            .ok_or("URL.securityReport requires (urlObject)")?,
    )
    .ok_or("Invalid URL in URL.securityReport")?;
    let report: Vec<Value> = security_report(&url).into_iter().map(Value::Str).collect();
    Ok(Value::Array(report))
}

pub fn builtin_url_is_valid(_env: &mut dyn BuiltinEnv, args: Vec<Value>) -> Result<Value, String> {
    let input = args.get(0).and_then(get_str).unwrap_or("");
    Ok(Value::Bool(URL::parse(input).is_ok()))
}

pub fn builtin_url_is_absolute(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args.get(0).and_then(get_str).unwrap_or("");
    if let Ok(url) = URL::parse(input) {
        Ok(Value::Bool(!url.scheme().is_empty()))
    } else {
        Ok(Value::Bool(false))
    }
}

pub fn builtin_url_is_relative(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args.get(0).and_then(get_str).unwrap_or("");
    Ok(Value::Bool(
        !input.contains("://") && !input.starts_with("data:") && !input.starts_with("mailto:"),
    ))
}

pub fn builtin_url_percent_encode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.percentEncode requires (string)")?;
    Ok(Value::Str(
        crate::runtime::stdlib_src::encoding::percent::percent_encode(input),
    ))
}

pub fn builtin_url_percent_decode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.percentDecode requires (string)")?;
    let decoded = crate::runtime::stdlib_src::encoding::percent::percent_decode(input)?;
    Ok(Value::Str(decoded))
}

pub fn builtin_url_form_encode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.formEncode requires (string)")?;
    Ok(Value::Str(
        crate::runtime::stdlib_src::encoding::percent::form_encode(input),
    ))
}

pub fn builtin_url_form_decode(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let input = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.formDecode requires (string)")?;
    let decoded = crate::runtime::stdlib_src::encoding::percent::form_decode(input)?;
    Ok(Value::Str(decoded))
}

pub fn builtin_url_to_data_url(
    _env: &mut dyn BuiltinEnv,
    args: Vec<Value>,
) -> Result<Value, String> {
    let media_type = args
        .get(0)
        .and_then(get_str)
        .ok_or("URL.toDataUrl requires (mediaType, payload)")?;
    let payload = args
        .get(1)
        .and_then(get_str)
        .ok_or("URL.toDataUrl requires payload string as second argument")?;
    let is_base64 = args
        .get(2)
        .and_then(|v| match v {
            Value::Bool(b) => Some(*b),
            _ => None,
        })
        .unwrap_or(true);
    let data_str = super::special_urls::create_data_url(media_type, payload, is_base64);
    Ok(Value::Str(data_str))
}

pub fn build_url_module_object() -> Value {
    let mut map = FastMap::default();

    map.insert(
        "parse".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_parse))),
    );
    map.insert(
        "parseRelative".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_parse_relative))),
    );
    map.insert(
        "builder".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_builder))),
    );
    map.insert(
        "resolve".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_resolve))),
    );
    map.insert(
        "join".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_join))),
    );
    map.insert(
        "pattern".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_pattern))),
    );
    map.insert(
        "template".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_template))),
    );
    map.insert(
        "SecurityPolicy".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_security_policy))),
    );
    map.insert(
        "fromFilePath".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_from_file_path))),
    );
    map.insert(
        "toFilePath".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_to_file_path))),
    );
    map.insert(
        "parseData".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_parse_data))),
    );
    map.insert(
        "parseView".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_parse_view))),
    );
    map.insert(
        "domainToAscii".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_domain_to_ascii))),
    );
    map.insert(
        "domainToUnicode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_domain_to_unicode))),
    );
    map.insert(
        "diff".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_diff))),
    );
    map.insert(
        "securityReport".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_security_report))),
    );
    map.insert(
        "isValid".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_is_valid))),
    );
    map.insert(
        "isAbsolute".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_is_absolute))),
    );
    map.insert(
        "isRelative".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_is_relative))),
    );
    map.insert(
        "percentEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_percent_encode))),
    );
    map.insert(
        "percentDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_percent_decode))),
    );
    map.insert(
        "formEncode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_form_encode))),
    );
    map.insert(
        "formDecode".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_form_decode))),
    );
    map.insert(
        "toDataUrl".to_string(),
        Value::Function(NativeFn(Arc::new(builtin_url_to_data_url))),
    );

    Value::Object(Arc::new(map))
}
