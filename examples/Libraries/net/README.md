# AdeshLang `Net` Standard Library — Complete User Guide & Examples Index

The **`Net` Standard Library** is AdeshLang's foundational networking module. It provides cross-platform, zero-GC, RAII-safe transport primitives for TCP, UDP, IP address management, network interface enumeration, and SSRF security policies.

---

## Example Scripts Index

| File Name | Category | Description |
| :--- | :--- | :--- |
| [`01_ip_addressing_advanced.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/01_ip_addressing_advanced.adesh) | IP Addressing | IPv4/IPv6 parsing, zone scopes (`fe80::1%eth0`), IPv4-mapped IPv6, address classification. |
| [`02_socket_addressing_parsing.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/02_socket_addressing_parsing.adesh) | Socket Addressing | `SocketAddress` parsing, bracketed IPv6 formatting (`[::1]:8080`), port validation. |
| [`03_tcp_echo_server_full.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/03_tcp_echo_server_full.adesh) | TCP Server | Production TCP echo server with client handling and graceful shutdown. |
| [`04_tcp_client_timeouts.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/04_tcp_client_timeouts.adesh) | TCP Client | TCP stream client with `setNoDelay`, `setReadTimeout`, and `setWriteTimeout`. |
| [`05_udp_broadcasting_multicast.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/05_udp_broadcasting_multicast.adesh) | UDP Datagrams | UDP datagram exchange (`sendTo`, `receiveFrom`), broadcasting, and IPv4/IPv6 multicast groups. |
| [`06_network_security_ssrf.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/06_network_security_ssrf.adesh) | Security | `NetworkPolicy` sandboxing, SSRF defense, blocking private/loopback networks. |
| [`07_network_interfaces_discovery.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/07_network_interfaces_discovery.adesh) | Interfaces | Local network interface enumeration (`Net.interfaces()`), MAC addresses, up/down flags. |
| [`08_dns_resolution.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/08_dns_resolution.adesh) | DNS Resolution | Hostname resolution (`Net.resolve()`), IPv4/IPv6 DNS lookups. |
| [`09_url_and_net_integration.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/09_url_and_net_integration.adesh) | Integration | Cross-library integration with `URL`, `Crypto`, `Encoding`, and `Net`. |
| [`10_tcp_chat_server.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/10_tcp_chat_server.adesh) | TCP Server | Multi-client TCP server endpoint with socket option configuration. |
| [`11_tcp_chat_client.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/11_tcp_chat_client.adesh) | TCP Client | Interactive TCP client endpoint with header transmission and response processing. |
| [`12_http_raw_client.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/12_http_raw_client.adesh) | HTTP Client | Raw HTTP/1.1 REST client built directly on `Net.tcpConnect()`. |
| [`13_http_raw_server.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/13_http_raw_server.adesh) | HTTP Server | Raw HTTP REST API endpoint built on `Net.tcpListen()` returning JSON payloads. |
| [`14_udp_echo_client_server.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/14_udp_echo_client_server.adesh) | UDP Pair | Bidirectional UDP client/server datagram pair. |
| [`15_secure_web_client.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/15_secure_web_client.adesh) | Integration | Secure Web Client pattern using `URL.parse`, `Crypto.sha256`, `Encoding.base64Encode`, `Net.tcpConnect`. |
| [`16_secure_api_server.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/16_secure_api_server.adesh) | Integration | Secure REST API Server using `JSON.stringify`, `Crypto.sha256`, `Encoding.base64Encode`, `Net.tcpListen`. |
| [`17_url_proxy_validator.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/17_url_proxy_validator.adesh) | Reverse Proxy | Reverse Proxy & SSRF Target Validator with `NetworkPolicy`. |
| [`19_advanced_networking_master_showcase.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/net/19_advanced_networking_master_showcase.adesh) | Showcase | Complete showcase of `defer`, `class`, `interface`, type annotations, and `Net`. |

---

## Quick Start Code Snippets

### 1. Connecting via TCP
```adesh
import Net;

let stream = Net.tcpConnect("127.0.0.1", 8080);
stream.writeText("GET / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");

let response = stream.read(1024);
print("Response bytes:", response.len());

stream.close();
```

### 2. Listening via TCP
```adesh
import Net;

let server = Net.tcpListen("127.0.0.1", 8080);
let client = server.accept();

client.stream.writeText("HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nHELLO");
client.stream.close();
server.close();
```

### 3. SSRF Protection Policy
```adesh
import Net;

let policy = Net.NetworkPolicy()
    .allowTcp()
    .allowPort(443)
    .denyPrivateNetworks()
    .denyLoopback();

let ip = Net.IPAddress.parse("127.0.0.1");
try {
    policy.validate(ip.address);
} catch (err) {
    print("Blocked untrusted IP:", err);
}
```
