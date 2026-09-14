pub mod alt_svc;
pub mod client;
pub mod cookie_jar;
pub mod dns_racing;
pub mod pool;
pub mod redirects;
pub mod retries;

pub use alt_svc::AltSvcCache;
pub use client::HttpClient;
pub use cookie_jar::{Cookie, CookieJar, SameSitePolicy};
pub use dns_racing::happy_eyeballs_connect;
pub use pool::{ConnectionPool, PoolKey};
pub use redirects::{RedirectEngine, RedirectPolicy};
pub use retries::RetryPolicy;
