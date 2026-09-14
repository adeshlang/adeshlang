//! AdeshLang Net Standard Library Module Root.

pub mod api;
pub mod errors;
pub mod interface;
pub mod ip;
pub mod options;
pub mod policy;
pub mod socket_addr;
pub mod tcp;
pub mod udp;

pub use api::{build_net_module_object, register_all};
pub use errors::{NetworkErrorKind, make_net_error};
pub use interface::{NetworkInterfaceInfo, list_network_interfaces};
pub use ip::IPAddress;
pub use options::SocketOptions;
pub use policy::NetworkPolicy;
pub use socket_addr::SocketAddress;
pub use tcp::{TcpStreamHandle, tcp_connect, tcp_listen};
pub use udp::udp_bind;

#[cfg(test)]
mod tests;
