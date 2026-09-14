//! Synchronous TLS Connection Stream abstraction wrapping Rustls ClientConnection / ServerConnection and TcpStream.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::Arc;

use once_cell::sync::Lazy;
use rustls::client::Resumption;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, ServerConfig, ServerConnection, Stream,
};

use super::cert::{TlsTrustStore, load_pem_key_and_chain};
use super::errors::TlsError;
use super::policy::TlsSecurityPolicy;

static GLOBAL_SESSION_CACHE: Lazy<Arc<rustls::client::ClientSessionMemoryCache>> =
    Lazy::new(|| Arc::new(rustls::client::ClientSessionMemoryCache::new(1024)));

#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ED25519,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
        ]
    }
}

pub enum TlsSession {
    Client(ClientConnection),
    Server(ServerConnection),
}

pub struct TlsConnection {
    pub stream: TcpStream,
    pub session: TlsSession,
    pub hostname: String,
    pub trace_events: Vec<String>,
    pub is_closed: bool,
}

impl TlsConnection {
    pub fn connect_client(
        host: &str,
        port: u16,
        policy: &TlsSecurityPolicy,
        trust_store: &TlsTrustStore,
        client_cert_pem: Option<(&[u8], &[u8])>,
    ) -> Result<Self, TlsError> {
        let addr = format!("{}:{}", host, port);
        let socket = TcpStream::connect(&addr).map_err(|e| {
            TlsError::TransportError(format!("TCP connect failed to {}: {}", addr, e))
        })?;

        Self::wrap_client(socket, host, policy, trust_store, client_cert_pem)
    }

    pub fn wrap_client(
        socket: TcpStream,
        host: &str,
        policy: &TlsSecurityPolicy,
        trust_store: &TlsTrustStore,
        client_cert_pem: Option<(&[u8], &[u8])>,
    ) -> Result<Self, TlsError> {
        let mut store = trust_store.clone();
        for ca in &policy.custom_ca_pems {
            let _ = store.add_pem_ca(ca);
        }

        let versions = policy.to_rustls_versions();
        let config_builder = ClientConfig::builder_with_protocol_versions(&versions);

        let mut config = if !policy.verify_certificates {
            let builder = config_builder
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoCertificateVerification));
            if let Some((cert_pem, key_pem)) = client_cert_pem {
                let (chain, key) = load_pem_key_and_chain(cert_pem, key_pem)?;
                builder.with_client_auth_cert(chain, key).map_err(|e| {
                    TlsError::ClientAuthError(format!("Failed to set client cert: {}", e))
                })?
            } else {
                builder.with_no_client_auth()
            }
        } else {
            let builder = config_builder.with_root_certificates(store.root_store.clone());
            if let Some((cert_pem, key_pem)) = client_cert_pem {
                let (chain, key) = load_pem_key_and_chain(cert_pem, key_pem)?;
                builder.with_client_auth_cert(chain, key).map_err(|e| {
                    TlsError::ClientAuthError(format!("Failed to set client cert: {}", e))
                })?
            } else {
                builder.with_no_client_auth()
            }
        };

        if policy.enable_resumption {
            config.resumption = Resumption::store(GLOBAL_SESSION_CACHE.clone());
        }

        if !policy.alpn_protocols.is_empty() {
            config.alpn_protocols = policy.alpn_protocols.clone();
        }

        let (server_name, is_ip) = match ServerName::try_from(host.to_string()) {
            Ok(sn) => (sn, false),
            Err(_) => {
                if let Ok(ip) = host.parse::<std::net::IpAddr>() {
                    (ServerName::IpAddress(ip.into()), true)
                } else {
                    return Err(TlsError::SniError(format!(
                        "Invalid hostname/IP '{}'",
                        host
                    )));
                }
            }
        };

        let conn = ClientConnection::new(Arc::new(config), server_name).map_err(|e| {
            TlsError::HandshakeError(format!("TLS client initialization failed: {}", e))
        })?;

        let mut trace = Vec::new();
        trace.push("ClientHelloSent".into());
        if is_ip {
            trace.push(format!("IP_SAN:{}", host));
        } else {
            trace.push(format!("SNI:{}", host));
        }

        let mut tls = TlsConnection {
            stream: socket,
            session: TlsSession::Client(conn),
            hostname: host.to_string(),
            trace_events: trace,
            is_closed: false,
        };

        // Complete initial handshake
        tls.flush()?;
        tls.trace_events.push("HandshakeCompleted".into());

        Ok(tls)
    }

    pub fn wrap_server(
        socket: TcpStream,
        cert_pem: &[u8],
        key_pem: &[u8],
        policy: &TlsSecurityPolicy,
    ) -> Result<Self, TlsError> {
        let (chain, key) = load_pem_key_and_chain(cert_pem, key_pem)?;
        let versions = policy.to_rustls_versions();

        let mut config = ServerConfig::builder_with_protocol_versions(&versions)
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .map_err(|e| {
                TlsError::CertificateError(format!("Server config certificate error: {}", e))
            })?;

        if !policy.alpn_protocols.is_empty() {
            config.alpn_protocols = policy.alpn_protocols.clone();
        }

        let conn = ServerConnection::new(Arc::new(config)).map_err(|e| {
            TlsError::HandshakeError(format!("TLS server connection creation failed: {}", e))
        })?;

        Ok(TlsConnection {
            stream: socket,
            session: TlsSession::Server(conn),
            hostname: String::new(),
            trace_events: vec!["ServerConnectionCreated".into()],
            is_closed: false,
        })
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, TlsError> {
        self.read_inner(buf)
    }

    fn read_inner(&mut self, buf: &mut [u8]) -> Result<usize, TlsError> {
        if self.is_closed {
            return Err(TlsError::TransportError("Connection is closed".into()));
        }
        match &mut self.session {
            TlsSession::Client(conn) => {
                let mut stream = Stream::new(conn, &mut self.stream);
                stream
                    .read(buf)
                    .map_err(|e| TlsError::TransportError(e.to_string()))
            }
            TlsSession::Server(conn) => {
                let mut stream = Stream::new(conn, &mut self.stream);
                stream
                    .read(buf)
                    .map_err(|e| TlsError::TransportError(e.to_string()))
            }
        }
    }

    pub fn read_bytes(&mut self, max_len: usize) -> Result<Vec<u8>, TlsError> {
        let mut buf = vec![0u8; max_len];
        let n = self.read_inner(&mut buf)?;
        buf.truncate(n);
        Ok(buf)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize, TlsError> {
        self.write_inner(buf)
    }

    fn write_inner(&mut self, buf: &[u8]) -> Result<usize, TlsError> {
        if self.is_closed {
            return Err(TlsError::TransportError("Connection is closed".into()));
        }
        match &mut self.session {
            TlsSession::Client(conn) => {
                let mut stream = Stream::new(conn, &mut self.stream);
                stream
                    .write(buf)
                    .map_err(|e| TlsError::TransportError(e.to_string()))
            }
            TlsSession::Server(conn) => {
                let mut stream = Stream::new(conn, &mut self.stream);
                stream
                    .write(buf)
                    .map_err(|e| TlsError::TransportError(e.to_string()))
            }
        }
    }

    pub fn flush(&mut self) -> Result<(), TlsError> {
        self.flush_inner()
    }

    fn flush_inner(&mut self) -> Result<(), TlsError> {
        match &mut self.session {
            TlsSession::Client(conn) => {
                let mut stream = Stream::new(conn, &mut self.stream);
                stream
                    .flush()
                    .map_err(|e| TlsError::TransportError(e.to_string()))
            }
            TlsSession::Server(conn) => {
                let mut stream = Stream::new(conn, &mut self.stream);
                stream
                    .flush()
                    .map_err(|e| TlsError::TransportError(e.to_string()))
            }
        }
    }

    pub fn alpn_protocol(&self) -> Option<String> {
        let bytes = match &self.session {
            TlsSession::Client(conn) => conn.alpn_protocol()?,
            TlsSession::Server(conn) => conn.alpn_protocol()?,
        };
        String::from_utf8(bytes.to_vec()).ok()
    }

    pub fn protocol_version(&self) -> Option<String> {
        let ver = match &self.session {
            TlsSession::Client(conn) => conn.protocol_version()?,
            TlsSession::Server(conn) => conn.protocol_version()?,
        };
        match ver {
            rustls::ProtocolVersion::TLSv1_2 => Some("TLSv1.2".into()),
            rustls::ProtocolVersion::TLSv1_3 => Some("TLSv1.3".into()),
            _ => None,
        }
    }

    pub fn peer_certificates(&self) -> Vec<Vec<u8>> {
        let certs = match &self.session {
            TlsSession::Client(conn) => conn.peer_certificates(),
            TlsSession::Server(conn) => conn.peer_certificates(),
        };
        certs
            .unwrap_or_default()
            .iter()
            .map(|c| c.to_vec())
            .collect()
    }

    pub fn shutdown(&mut self) -> Result<(), TlsError> {
        if self.is_closed {
            return Ok(());
        }
        self.is_closed = true;
        self.trace_events.push("CloseNotifySent".into());
        match &mut self.session {
            TlsSession::Client(conn) => conn.send_close_notify(),
            TlsSession::Server(conn) => conn.send_close_notify(),
        }
        let _ = self.flush_inner();
        Ok(())
    }
}

impl std::io::Read for TlsConnection {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read_inner(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }
}

impl std::io::Write for TlsConnection {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write_inner(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.flush_inner()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))
    }
}
