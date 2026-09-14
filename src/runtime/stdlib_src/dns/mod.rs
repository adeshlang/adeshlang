//! DNS Standard Library Module Root

pub mod api;
pub mod cache;
pub mod dnssec;
pub mod edns;
pub mod message;
pub mod name;
pub mod policy;
pub mod records;
pub mod resolver;
pub mod service;
pub mod transport;
pub mod wire;

#[cfg(test)]
pub mod tests;

pub use api::register_all;
