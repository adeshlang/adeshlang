//! AdeshLang URL Standard Library Module Root.

pub mod api;
pub mod builder;
pub mod idna_punycode;
pub mod ipv4_ipv6;
pub mod parser;
pub mod pattern;
pub mod query;
pub mod security;
pub mod special_urls;
pub mod template;
pub mod url_object;
pub mod view;

pub use api::{build_url_module_object, register_all};
pub use builder::URLBuilder;
pub use ipv4_ipv6::IPAddress;
pub use parser::parse_url;
pub use pattern::URLPattern;
pub use query::QueryParams;
pub use security::URLSecurityPolicy;
pub use template::URLTemplate;
pub use url_object::URL;
pub use view::URLView;

#[cfg(test)]
mod tests;
