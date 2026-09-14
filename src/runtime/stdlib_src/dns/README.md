# AdeshLang `DNS` Standard Library Specification & Complete Documentation

The **`DNS` Standard Library** is the production-grade, high-performance, ownership-safe, zero-GC DNS subsystem for AdeshLang (`import DNS;`).

Built directly on top of `Net` (without duplicating TCP/UDP sockets, IP address abstractions, or network interfaces), `DNS` provides protocol-complete resolution, record querying, service discovery, security policy sandboxing, and thread-safe LRU caching.

---

## Table of Contents
1. [Overview & Key Features](#1-overview--key-features)
2. [Importing DNS](#2-importing-dns)
3. [Top-Level API Reference](#3-top-level-api-reference)
   - [`DNS.resolve(hostname)`](#dnsresolvehostname)
   - [`DNS.query(hostname, recordType)`](#dnsqueryhostname-recordtype)
   - [`DNS.reverseLookup(ipString)`](#dnsreverselookupipstring)
   - [`DNS.discover(serviceName)`](#dnsdiscoverservicename)
   - [`DNS.Resolver()`](#dnsresolver)
   - [`DNS.Policy()`](#dnspolicy)
4. [DNS Record Types (`DNS.Type`)](#4-dns-record-types-dnstype)
   - [`A` (IPv4)](#a-record)
   - [`AAAA` (IPv6)](#aaaa-record)
   - [`CNAME` (Canonical Name)](#cname-record)
   - [`MX` (Mail Exchange)](#mx-record)
   - [`TXT` (Text / Verification)](#txt-record)
   - [`NS` (Name Server)](#ns-record)
   - [`PTR` (Pointer / Reverse DNS)](#ptr-record)
   - [`SOA` (Start of Authority)](#soa-record)
   - [`SRV` (Service Locator)](#srv-record)
   - [`CAA` (Certificate Authority Authorization)](#caa-record)
5. [Resolver Core & Configuration](#5-resolver-core--configuration)
   - [Custom Servers](#custom-servers)
   - [Timeouts & Retries](#timeouts--retries)
   - [Cache Integration](#cache-integration)
6. [Security Policy & SSRF Defense](#6-security-policy--ssrf-defense)
   - [Rebinding Defense](#rebinding-defense)
   - [Loopback & Private Network Denials](#loopback--private-network-denials)
7. [Wire Format & Compression Protection](#7-wire-format--compression-protection)
8. [Example Gallery](#8-example-gallery)

---

## 1. Overview & Key Features

- **Ownership & RAII Safe**: Completely integrates with AdeshLang zero-GC borrowing and RAII semantics.
- **Protocol-Complete Wire Decoder**: High-performance binary encoder and decoder supporting EDNS0 pseudo-records, compression pointers, and pointer loop safety bounds ($\text{depth} \le 16$).
- **Thread-Safe LRU Cache**: Automatic positive caching (respecting authoritative TTLs) and negative caching (`NXDOMAIN`/`NODATA`).
- **Service Discovery**: Built-in RFC 2782 SRV service discovery algorithm (`DNS.discover()`).
- **Security Policy Sandboxing**: Granular `DNSPolicy` for SSRF protection, loopback blocking, link-local denial, and private network restrictions.

---

## 2. Importing DNS

```adesh
import DNS;
```

---

## 3. Top-Level API Reference

### `DNS.resolve(hostname)`
Resolves a domain hostname string into an array of IPv4 and IPv6 address strings.

- **Parameters**: `hostname` (string)
- **Returns**: `Array<String>`
- **Example**:
  ```adesh
  let ips = DNS.resolve("google.com");
  for (ip in ips) {
      print("IP:", ip);
  }
  ```

### `DNS.query(hostname, recordType)`
Queries specific DNS records for a given domain and record type string (e.g. `DNS.Type.A`, `DNS.Type.MX`, `DNS.Type.TXT`).

- **Parameters**: `hostname` (string), `recordType` (string)
- **Returns**: `Array<Object>` where each object contains `{ name, type, ttl, data }`.
- **Example**:
  ```adesh
  let mxRecords = DNS.query("gmail.com", DNS.Type.MX);
  for (rec in mxRecords) {
      print("Exchange:", rec.data.exchange, "Priority:", rec.data.priority);
  }
  ```

### `DNS.reverseLookup(ipString)`
Performs a reverse DNS lookup for an IPv4 or IPv6 address string generating `in-addr.arpa` or `ip6.arpa` queries.

- **Parameters**: `ipString` (string)
- **Returns**: `String` (resolved canonical hostname)
- **Example**:
  ```adesh
  let host = DNS.reverseLookup("8.8.8.8");
  print("Host:", host); // "dns.google"
  ```

### `DNS.discover(serviceName)`
Performs RFC 2782 SRV service discovery and returns ordered service endpoints sorted by priority and weight.

- **Parameters**: `serviceName` (string)
- **Returns**: `Array<Object>` containing `{ host, port, priority, weight }`.
- **Example**:
  ```adesh
  let services = DNS.discover("_xmpp-client._tcp.jabber.org");
  for (s in services) {
      print("Discovered Endpoint:", s.host, s.port);
  }
  ```

### `DNS.Resolver()`
Constructs a configurable `Resolver` instance.

- **Methods**: `.servers(array)`, `.timeout(seconds)`, `.retries(count)`, `.cache(enabled)`
- **Example**:
  ```adesh
  let resolver = DNS.Resolver()
      .servers(["1.1.1.1", "8.8.8.8"])
      .timeout(3)
      .cache(true);

  let records = resolver.query("cloudflare.com", DNS.Type.A);
  ```

### `DNS.Policy()`
Constructs a security policy builder object for SSRF prevention.

- **Methods**: `.denyLoopback()`, `.denyPrivateNetworks()`
- **Example**:
  ```adesh
  let policy = DNS.Policy()
      .denyLoopback()
      .denyPrivateNetworks();
  ```

---

## 4. DNS Record Types (`DNS.Type`)

| Type Symbol | Record Type | Data Object Schema |
| :--- | :--- | :--- |
| `DNS.Type.A` | IPv4 Address | `String` (e.g. `"172.217.24.142"`) |
| `DNS.Type.AAAA` | IPv6 Address | `String` (e.g. `"2404:6800:4007:81b::200e"`) |
| `DNS.Type.CNAME` | Canonical Name | `String` (e.g. `"github.com"`) |
| `DNS.Type.MX` | Mail Exchange | `{ exchange: String, priority: Number }` |
| `DNS.Type.TXT` | Text Records | `Array<String>` (e.g. `["v=spf1 ..."]`) |
| `DNS.Type.NS` | Name Server | `String` (e.g. `"ns1.google.com"`) |
| `DNS.Type.PTR` | Pointer | `String` (e.g. `"dns.google"`) |
| `DNS.Type.SOA` | Start of Authority | `{ primaryNs, respMailbox, serial, refresh, retry, expire, minimumTtl }` |
| `DNS.Type.SRV` | Service Locator | `{ host, port, priority, weight }` |
| `DNS.Type.CAA` | Certificate Authority | `{ flags, tag, value }` |

---

## 5. Resolver Core & Configuration

The resolution engine queries upstream DNS servers using UDP transport with automatic TCP fallback whenever the truncated bit (`TC=1`) is received in a packet header.

### Multi-Server Rotation
Configurable upstream nameservers (`servers(["1.1.1.1", "8.8.8.8"])`) are queried sequentially with automatic failover upon network timeouts or socket failures.

---

## 6. Security Policy & SSRF Defense

```adesh
import DNS;

let policy = DNS.Policy()
    .denyLoopback()
    .denyPrivateNetworks();
```

When attached to DNS resolution workflows, `DNSPolicy` validates resolved IP addresses before returning them to prevent:
- **SSRF (Server-Side Request Forgery)** attacks targeting internal services.
- **DNS Rebinding** attempts returning `127.0.0.1`, `10.0.0.0/8`, `172.16.0.0/12`, or `192.168.0.0/16` addresses.

---

## 7. Wire Format & Compression Protection

The binary parser (`src/runtime/stdlib_src/dns/wire.rs`) inspects DNS response byte streams with built-in safety constraints:
- **Pointer Loops**: Prevents infinite pointer recursion cycles during label decoding ($\text{max depth} = 16$).
- **Buffer Boundary Bounds**: Validates label lengths ($\le 63$ octets) and full domain lengths ($\le 255$ octets) prior to slice access.

---

## 8. Example Gallery

All example scripts are located in [`examples/libraries/dns/`](file:///d:/Projects/AdeshLang/examples/libraries/dns/):

- [`simple_resolve.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/simple_resolve.adesh): High-level hostname resolution.
- [`resolve_a_aaaa.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/resolve_a_aaaa.adesh): Querying `A` and `AAAA` records.
- [`query_mx_txt_ns.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/query_mx_txt_ns.adesh): Querying mail exchanges, TXT verification, and name servers.
- [`query_cname_soa.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/query_cname_soa.adesh): Canonical names (`CNAME`) and Start of Authority (`SOA`).
- [`service_discovery.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/service_discovery.adesh): SRV record service discovery.
- [`query_caa_custom.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/query_caa_custom.adesh): CAA certificate authorization queries.
- [`reverse_lookup.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/reverse_lookup.adesh): Reverse DNS lookups for IPv4 & IPv6.
- [`dns_security_policy.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/dns_security_policy.adesh): Security policy creation & SSRF defense.
- [`dns_caching.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/dns_caching.adesh): Resolver caching demonstration.
- [`batch_resolution.adesh`](file:///d:/Projects/AdeshLang/examples/libraries/dns/batch_resolution.adesh): Parallel multi-domain resolution batching.
