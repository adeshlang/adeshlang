# AdeshLang TLS Standard Library — Production-Grade Security Reference

Production-grade, secure, memory-safe, fail-closed Transport Layer Security (TLS 1.3 & TLS 1.2) standard library for AdeshLang.

---

## Architecture & Principles
AdeshLang TLS owns the TLS state machine and security policies while building upon `Crypto`, `Net`, `DNS`, `Time`, and `IO`.

- **Dedicated `TLS` Type Annotations**: Full support for explicit `let tls: TLS = TLS.connect(...)` type declarations.
- **Named Specific Imports**: Supports direct destructuring imports (`import { connect, SecurityPolicy, TrustStore } from TLS;`).
- **Dedicated Type Builders**: Dedicated `SecurityPolicy()` and `TrustStore()` builders.
- **Secure Defaults**: Minimum TLS 1.2, Maximum TLS 1.3. Insecure protocols (SSLv2/v3, TLS 1.0/1.1) disabled by default.
- **Strict Verification**: Hostname and IP SAN verification enabled by default using system trust stores and `webpki-roots`.
- **Fail-Closed Design**: Any handshake or decryption failure closes the connection cleanly.
- **First-Class OO API**: Supports `.readBytes()`, `.readText()`, `.writeBytes()`, `.writeText()`, `.version()`, `.alpn()`, and `.close()`.

---

## Complete API Reference

### 1. `import TLS;` / `import { connect, connectWithOptions, SecurityPolicy, TrustStore } from TLS;`
Allows clean module imports and named specific imports directly from the `TLS` module.

```adesh
import { connectWithOptions, SecurityPolicy, TrustStore } from TLS;

let policy: Object = SecurityPolicy().strict();
let trustStore: Object = TrustStore().systemAndWebpki();
let tls: TLS = connectWithOptions("1.1.1.1", 443, policy);
```

### 2. `TLS.connect(host: String, port: Integer) -> TLS`
Establishes a synchronous TCP connection to `host:port` and executes a TLS handshake using default security policies. Returns a first-class `TLS` connection instance.

```adesh
let tls: TLS = TLS.connect("1.1.1.1", 443);
```

### 3. `TLS.connectWithOptions(host: String, port: Integer, options: Object) -> TLS`
Establishes a secure TLS connection with custom options (ALPN, custom CA, mTLS client credentials, min protocol version, verification flags).

```adesh
let options: Object = {
    "alpn": ["http/1.1", "h2"],
    "verifyCertificates": true,
    "verifyHostname": true,
    "minVersion": "TLS1.3"
};
let tls: TLS = TLS.connectWithOptions("example.com", 443, options);
```

### 4. `TLS.Server(tcpListener: Object, certPem: String, keyPem: String) -> TlsServer`
Creates a production TLS server listener wrapper over a `Net.tcpListen` instance. Calling `server.accept()` returns individual secured `TLS` connection instances.

```adesh
let tcpListener: Object = Net.tcpListen("127.0.0.1", 8443);
let server: Object = TLS.Server(tcpListener, certPem, keyPem);
let tlsClient: TLS = server.accept();
```

### 5. `TLS.wrap(tcpSocket: Object, hostname: String) -> TLS`
Upgrades an existing cleartext `Net` socket connection to an encrypted TLS stream (essential for `STARTTLS` protocols).

```adesh
let socket: Object = Net.tcpConnect("example.com", 443);
let tls: TLS = TLS.wrap(socket, "example.com");
```

### 6. `tls.writeText(str: String) -> Integer` / `tls.writeBytes(bytes: Array<Byte>) -> Integer`
Encrypts and transmits UTF-8 text strings or raw binary byte arrays over the TLS stream. Returns the number of application bytes written.

```adesh
let bytesWritten: i64 = tls.writeText("GET / HTTP/1.1\r\nHost: 1.1.1.1\r\n\r\n");
```

### 7. `tls.readBytes(maxBytes: Integer = 8192) -> Array<Byte>`
Decrypts and reads up to `maxBytes` of raw binary application payload without enforcing UTF-8 encoding.

```adesh
let rawBytes: Array<u8> = tls.readBytes(4096);
```

### 8. `tls.readText(maxBytes: Integer = 8192) -> String`
Decrypts and reads up to `maxBytes` of application payload as a decoded UTF-8 string with strict error handling on invalid UTF-8 sequences.

```adesh
let responseText: String = tls.readText(4096);
```

### 9. `tls.version() -> String`
Returns the negotiated TLS protocol version string (`TLSv1.3` or `TLSv1.2`).

```adesh
let ver: String = tls.version(); // "TLSv1.3"
```

### 10. `tls.alpn() -> String`
Returns the negotiated Application-Layer Protocol Negotiation string (e.g. `h2`, `http/1.1`, or `""`).

```adesh
let alpn: String = tls.alpn(); // "h2"
```

### 11. `tls.peerCertificates() -> Array<Array<Byte>>`
Returns the peer's raw X.509 certificate chain as an array of byte arrays for direct cryptographic verification or pinning checks.

```adesh
let certs: Array<Array<u8>> = tls.peerCertificates();
```

### 12. `tls.trace() -> Array<String>`
Returns diagnostic handshake event logs without exposing secret key material.

```adesh
let events: Array<String> = tls.trace();
```

### 13. `tls.close() -> Boolean`
Sends a graceful `close_notify` alert to the peer and releases all connection resources safely (idempotent across multiple calls).

### 14. File-Based Certificates & Private Keys
Supports loading CA bundles, client certificates, and server credentials directly from filesystem paths or inline PEM literals.

```adesh
// Using file path references directly:
let options: Object = {
    "caPath": "examples/Libraries/tls/cert.pem",
    "clientCertPath": "examples/Libraries/tls/cert.pem",
    "clientKeyPath": "examples/Libraries/tls/key.pem",
    "verifyCertificates": true,
    "verifyHostname": true,
    "minVersion": "TLS1.3"
};

let tls: TLS = TLS.connectWithOptions("127.0.0.1", 8443, options);
```

---

## Example Scripts Directory

| Example Script | Description |
| :--- | :--- |
| [`tls_examples.adesh`](./tls_examples.adesh) | Complete suite demonstrating typed client, options, binary transfers, server listener, and file-based cert paths. |
| [`tls_type_annotation.adesh`](./tls_type_annotation.adesh) | Demonstrating explicit `let tls: TLS = ...` type annotations. |
| [`named_imports_and_types.adesh`](./named_imports_and_types.adesh) | Specific named imports (`import { connectWithOptions, SecurityPolicy } from TLS;`) and dedicated policy types. |
| [`typed_tls_client.adesh`](./typed_tls_client.adesh) | Strongly type-annotated TLS 1.3 client connection (`let tls: TLS = ...`). |
| [`binary_tls_transfer.adesh`](./binary_tls_transfer.adesh) | Transferring arbitrary non-UTF-8 binary bytes using `readBytes` and `writeBytes`. |
| [`tls_server_listener.adesh`](./tls_server_listener.adesh) | Production TLS server listener pipeline (`Net.tcpListen` -> `TLS.Server` -> `server.accept()`). |
| [`certificate_pinning.adesh`](./certificate_pinning.adesh) | SPKI public key pinning, custom certificate validation, and security policy enforcement. |
| [`simple_tls_client.adesh`](./simple_tls_client.adesh) | Basic TLS 1.3 / 1.2 client connection over port 443 with write/read and graceful shutdown. |
| [`tls_object_api.adesh`](./tls_object_api.adesh) | First-class object-oriented TLS connection API (`tls.write()`, `tls.read()`, `tls.close()`). |
| [`connect_with_options.adesh`](./connect_with_options.adesh) | Establishing connections with custom options (ALPN, custom CA, mTLS, min version) and peer certificate chain inspection. |
| [`starttls.adesh`](./starttls.adesh) | Stream wrapping over an established TCP socket for STARTTLS protocol upgrades. |
| [`tls_server.adesh`](./tls_server.adesh) | Single-socket server wrapper supporting both PEM literals and file path references (`cert.pem`, `key.pem`). |
| [`alpn_sni_inspection.adesh`](./alpn_sni_inspection.adesh) | SNI hostname validation and ALPN protocol negotiation inspection (`TLS.version`, `TLS.alpn`). |
| [`tls_error_handling.adesh`](./tls_error_handling.adesh) | Fail-closed edge-case error handling for non-TLS ports and unreachable endpoints. |
| [`advanced_tls_policy.adesh`](./advanced_tls_policy.adesh) | Security policy configuration. |
| [`cert_inspection.adesh`](./cert_inspection.adesh) | X.509 DER/PEM parsing and fingerprints. |
| [`stream_wrapping.adesh`](./stream_wrapping.adesh) | Multi-chunk stream pipeline execution. |

---

## Test Certificates & Key Fixtures

The directory includes self-contained test certificates for local development and unit tests:
- `cert.pem`: X.509 certificate (RSA 2048-bit, self-signed for localhost).
- `key.pem`: PKCS#8 RSA private key.

## How to Run Examples

Run any example script using the AdeshLang CLI runner:

```bash
cargo run --bin adeshlang -- run "examples/Libraries/tls/tls_examples.adesh"
cargo run --bin adeshlang -- run "examples/Libraries/tls/tls_type_annotation.adesh"
cargo run --bin adeshlang -- run "examples/Libraries/tls/typed_tls_client.adesh"
cargo run --bin adeshlang -- run "examples/Libraries/tls/tls_server_listener.adesh"
```
