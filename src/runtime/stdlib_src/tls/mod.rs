//! Standard Library TLS module.

pub mod api;
pub mod cert;
pub mod connection;
pub mod errors;
pub mod policy;

pub fn init_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

pub fn register_all(registry: &mut crate::runtime::stdlib_src::registry::BuiltinRegistry) {
    init_crypto_provider();
    api::register_all(registry);
}
