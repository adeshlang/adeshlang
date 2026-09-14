# networking-guide.md

> Consolidated from 6 documentation files on 2026-08-29.

---


---

## Source: net.md

# AdeshLang `Net` Standard Library Master Documentation

The **`Net` Standard Library** is the production-grade, high-performance networking foundation for AdeshLang. It provides cross-platform, zero-GC, RAII-safe networking primitives for TCP, UDP, IP addressing, socket management, network interface enumeration, and security policy enforcement (SSRF protection).

---

## Table of Contents

1. [Architecture & Design Principles](#architecture--design-principles)
2. [Importing Net](#importing-net)
3. [Complete API Reference](#complete-api-reference)
   - [Top-Level `Net` Functions](#top-level-net-functions)
   - [IP Address (`Net.IPAddress`)](#ip-address-netipaddress)
   - [Socket Address (`Net.SocketAddress`)](#socket-address-netsocketaddress)
   - [TCP Stream (`TcpStream`)](#tcp-stream-tcpstream)
   - [TCP Listener (`TcpListener`)](#tcp-listener-tcplistener)
   - [UDP Socket (`UdpSocket`)](#udp-socket-udpsocket)
   - [Network Interfaces (`Net.interfaces()`)](#network-interfaces-netinterfaces)
   - [Security Policy (`Net.NetworkPolicy()`)](#security-policy-netnetworkpolicy)
4. [Error Handling & Taxonomy](#error-handling--taxonomy)
5. [Real-World Use Cases & Code Examples](#real-world-use-cases--code-examples)

---

## Architecture & Design Principles

```text
URL (Application Layer)
 ↓
Net (Transport & Network Primitives)
 ↓
DNS Resolution / IP Addressing
 ↓
TCP (Streams) / UDP (Datagrams)
 ↓
OS Socket Abstraction (Winsock / POSIX / WASI)
```

- **Ownership & RAII**: Native OS socket handles (`SOCKET` on Windows, `fd` on POSIX) are bound to thread-safe handle wrappers. When an object drops or `.close()` is called, the OS handle is closed safely without resource leaks or double-close panics.
- **Byte-Oriented & UTF-8 Compatible**: Primary operations work on byte arrays while providing explicit string helpers (`writeText`).
- **Capability-Aware & Secure**: Integrated `NetworkPolicy` allows sandboxing network activity (blocking SSRF, private IPs, loopback, or unauthorized ports).

---

## Importing Net

```adesh
import Net;
```

---

## Complete API Reference

### Top-Level `Net` Functions

| Function Signature | Description |
| :--- | :--- |
| `Net.tcpConnect(host, port)` | Connects to a TCP server endpoint (`host:port`). |
| `Net.tcpListen(host, port)` | Binds a TCP server listener to `host:port`. |
| `Net.udpBind(host, port)` | Binds a UDP datagram socket to `host:port`. |
| `Net.resolve(hostname)` | Resolves a domain hostname to an array of IP addresses. |
| `Net.resolveAddress(hostname)` | Alias for `Net.resolve()`. |
| `Net.interfaces()` | Enumerates local network interfaces on the machine. |
| `Net.IPAddress.parse(ipStr)` | Parses an IPv4 or IPv6 address string into an `IPAddress` object. |
| `Net.SocketAddress.parse(addrStr)` | Parses a formatted address string (`"127.0.0.1:8080"` or `"[::1]:443"`) into a `SocketAddress` object. |
| `Net.NetworkPolicy()` | Constructs a new `NetworkPolicy` builder object. |

---

### IP Address (`Net.IPAddress`)

#### Constructor / Parser:
```adesh
let ip = Net.IPAddress.parse("192.168.1.1");
let ip6 = Net.IPAddress.parse("fe80::1%eth0");
```

#### Object Properties:
- `ip.address`: String representation (e.g. `"192.168.1.1"`, `"fe80::1%eth0"`).
- `ip.version`: `"v4"` or `"v6"`.
- `ip.zone`: Optional zone scope identifier for IPv6 (e.g. `"eth0"`).
- `ip.isLoopback`: `true` if loopback address (`127.0.0.1`, `::1`).
- `ip.isPrivate`: `true` if RFC 1918 / Unique Local address (`192.168.x.x`, `10.x.x.x`, `172.16.x.x`, `fc00::/7`).
- `ip.isLinkLocal`: `true` if link-local address (`169.254.x.x`, `fe80::/10`).
- `ip.isMulticast`: `true` if multicast group address (`224.0.0.0/4`, `ff00::/8`).
- `ip.isBroadcast`: `true` if IPv4 broadcast address (`255.255.255.255`).
- `ip.isUnspecified`: `true` if unspecified address (`0.0.0.0`, `::`).
- `ip.isGlobal`: `true` if globally routable internet IP address.
- `ip.isDocumentation`: `true` if documentation address (`192.0.2.x`, `2001:db8::/32`).
- `ip.isBenchmark`: `true` if benchmark testing address (`198.18.x.x`).

---

### Socket Address (`Net.SocketAddress`)

#### Constructor / Parser:
```adesh
let addr = Net.SocketAddress.parse("127.0.0.1:8080");
let addr6 = Net.SocketAddress.parse("[::1]:443");
```

#### Object Properties:
- `addr.ip`: `IPAddress` object.
- `addr.port`: Integer port number (`0..=65535`).
- `addr.address`: Formatted canonical string (e.g. `"127.0.0.1:8080"`, `"[::1]:443"`).

---

### TCP Stream (`TcpStream`)

Returned by `Net.tcpConnect()` or `server.accept().stream`.

#### Methods:
- `stream.read(maxBytes)`: Reads up to `maxBytes` into a byte array `[u8]`.
- `stream.readExact(exactBytes)`: Reads exactly `exactBytes` or throws EOF error.
- `stream.write(bytesOrString)`: Writes byte array or string. Returns bytes written count.
- `stream.writeAll(bytesOrString)`: Writes all bytes or throws error.
- `stream.writeText(string)`: Convenience method to transmit UTF-8 encoded text.
- `stream.flush()`: Flushes buffered stream data.
- `stream.shutdown()`: Performs socket shutdown (read/write).
- `stream.close()`: Closes stream handle immediately and releases OS resources.
- `stream.localAddress()`: Returns local address string (`"127.0.0.1:54123"`).
- `stream.remoteAddress()`: Returns remote peer address string (`"127.0.0.1:8080"`).
- `stream.setNoDelay(bool)`: Enables or disables `TCP_NODELAY` (Nagle's algorithm).
- `stream.setReadTimeout(seconds)`: Sets socket read timeout in floating-point seconds.
- `stream.setWriteTimeout(seconds)`: Sets socket write timeout in floating-point seconds.

---

### TCP Listener (`TcpListener`)

Returned by `Net.tcpListen(host, port)`.

#### Methods:
- `listener.accept()`: Blocks and accepts an incoming client connection. Returns an object:
  ```adesh
  {
      stream: TcpStream,
      remoteAddress: String,
      localAddress: String
  }
  ```
- `listener.localAddress()`: Returns local bound address string (`"0.0.0.0:8080"`).
- `listener.close()`: Stops listening and closes native listener socket handle.

---

### UDP Socket (`UdpSocket`)

Returned by `Net.udpBind(host, port)`.

#### Methods:
- `socket.sendTo(data, targetHost, port)`: Transmits datagram payload to `targetHost:port`.
- `socket.receiveFrom(maxBytes)`: Receives incoming datagram. Returns `UdpPacket` object:
  ```adesh
  {
      data: Array[u8],
      remoteAddress: String,
      remotePort: Integer,
      bytesReceived: Integer
  }
  ```
- `socket.setBroadcast(bool)`: Enables or disables UDP broadcasting.
- `socket.joinMulticast(groupIp, interfaceIp)`: Joins IPv4 or IPv6 multicast group.
- `socket.leaveMulticast(groupIp, interfaceIp)`: Leaves IPv4 or IPv6 multicast group.
- `socket.localAddress()`: Returns local bound address string (`"0.0.0.0:9000"`).
- `socket.close()`: Closes UDP socket handle.

---

### Network Interfaces (`Net.interfaces()`)

Returns an array of local network interfaces. Each object exposes:
- `iface.name`: Interface name (e.g. `"lo0"`, `"eth0"`, `"wlan0"`).
- `iface.index`: Interface index integer.
- `iface.isUp`: Boolean up/down status.
- `iface.isLoopback`: Boolean loopback flag.
- `iface.isMulticast`: Boolean multicast flag.
- `iface.addresses`: Array of assigned IP address strings (IPv4 and IPv6).

---

### Security Policy (`Net.NetworkPolicy()`)

Constructs a security sandbox policy.

#### Builder Methods:
- `policy.allowTcp()`, `policy.denyTcp()`
- `policy.allowUdp()`, `policy.denyUdp()`
- `policy.allowIPv4()`, `policy.denyIPv4()`
- `policy.allowIPv6()`, `policy.denyIPv6()`
- `policy.denyPrivateNetworks()`: Blocks RFC 1918 / private IPs (SSRF defense).
- `policy.denyLoopback()`: Blocks loopback addresses (`127.0.0.1`, `::1`).
- `policy.allowPort(port)`, `policy.denyPort(port)`: Port whitelist/blacklist.
- `policy.validate(ipOrAddressStr)`: Validates IP string against policy rules. Throws error if violated.

---

## Real-World Use Cases & Code Examples

### 1. Simple TCP Echo Server
```adesh
import Net;

let server = Net.tcpListen("127.0.0.1", 8080);
let client = server.accept();

let data = client.stream.read(1024);
client.stream.writeAll(data);

client.stream.close();
server.close();
```

### 2. HTTP REST API Server Endpoint
```adesh
import Net;

let server = Net.tcpListen("127.0.0.1", 9000);
let client = server.accept();
let stream = client.stream;

let req = stream.read(2048);

let jsonBody = "{\"status\":\"ok\",\"message\":\"Hello from AdeshLang API\"}";
let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 51\r\n\r\n" + jsonBody;

stream.writeText(response);
stream.close();
server.close();
```

### 3. SSRF Protection & Security Check
```adesh
import Net;

let policy = Net.NetworkPolicy()
    .allowTcp()
    .allowPort(443)
    .denyPrivateNetworks()
    .denyLoopback();

try {
    policy.validate("192.168.1.1");
} catch (err) {
    print("Blocked untrusted private network target:", err);
}
```


---

## Source: dns.md

# AdeshLang DNS Standard Library Specification & Documentation

The **`DNS` Standard Library** is the production-grade, high-performance DNS resolution, record querying, caching, security policy, and service discovery foundation for AdeshLang.

## Table of Contents
1. [Overview](#overview)
2. [Importing DNS](#importing-dns)
3. [API Reference](#api-reference)
   - [`DNS.resolve(hostname)`](#dnsresolvehostname)
   - [`DNS.query(hostname, type)`](#dnsqueryhostname-type)
   - [`DNS.reverseLookup(ipStr)`](#dnsreverselookupipstr)
   - [`DNS.discover(serviceName)`](#dnsdiscoverservicename)
4. [Supported Record Types](#supported-record-types)
5. [Architecture & Safety](#architecture--safety)

---

## Overview

The `DNS` module provides a comprehensive suite of DNS operations built cleanly on top of `Net`:
- Domain name parsing & label length validation
- IDN / Punycode support
- EDNS(0) and wire format binary encoding & decoding
- Compression pointer safety (loop & recursion depth protection)
- Thread-safe LRU caching with positive & negative TTL tracking
- Security policies (SSRF defense, private IP denying, loopback blocking)
- Service Discovery (`SRV` & `TXT`)

---

## Importing DNS

```adesh
import DNS;
```

---

## API Reference

### `DNS.resolve(hostname)`
Resolves a domain name into an array of IP address strings.

```adesh
import DNS;

let ips = DNS.resolve("example.com");
for (ip in ips) {
    print(ip);
}
```

### `DNS.query(hostname, type)`
Queries specific DNS record types (`A`, `AAAA`, `MX`, `TXT`, `CNAME`, `NS`, `PTR`, `SOA`, `SRV`).

```adesh
import DNS;

let records = DNS.query("example.com", DNS.Type.MX);
for (rec in records) {
    print(rec.name, rec.type, rec.ttl, rec.data);
}
```

### `DNS.reverseLookup(ipStr)`
Performs reverse DNS lookup using `in-addr.arpa` or `ip6.arpa`.

```adesh
import DNS;

let host = DNS.reverseLookup("8.8.8.8");
print(host);
```

### `DNS.discover(serviceName)`
Performs RFC 2782 service discovery over SRV records.

```adesh
import DNS;

let services = DNS.discover("_http._tcp.example.com");
for (s in services) {
    print(s.host, s.port, s.priority, s.weight);
}
```


---

## Source: tls.md

# AdeshLang TLS Standard Library

## Overview
The `TLS` module provides production-grade, memory-safe, fail-closed Transport Layer Security (TLS 1.3 / TLS 1.2) capabilities for AdeshLang applications.

It cleanly isolates the TLS state machine and security policies while seamlessly integrating with `Crypto`, `Net`, `DNS`, `Time`, and `IO` standard libraries.

## Security Defaults
- **Protocol Versions**: Minimum TLS 1.2, Maximum TLS 1.3 (Insecure legacy versions SSLv2/v3/1.0/1.1 are completely disabled).
- **Certificate Verification**: ON by default using system trust store and `webpki-roots`.
- **Hostname & IP SAN Verification**: Mandatory by default (DNS hostnames are validated against DNS SANs; IP literals are validated against IP SANs).
- **SNI (Server Name Indication)**: Sent automatically for DNS domain names (IP addresses are matched via certificate IP SANs).
- **0-RTT Early Data**: Disabled by default to protect against replay attacks.
- **Session Resumption**: TLS 1.3 `ClientSessionMemoryCache` enabled by default across connection instances.
- **Key Material Safety**: Sensitive private keys and secrets are bounded to memory lifetime and released promptly.

## Module Import & Type System

### Full Module Import
```adesh
import TLS;

let tls: TLS = TLS.connect("1.1.1.1", 443);
```

### Named Imports
```adesh
import { connectWithOptions, SecurityPolicy, TrustStore } from TLS;

let policy: Object = SecurityPolicy().strict();
let trustStore: Object = TrustStore().systemAndWebpki();
let tls: TLS = connectWithOptions("1.1.1.1", 443, policy);
```

## Public API Reference

### Client Connections

#### `TLS.connect(host: String, port: i64): TLS`
Establishes a secure TLS connection to the destination host and port with default production security policy.

#### `TLS.connectWithOptions(host: String, port: i64, options: Object): TLS`
Establishes a secure connection with custom parameters (ALPN, custom CA PEM, client certificate for mTLS, min/max version).

#### `TLS.wrap(socket: Object, hostname: String): TLS`
Wraps an existing established raw TCP socket into a TLS client stream (useful for STARTTLS protocols).

### Server Operations

#### `TLS.Server(tcpListener: Object, certPem: String, keyPem: String): Object`
Creates a high-level production TLS server listener wrapper over `Net.tcpListen()`.
- `server.accept(): TLS`: Accepts the next TCP connection and wraps it in a server TLS state machine.

#### `TLS.bindServer(tcpSocket: Object, certPem: String, keyPem: String): TLS`
Wraps a single accepted raw TCP socket stream into a server TLS connection.

### Connection Methods (`tls.*`)

- `tls.writeText(text: String): i64`: Encrypts and writes UTF-8 text.
- `tls.writeBytes(bytes: Array<u8>): i64`: Encrypts and writes binary byte array.
- `tls.readText(maxBytes: i64 = 8192): String`: Reads and decrypts UTF-8 text. Fails closed on invalid UTF-8.
- `tls.readBytes(maxBytes: i64 = 8192): Array<u8>`: Reads raw binary data without text encoding assumptions.
- `tls.version(): String`: Returns negotiated protocol (`"TLSv1.3"` or `"TLSv1.2"`).
- `tls.alpn(): String`: Returns negotiated ALPN protocol (e.g., `"h2"`, `"http/1.1"`).
- `tls.peerCertificates(): Array<Array<u8>>`: Returns peer certificate chain DER bytes.
- `tls.trace(): Array<String>`: Returns structured diagnostic trace events.
- `tls.close(): Bool`: Gracefully sends TLS `close_notify` and releases connection resources.

## Example Code

### Type-Annotated Client
```adesh
import TLS;

let tls: TLS = TLS.connect("1.1.1.1", 443);
let bytesWritten: i64 = tls.writeText("GET / HTTP/1.1\r\nHost: 1.1.1.1\r\nConnection: close\r\n\r\n");

print("Protocol Version: " + tls.version());
print("ALPN Protocol: " + tls.alpn());

let responseText: String = tls.readText(2048);
print(responseText);

tls.close();
```

### Binary Non-UTF-8 Stream Transfer
```adesh
import TLS;

let tls: TLS = TLS.connect("1.1.1.1", 443);
tls.writeText("GET / HTTP/1.1\r\nHost: 1.1.1.1\r\nConnection: close\r\n\r\n");

let rawBytes: Array<u8> = tls.readBytes(1024);
print("Bytes length: " + rawBytes.length);

tls.close();
```

### Production TLS Server Listener
```adesh
import Net;
import TLS;

let tcpListener: Object = Net.tcpListen("127.0.0.1", 8443);
let server: Object = TLS.Server(tcpListener, certPem, keyPem);
let clientTls: TLS = server.accept();
```


---

## Source: tls-security.md

# AdeshLang TLS Security Guide

## Defense-in-Depth Architecture
AdeshLang TLS prioritizes fail-closed behavior, strict verification, zero insecure defaults, and clear ownership semantics.

### Key Controls
1. **No Insecure Defaults**: Certificate and hostname verification are strictly enforced out-of-the-box.
2. **Key Material Lifetime**: Private key memory buffers are owned strictly and cleaned up upon dropping connection resources.
3. **No Unauthenticated Data**: Record authentication tags are verified before exposing application payload buffers.
4. **Resilience & Resource Limits**: Certificate parsing and TLS record length processing are bounded to defend against DoS and memory exhaustion.


---

## Source: websocket.md

# WebSocket Standard Library & Async/DTO Guide — AdeshLang

AdeshLang provides a production-grade, RFC 6455 & RFC 6455/WSS WebSocket engine with:
- Native **Async/Await** non-blocking primitives (`connectAsync`, `receiveAsync`, `sendTextAsync`, `closeAsync`, `runAsync`, `acceptAsync`, `broadcastAsync`)
- First-class **DTO Type Safety** and **Schema Validation** (`WebSocket.DTO` / `WebSocket.Schema`)
- **Regex Pattern Constraints** on DTO fields (`pattern: r"^..."`)

---

## 1. Quickstart: Async/Await & Typed DTO Exchange

```adesh
import WebSocket;

// Define a DTO schema with Regex field validation rules
let UserPayloadDTO = WebSocket.DTO.define("UserPayload", {
    "username": {
        "type": "string",
        "required": true,
        "pattern": "^[a-zA-Z0-9_]{3,16}$"
    },
    "email": {
        "type": "string",
        "required": true,
        "pattern": "^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]{2,}$"
    }
});

async fn mainSession() {
    print("Connecting asynchronously to WebSocket server...");
    let conn = await WebSocket.connectAsync("ws://127.0.0.1:8080");

    let payload = {
        "username": "adesh_user",
        "email": "user@adeshlang.org"
    };

    // Validates payload against Regex rules, serializes to JSON, and sends over socket
    await conn.sendDTOAsync(UserPayloadDTO, payload);

    // Receives incoming text, parses JSON, and validates DTO/Regex rules
    let responseDTO = await conn.receiveDTOAsync(UserPayloadDTO);
    print("Received validated response DTO:", responseDTO);

    await conn.closeAsync(1000, "Done");
}
```

---

## 2. API Surface Reference

### Module Functions (`import WebSocket`)
| Function | Return Type | Description |
| --- | --- | --- |
| `WebSocket.connect(uri, config?)` | `WebSocketConnection \| WebSocketError` | Connect synchronously |
| `WebSocket.connectAsync(uri, config?)` | `Promise<WebSocketConnection>` | Connect asynchronously |
| `WebSocket.server(config?)` | `WebSocketServer` | Create WebSocket server instance |
| `WebSocket.DTO` | `DTOBuilder` | DTO schema definition builder |
| `WebSocket.Schema` | `SchemaBuilder` | Schema constraint builder |

### Client Connection (`WebSocketConnection`)
| Method | Return Type | Description |
| --- | --- | --- |
| `conn.sendText(text)` | `null \| WebSocketError` | Send text frame synchronously |
| `conn.sendTextAsync(text)` | `Promise<null>` | Send text frame asynchronously |
| `conn.sendBinary(bytes)` | `null \| WebSocketError` | Send binary frame synchronously |
| `conn.sendBinaryAsync(bytes)` | `Promise<null>` | Send binary frame asynchronously |
| `conn.sendPing(payload?)` | `null \| WebSocketError` | Send Ping frame |
| `conn.sendPingAsync(payload?)` | `Promise<null>` | Send Ping frame asynchronously |
| `conn.sendPong(payload?)` | `null \| WebSocketError` | Send Pong frame |
| `conn.sendPongAsync(payload?)` | `Promise<null>` | Send Pong frame asynchronously |
| `conn.receive()` | `WebSocketMessage \| WebSocketError` | Receive message synchronously |
| `conn.receiveAsync()` | `Promise<WebSocketMessage>` | Receive message asynchronously |
| `conn.sendDTO(dto, data)` | `null \| WebSocketError` | Validate with DTO/Regex & send JSON |
| `conn.sendDTOAsync(dto, data)` | `Promise<null>` | Validate with DTO/Regex & send JSON async |
| `conn.receiveDTO(dto?)` | `object \| WebSocketError` | Receive JSON & validate DTO/Regex |
| `conn.receiveDTOAsync(dto?)` | `Promise<object>` | Receive JSON & validate DTO/Regex async |
| `conn.close(code?, reason?)` | `null \| WebSocketError` | Close session synchronously |
| `conn.closeAsync(code?, reason?)` | `Promise<null>` | Close session asynchronously |

### WebSocket Server (`WebSocketServer`)
| Method | Return Type | Description |
| --- | --- | --- |
| `server.onConnection(handler)` | `null` | Set connection event handler |
| `server.run()` | `null` | Run server loop synchronously |
| `server.runAsync()` | `Promise<null>` | Run server loop asynchronously in background |
| `server.acceptAsync()` | `Promise<WebSocketConnection>` | Accept single connection asynchronously |
| `server.broadcast(message)` | `int` | Broadcast message to all active clients |
| `server.broadcastAsync(message)` | `Promise<int>` | Broadcast message asynchronously |
| `server.startEcho()` | `bool` | Start background echo server thread |
| `server.close()` | `null` | Stop server synchronously |
| `server.closeAsync()` | `Promise<null>` | Stop server asynchronously |

---

## 3. Enterprise Production WSS with Env, Path & IO Libraries

In production servers, SSL/TLS certificate locations are loaded dynamically using environment variables (`Env`) and resolved cross-platform using `Path` before loading PEM data via `IO`:

```adesh
import Env;
import Path;
import IO;
import WebSocket;

// Load environment configuration (.env / system variables)
Env.load(".env");

let sslCertDir = Env.getOrDefault("SSL_CERT_DIR", "/etc/letsencrypt/live/api.mycompany.com");
let certPath = Path.join(sslCertDir, "fullchain.pem").toString();
let keyPath  = Path.join(sslCertDir, "privkey.pem").toString();

// Read CA certificate chain & private key from disk
let certPem = IO.readTextFile(certPath);
let keyPem  = IO.readTextFile(keyPath);

// Configure & run enterprise WSS server over TLS
let wssServer = WebSocket.server({
    "bind_address": "0.0.0.0",
    "port": 8443,
    "tls_cert_pem": certPem,
    "tls_key_pem": keyPem,
    "protocols": ["wss-v1"]
});

wssServer.onConnection(fn(conn) {
    print("Encrypted WSS connection established over TLS!");
});

// Non-blocking background WSS server loop
wssServer.runAsync();

// Connect securely over WSS using Async/Await & Strict TLS verification
let wssClient = await WebSocket.connectAsync("wss://127.0.0.1:8443", {
    "verify_tls": true,
    "tls_ca_pem": certPem
});
```


---

## Source: url.md

# AdeshLang `URL` Standard Library

The `URL` standard library provides standards-compliant, memory-safe, zero-GC compatible, high-performance URL parsing, construction, resolution, routing, security policies, and addressing capabilities for AdeshLang applications.

---

## Overview & Architecture

```text
                         AdeshLang URL
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
       Parser              Builder             View
          │                   │                   │
          └───────────────────┼───────────────────┘
                              │
                      URL Component Model
                              │
       ┌──────────────┬───────┼────────┬─────────────┐
       │              │       │        │             │
     Scheme         Host     Path    Query        Fragment
       │              │       │        │             │
       │         ┌────┴───┐   │   QueryParams        │
       │         │        │   │        │              │
       │       IPv4     IPv6  │   Encoding            │
       │         │        │   │                       │
       └─────────┴────────┴───┴───────────────────────┘
                              │
                  ┌───────────┼───────────┐
                  │           │           │
               IDNA       Resolution   Patterns
                  │           │           │
                  └───────────┼───────────┘
                              │
                       Security Layer
                              │
             ┌────────────────┼────────────────┐
             │                │                │
          Redaction         SSRF           Policies
             │                │                │
             └────────────────┼────────────────┘
```

---

## Import Syntax

```adesh
import URL;
```

or with alias / namespace import:

```adesh
import "URL" as WebURL;
```

---

## API Levels & Usage

### Level 1 — Beginner (Simple Parsing & Access)

```adesh
import URL;

let url: URL = URL.parse("https://user:secret@example.com:8443/users/10?page=2&limit=50#results")?;

print(url.scheme());   // "https"
print(url.host());     // "example.com"
print(url.port());     // 8443
print(url.path());     // "/users/10"
print(url.query());    // "page=2&limit=50"
print(url.redacted()); // "https://user:***@example.com:8443/users/10?page=2&limit=50#results"
```

### Level 2 — Application Developer (Fluent URL Builder & Query Parameter Management)

```adesh
import URL;

let url: URL = URL.builder()
    .scheme("https")
    .host("api.example.com")
    .path("/users")
    .appendPathSegment("posts")
    .queryParam("page", 2)
    .queryParam("limit", 50)
    .fragment("active")
    .build()?;

print(url.href()); // "https://api.example.com/users/posts?page=2&limit=50#active"
```

### Level 3 — Systems & Security Developer (SSRF Validation, Zero-Copy Parsing & Routing)

```adesh
import URL;

let policy: URLSecurityPolicy = URL.SecurityPolicy()
    .allowScheme("https")
    .denyPrivateNetworks()
    .denyCredentials();

let url: URL = URL.parse("https://example.com/webhook")?;
policy.validate(url)?;

let view: URLView = URL.parseView("https://example.com/stream")?;
print(view.host()); // "example.com"
```

---

## Features & Capabilities

1. **Standards Compliance**: Follows RFC 3986 (URI), RFC 3987 (IRI), RFC 5890-5895 (IDNA), RFC 6874 (IPv6 Zone Identifiers), and WHATWG URL model.
2. **Credential Safety**: All string conversions automatically redact passwords (`user:***@host`).
3. **IPv4 & IPv6 Parsing**: Parses dotted quad IPv4, compressed IPv6 (`::1`, `2001:db8::1`), IPv4-mapped IPv6, and scoped zone IDs (`fe80::1%25eth0`).
4. **IDNA / Punycode**: Converts internationalized domain names (`münich.example` <-> `xn--mnich-kva.example`).
5. **Relative Reference Resolution**: Supports RFC 3986 relative reference resolution matrix (`./`, `../`, empty, query-only, fragment-only).
6. **URLPattern Matching**: Extracts route parameters (`/users/:id`, `/assets/*`).
7. **URLTemplate Expansion**: Expands template variables (`https://api.example.com/users/{id}`).
8. **Security Policies & SSRF Protection**: Detects loopback (`127.0.0.1`), private networks (`10.0.0.0/8`, `192.168.0.0/16`), link-local, cloud metadata service endpoints (`169.254.169.254`), and insecure redirect downgrades.
9. **Zero-Copy Borrowed Parsing**: `URLView` provides slice-based component access without memory allocation.

