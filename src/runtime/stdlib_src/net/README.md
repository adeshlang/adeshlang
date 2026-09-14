# AdeshLang `Net` Standard Library Specification & Complete Documentation

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
