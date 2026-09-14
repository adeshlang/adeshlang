//! DNS Transport Layer sitting directly on top of `Net` builtins.

use super::message::DNSMessage;
use super::wire::{DNSWireDecoder, DNSWireEncoder};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream, UdpSocket};
use std::time::Duration;

pub struct DNSTransport;

impl DNSTransport {
    pub fn query_udp(
        server: &str,
        msg: &DNSMessage,
        timeout: Duration,
    ) -> Result<DNSMessage, String> {
        let addr: SocketAddr = if server.contains(':') {
            server
                .parse()
                .map_err(|e| format!("Invalid DNS server socket address '{}': {}", server, e))?
        } else {
            format!("{}:53", server)
                .parse()
                .map_err(|e| format!("Invalid DNS server IP '{}': {}", server, e))?
        };

        let socket = UdpSocket::bind("0.0.0.0:0")
            .map_err(|e| format!("Failed to bind local UDP socket for DNS: {}", e))?;

        socket
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set UDP read timeout: {}", e))?;
        socket
            .set_write_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set UDP write timeout: {}", e))?;

        let wire_bytes = DNSWireEncoder::encode(msg)?;
        socket
            .send_to(&wire_bytes, addr)
            .map_err(|e| format!("UDP send to DNS server '{}' failed: {}", server, e))?;

        let mut buf = [0u8; 4096];
        let (len, _) = socket
            .recv_from(&mut buf)
            .map_err(|e| format!("UDP receive from DNS server '{}' failed: {}", server, e))?;

        let response = DNSWireDecoder::decode(&buf[..len])?;

        // If Truncated bit (TC) is set, fallback to TCP
        if response.header.tc {
            return Self::query_tcp(server, msg, timeout);
        }

        Ok(response)
    }

    pub fn query_tcp(
        server: &str,
        msg: &DNSMessage,
        timeout: Duration,
    ) -> Result<DNSMessage, String> {
        let addr: SocketAddr = if server.contains(':') {
            server
                .parse()
                .map_err(|e| format!("Invalid DNS server socket address '{}': {}", server, e))?
        } else {
            format!("{}:53", server)
                .parse()
                .map_err(|e| format!("Invalid DNS server IP '{}': {}", server, e))?
        };

        let mut stream = TcpStream::connect_timeout(&addr, timeout)
            .map_err(|e| format!("TCP connection to DNS server '{}' failed: {}", server, e))?;

        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set TCP read timeout: {}", e))?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|e| format!("Failed to set TCP write timeout: {}", e))?;

        let wire_bytes = DNSWireEncoder::encode(msg)?;
        let len_prefix = (wire_bytes.len() as u16).to_be_bytes();

        stream
            .write_all(&len_prefix)
            .map_err(|e| format!("Failed writing TCP DNS prefix length: {}", e))?;
        stream
            .write_all(&wire_bytes)
            .map_err(|e| format!("Failed writing TCP DNS query payload: {}", e))?;

        let mut len_buf = [0u8; 2];
        stream
            .read_exact(&mut len_buf)
            .map_err(|e| format!("Failed reading TCP DNS prefix length response: {}", e))?;
        let resp_len = u16::from_be_bytes(len_buf) as usize;

        let mut resp_buf = vec![0u8; resp_len];
        stream
            .read_exact(&mut resp_buf)
            .map_err(|e| format!("Failed reading TCP DNS response payload: {}", e))?;

        DNSWireDecoder::decode(&resp_buf)
    }
}
