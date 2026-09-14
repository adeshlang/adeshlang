# http-guide.md

> Consolidated from 8 documentation files on 2026-08-29.

---


---

## Source: http\README.md

# AdeshLang HTTP Standard Library & Application Platform — Documentation Index

Welcome to the comprehensive documentation suite for the **AdeshLang HTTP Standard Library (Phase 4)**.

## Architectural Overview

```text
                    ┌──────────────────────────┐
                    │      AdeshLang HTTP      │
                    │     Application Layer    │
                    │                          │
                    │ REST / Gateway / RPC     │
                    │ WebSocket / SSE          │
                    │ Auth / Cache / Middleware│
                    └────────────┬─────────────┘
                                 │
                    ┌────────────▼─────────────┐
                    │       HTTP Platform      │
                    │                          │
                    │ Client / Server / Router │
                    │ Streaming / Proxy         │
                    │ Security / Observability  │
                    └────────────┬─────────────┘
                                 │
                    ┌────────────▼─────────────┐
                    │      Protocol Core       │
                    │                          │
                    │ HTTP/1.1                 │
                    │ HTTP/2 + HPACK           │
                    │ HTTP/3 + QPACK           │
                    │ QUIC + TLS               │
                    └──────────────────────────┘
```

## Documentation Modules

- [HTTP Client](client.md) — HTTP/1.1, HTTP/2, HTTP/3 Client, ClientBuilder, ClientProfile & RequestBuilder
- [HTTP Server](server.md) — Server lifecycle, listening, health probes, body size limits & graceful shutdown
- [Router 2.0](router.md) — Route matching, groups, nested routers, wildcard & constraint parameters
- [Request Abstraction](request.md) — Properties, getters, streaming bodies (`bodyStream`), JSON parsing & validation
- [Response Abstraction](response.md) — Status codes, headers, ResponseBuilder, JSON serialization & Problem Details
- [Middleware System](middleware.md) — Pipeline composition, error propagation, CORS, Security Headers, CSRF & Rate Limits
- [Streaming I/O](streaming.md) — Zero-copy chunked streaming, request/response bodies, memory safety
- [FileUploads & Multipart](uploads.md) — Streaming form processing, file size limits & MIME verification
- [File Downloads](downloads.md) — Content-Disposition, Range requests & progress tracking
- [Authentication](authentication.md) — Bearer token, Basic Auth, API keys & custom auth middleware
- [Authorization](authorization.md) — Role-Based Access Control (RBAC), scopes & permissions
- [Security & Hardening](security.md) — Request smuggling defense, CRLF prevention, decompression bomb protection
- [HTTP Cache Subsystem](cache.md) — RFC 9111 cache engine, ETags, Cache-Control & Single-Flight coalescing
- [Reverse Proxy 2.0](proxy.md) — Upstream load balancing, health checks, path & header rewriting
- [API Gateway](gateway.md) — Routing, transformation, security & multi-tenant SaaS integration
- [WebSocket Subsystem](websocket.md) — Framing, handshakes, ping/pong & WebSocket Rooms/Broadcasting
- [Server-Sent Events](sse.md) — SSE streaming, event formatting & Last-Event-ID reconnection replay
- [WebTransport & HTTP/3 Datagrams](webtransport.md) — Extended CONNECT, datagrams & QUIC streams
- [Testing & Mocking](testing.md) — In-memory TestClient, MockClient, contract testing & record/replay
- [Performance & Benchmarks](performance.md) — Connection pooling, zero-copy parsing & benchmark suite
- [Troubleshooting & Diagnostics](troubleshooting.md) — Diagnostic tools, HTTP Explain & Security Posture scanning
- [Warning System & Suppression](warnings.md) — Line & column precision, comment directives (`// @allow(warnings)`), CLI flags & env vars
- [Multi-Protocol Engine & Verification](protocols.md) — HTTP/1.1, HTTP/2, HTTP/3, TLS 1.3, X.509 cert generation & live `curl` test commands

---

## Live Multi-Protocol HTTPS Server & `curl` Testing

AdeshLang includes built-in X.509 TLS certificate generation via `Crypto.generateSelfSignedCert()` and server binding via `server.enableTls()`.

### Example Server: `examples/Libraries/http/protocol_https_test_suite.adesh`

```adesh
import Crypto;
import TLS;
import HTTP;

// Automatically generates cert.pem and key.pem in project root
let certPair = Crypto.generateSelfSignedCert();

let router = HTTP.Router();
router.get("/", fn(req, res) {
    return res.json({ status: "ok", protocol: req.version, secure: true });
});

router.get("/h2", fn(req, res) {
    return res.json({ message: "HTTPS HTTP/2 Multiplexed Stream Verified Successfully", protocol: req.version, secure: true });
});

router.get("/h3", fn(req, res) {
    return res.header("Alt-Svc", "h3=\":4045\"; ma=86400").json({ message: "HTTPS HTTP/3 QUIC UDP Stream Verified Successfully", protocol: req.version, quic: true, secure: true });
});

let server = HTTP.Server("127.0.0.1:4045", router);
server.enableTls(certPair.certPem, certPair.keyPem);
server.setProtocols(["h3", "h2", "http/1.1"]);
server.listen(fn() {
    print("HTTPS Server listening on https://127.0.0.1:4045 with TLS 1.3, HTTP/2, and HTTP/3.");
});
```

### Live `curl` Verification Commands

```bash
# 1. Test HTTPS HTTP/1.1 over TLS 1.3
curl.exe -k -v --http1.1 https://127.0.0.1:4045/

# 2. Test HTTPS HTTP/2 Multiplexed Stream
curl.exe -k -v --http2 https://127.0.0.1:4045/h2

# 3. Test HTTPS HTTP/3 Direct QUIC UDP Handshake
curl.exe -k -v --http3-only https://127.0.0.1:4045/h3

# 4. Test Concurrent Parallel HTTP/3 Requests
curl.exe -k -v --http3-only --parallel https://127.0.0.1:4045/h3 https://127.0.0.1:4045/h3 https://127.0.0.1:4045/h3
```



---

## Source: http\client.md

# AdeshLang HTTP Client Architecture

The AdeshLang HTTP Client provides high-performance, non-blocking networking over HTTP/1.1, HTTP/2, and HTTP/3 (QUIC).

## Quick Start

```adesh
import HTTP;

let res = HTTP.get("https://api.example.com/users");
print("Status:", res.status);
print("Body:", res.text());
```

## ClientBuilder 2.0 & Profiles

```adesh
import HTTP;

let client = HTTP.ClientBuilder()
    .http3Preferred()
    .compression()
    .cache()
    .retry()
    .build();
```

### Supported Profiles

- `Balanced`: Optimal balance of throughput, connection pooling, and latency
- `LowLatency`: Optimizes TCP_NODELAY, HTTP/2 multiplexing, and early data
- `HighThroughput`: Maximizes buffer sizes and HTTP/3 QUIC stream concurrency
- `StrictSecurity`: Enforces TLS 1.3, strict certificate validation, HSTS, and Content-Digest verification
- `Mobile`: Minimizes battery usage and network round-trips
- `InternalService`: Optimized for microservice-to-microservice RPC over high-speed networks


---

## Source: http\server.md

# AdeshLang HTTP Server & Router 2.0

The AdeshLang HTTP Server provides multi-threaded request processing, route matching, parameter constraints, and lifecycle health probes.

## Server Usage

```adesh
import HTTP;

let router = HTTP.Router();

router.get("/health", fn(req, res) {
    return res.json({ status: "healthy", uptime: true });
});

let server = HTTP.Server("0.0.0.0:3000", router);
server.listen(fn() {
    print("Server running on port 3000");
});
```

## Router 2.0 Features

- **Route Groups:** `router.group("/api/v1", fn(api) { ... })`
- **Nested Routers:** `api.mount("/users", usersRouter)`
- **Constraint Routing:** `/users/:id<int>` and `/users/:id<uuid>`
- **Automatic Method Support:** Automatic 405 Method Not Allowed with dynamic `Allow` header, OPTIONS preflight fallback, and HEAD responses for GET routes.


---

## Source: http\middleware.md

# AdeshLang HTTP Middleware & Security Subsystem

The middleware framework provides pipeline execution for CORS, Security Headers, CSRF protection, Rate Limiting, and Circuit Breakers.

## Security Middleware

```adesh
import HTTP;

let router = HTTP.Router();

// CORS
router.use(HTTP.CORS({
    origins: ["https://example.com"],
    methods: ["GET", "POST"]
}));

// Security Headers
router.use(HTTP.SecurityHeaders());

// Rate Limiting
router.use(HTTP.RateLimit({ limit: 100, windowSec: 60 }));
```


---

## Source: http\protocols.md

# AdeshLang HTTP/1.1, HTTP/2, and HTTP/3 Protocol Guide

The AdeshLang HTTP engine implements HTTP/1.1, HTTP/2 (binary framing & HPACK), and HTTP/3 (QUIC UDP transport & QPACK).

---

## 1. Protocol Comparison & Feature Matrix

| Feature | HTTP/1.1 | HTTP/2 | HTTP/3 |
| :--- | :--- | :--- | :--- |
| **Wire Format** | Text-based headers (`\r\n`) | 9-Byte Binary Frames | QUIC UDP Packets & Varints |
| **Multiplexing** | No (Head-of-Line blocking) | Yes (Binary Stream IDs) | Yes (Stream-independent QUIC) |
| **Header Compression** | None | HPACK Static & Dynamic | QPACK Out-of-order Compression |
| **Transport** | TCP Sockets | TCP + TLS (ALPN `h2`) | UDP + QUIC (ALPN `h3`) |
| **Loss Recovery** | TCP Window Reset | Connection-wide TCP stall | Per-stream QUIC packet loss recovery |

---

## 2. API Usage & Protocol Enforcement

### 2.1 Client Protocol Enforcement

```adesh
import HTTP;

// Enforce HTTP/1.1 Only
let http1Client = HTTP.ClientBuilder()
    .http1Only()
    .build();

// Enforce HTTP/2 Only (TLS ALPN "h2" binary multiplexing)
let http2Client = HTTP.ClientBuilder()
    .http2Only()
    .build();

// Prefer HTTP/3 over QUIC UDP with 0-RTT connection handshake
let http3Client = HTTP.ClientBuilder()
    .http3Preferred()
    .profile("LowLatency")
    .build();
```

---

### 2.2 Server ALPN Protocol Negotiation

```adesh
import HTTP;

let router = HTTP.Router();
router.get("/protocol", fn(req, res) {
    return res.json({
        negotiatedProtocol: req.version, // "HTTP/1.1", "HTTP/2.0", or "HTTP/3.0"
        scheme: req.scheme
    });
});

let server = HTTP.Server("0.0.0.0:4430", router);

// Enforce ALPN negotiation order: HTTP/3 (QUIC) -> HTTP/2 -> HTTP/1.1
server.setProtocols(["h3", "h2", "http/1.1"]);

server.listen(fn() {
    print("Server listening on port 4430 with multi-protocol support.");
});
```

---

## 3. Terminal Testing (`curl` & Diagnostic Tools)

### 3.1 HTTP/1.1 Testing with `curl`

```bash
curl -v http://127.0.0.1:4040/
```

**Output:**
```http
> GET / HTTP/1.1
> Host: 127.0.0.1:4040
> User-Agent: curl/8.21.0
< HTTP/1.1 200 OK
< content-type: application/json
```

---

### 3.2 HTTP/2 Prior-Knowledge Testing with `curl`

For HTTP/2 over unencrypted TCP prior-knowledge:

```bash
curl -v --http2-prior-knowledge http://127.0.0.1:4042/h2
```

For HTTP/2 over TLS:

```bash
curl -v --http2 https://localhost:4430/h2
```

---

### 3.3 HTTP/3 QUIC Testing & Stream Lifecycle Management

HTTP/3 relies on **QUIC v1 (RFC 9000 & RFC 9114)** over UDP socket transport with TLS 1.3 encryption.

#### Stream Finalization & State Lifecycle
AdeshLang enforces an explicit state machine for HTTP/3 QUIC streams (`Open` $\rightarrow$ `HeadersSent` $\rightarrow$ `BodyStreaming` $\rightarrow$ `Completed`). 

* **Clean Response Completion:** `HEADERS` $\rightarrow$ `DATA` $\rightarrow$ optional `TRAILERS` $\rightarrow$ `FIN` frame (`send.finish()`). Awaiting QUIC `FIN` acknowledgement ensures the server gracefully completes its sending direction without dropping the task handle prematurely or emitting an unsolicited `RESET_STREAM`.
* **Reset Protection:** Streams marked as `Completed` are protected against post-response reset. `RESET_STREAM` is strictly reserved for stream aborts, request cancellations (`H3_REQUEST_CANCELLED`), or internal errors (`H3_INTERNAL_ERROR`).

#### Live `curl` Command Examples (Port 4045)

* **Direct HTTP/3 QUIC Request:**
  ```bash
  curl -k -v --http3-only https://127.0.0.1:4045/h3
  ```

* **Sequential Requests (Connection Reuse):**
  ```bash
  curl -k -v --http3-only https://127.0.0.1:4045/h3 https://127.0.0.1:4045/h3
  ```

* **Concurrent Parallel Requests (Stream Multiplexing):**
  ```bash
  curl -k -v --http3-only --parallel https://127.0.0.1:4045/h3 https://127.0.0.1:4045/h3 https://127.0.0.1:4045/h3
  ```

---

## 4. Diagnostic & Capability Discovery API

```adesh
import HTTP;

// Query target URL capability support
let disc = HTTP.discover("https://127.0.0.1:4045");

print("HTTP/1.1:", disc.http1);
print("HTTP/2:", disc.http2);
print("HTTP/3:", disc.http3);
print("WebSocket:", disc.websocket);
print("SSE:", disc.sse);
```



---

## Source: http\phase3.md

# AdeshLang HTTP Phase 3 Report

Date: 2026-08-15

## Phase 2 Regression

PASS

- `cargo check --lib`: pass
- `cargo test --lib http::tests`: pass

## New Features

Implemented in this Phase 3 increment:

- Secure extension method registry (`HttpMethodRegistry`).
- Expanded QUERY request APIs (`query`, `query_json`, `query_stream`) in core request/client and interpreter-facing HTTP module.
- Structured Fields parser/encoder expansion with bounded safety model.
- Priority header model (`Priority`) with parse/encode and request/response integration.
- Query-method cache policy hardening to avoid accidental caching.
- Additional test coverage for method registry, priority, and structured field safety.

## QUERY

Status: COMPLETE (for this increment)

- `HttpMethod::Query` integrated.
- Safe/idempotent semantics retained.
- Explicit non-default cacheability behavior enforced in cache layer unless explicit cache directives exist.
- Added:
  - `Request::query_with_body`
  - `Request::query_json`
  - `Request::query_stream`
  - `HttpClient::query`
  - `HttpClient::query_json`
  - `HttpClient::query_stream`
  - `HTTP.query_json`
  - `HTTP.query_stream`

## Structured Fields

Status: PARTIAL

- Added support for:
  - Item
  - List
  - Dictionary
  - InnerList
  - Parameters
  - Token/String/ByteSequence/Integer/Decimal/Boolean
- Added encode/decode APIs:
  - `parse_item`, `parse_list`, `parse_dictionary`
  - `encode_item`, `encode_list`, `encode_dictionary`
- Added safety checks for malformed syntax, nesting, member counts, string/byte bounds.

Remaining for full completion:

- Full RFC-level edge-case interoperability suite.
- Dedicated fuzz targets and corpus.
- Additional canonicalization nuances for future dependent specs.

## Priority

Status: PARTIAL

- Added `Priority` model with:
  - `urgency` in range 0..7
  - `incremental` flag
  - extensions map
- Added parse/encode through Structured Fields Dictionary.
- Added request/response helpers for setting and reading Priority header.

Remaining:

- Scheduler 2.0 (FIFO/Strict/WeightedFair/UrgencyBased/LatencyAware/BandwidthAware/Adaptive).
- Dynamic reprioritization hooks for HTTP/2 and HTTP/3 stream-level controls.

## Digest

Status: MISSING

- Content-Digest/Repr-Digest infrastructure not implemented in this increment.

## Message Signatures

Status: MISSING

- HTTP Message Signatures module not implemented in this increment.

## Cache

Status: PARTIAL

- Baseline cache present.
- Hardened to avoid implicit caching for non-cacheable methods like QUERY unless explicit directives are present.

Remaining:

- Targeted cache control fields.
- partitioning
- tags/groups
- purge APIs
- warming
- background revalidation
- consistency modes.

## REST

Status: PARTIAL

- Basic routing exists.

Remaining:

- Resource/controller abstractions
- automatic OPTIONS/Allow generation
- automatic HEAD behavior controls
- version router integration
- validation/schema-first route declarations.

## WebSocket

Status: PARTIAL

- Frame/handshake foundations exist.

Remaining:

- Session manager with rooms/subscriptions/heartbeats/limits/graceful close APIs.

## SSE

Status: PARTIAL

- SSE module exists.

Remaining:

- Broadcast/channel abstractions and management API.

## WebTransport

Status: MISSING/EXPERIMENTAL

- Dedicated WebTransport architecture module not completed in this increment.

## Gateway

Status: PARTIAL

- Reverse proxy/load balancer foundations exist.

Remaining:

- Full gateway policy/middleware transformation and route policy composition.

## Service Mesh

Status: MISSING

- Service client/discovery abstractions not implemented in this increment.

## Security

Status: PARTIAL

- Existing baseline security checks retained.
- Added secure method-registry guardrails.

Remaining:

- Digest verification/signatures/auth framework/key rotation/security scanner/event streams/pinning/posture APIs.

## Observability

Status: PARTIAL

- Baseline explainability pieces exist.

Remaining:

- Metric/tracing/logging adapters and label-cardinality controls.

## Testing

Status: PARTIAL

- Added 3 new HTTP tests.
- Current HTTP test target passes.

Remaining:

- Dedicated `tests/http/phase3/` matrix and broader integration coverage.

## Fuzzing

Status: MISSING

- No new fuzz targets added in this increment.

## Interoperability

Status: MISSING

- No external interop run (curl/nginx/envoy/caddy/haproxy/apache) performed in this increment.

## Benchmarks

Status: MISSING

- No new benchmark suite added in this increment.

## Examples

Status: PARTIAL

- No 50+ example expansion added in this increment.

## Documentation

Status: PARTIAL

- Added:
  - `docs/http/phase3-baseline.md`
  - `docs/http/phase3.md`

Remaining:

- Full Phase 3 document set under `docs/http/` as requested.

## Known Limitations

- Priority scheduling and dynamic reprioritization not yet implemented.
- Structured Fields parser is significantly improved but not yet validated against full conformance corpus.
- Query stream API currently supports streaming body path via callback model; advanced backpressure/cancellation semantics still require deeper integration.
- Phase 3 architecture is far from complete relative to roadmap size.

## Experimental Features

- Any partially wired protocol extension behavior without external interop evidence should be treated as experimental.

## Next Recommended Phase

Phase 3.1 (security-critical foundation) in this order:

1. Digest Fields (Content-Digest, Repr-Digest, preferences, streaming verification, trailers).
2. HTTP Message Signatures (component model + key provider + canonicalization).
3. Client/server middleware pipeline ordering with explicit security phases.
4. Structured Fields conformance and fuzzing expansion.


---

## Source: http\phase3-baseline.md

# AdeshLang HTTP Phase 3 Baseline Report

Date: 2026-08-15

## Baseline Verification Commands

- `cargo check --lib`: PASS
- `cargo test --lib http::tests`: PASS (17 passed, 0 failed)

## Scope Audited

- `src/runtime/stdlib_src/http/`
- `examples/Libraries/http/`
- `tests/`
- `docs/`

## Classification Legend

- COMPLETE: Implemented and covered by tests in current baseline.
- PARTIAL: Some architecture or API exists, but behavior is incomplete.
- BROKEN: Present but failing or semantically incorrect in baseline.
- MISSING: Not present.
- EXPERIMENTAL: Early architecture exists but should not be treated as production-complete.

## Phase 2 Claims Verification

### COMPLETE

- HTTP method model including `QUERY` token parsing and safety/idempotency metadata.
- HTTP status model and categories.
- HTTP/1.1 parser, chunked decoding, trailer parsing, and framing-smuggling validation checks.
- HTTP/2 frame codec and HPACK codec tests.
- HTTP/3 frame codec and QPACK codec tests.
- Request/Response core model and basic client send path.
- Basic reverse proxy/load-balancer skeleton.
- Basic server router with path parameter and wildcard matching.
- Basic WebSocket frame and handshake support.
- Multipart encode/parse baseline.
- Baseline policy and request-budget structures.
- Baseline cache and SingleFlight foundations.

### PARTIAL

- QUIC integration and HTTP/3 runtime transport: frame/codec support exists; full wire-level production parity not validated here.
- HTTP datagrams/Capsule/CONNECT-UDP: architecture references exist in docs and partial code paths; comprehensive operational validation not demonstrated in baseline tests.
- Connection pooling and Happy Eyeballs: basic implementations exist, but no broad stress/interoperability validation in this baseline pass.
- Explainable HTTP and transport autopilot: foundational models exist; not yet a complete observability/control-plane product.
- Server APIs and routing ergonomics: functional primitives exist; advanced route grouping, host routing, and versioning are not complete.
- Security hardening: specific protections exist (CRLF defense, smuggling checks), but advanced scanner/profile/event systems are not present.

### BROKEN

- No Phase 2 regressions found in current baseline command set.

### MISSING

- Large set of advanced platform capabilities listed for Phase 3 (gateway policy composition, service mesh client, full auth challenge framework, message signatures, digest fields, targeted cache-control, advanced middleware/hook pipeline, OpenAPI generation, RPC/GraphQL transport integration, offline queue/sync, advanced observability export controls, etc.).

### EXPERIMENTAL

- Some protocol-adjacent and architecture modules appear as early-stage scaffolding and should remain explicitly marked as non-final until interop/perf/security validation is complete.

## Immediate Regression Fixes Applied Before Phase 3 Expansion

- Corrected `QUERY` cache semantics in cache storage policy:
  non-cacheable methods (including `QUERY`) now require explicit caching directives.

## Initial Phase 3 Slice Implemented (This Session)

- Added `HttpMethodRegistry` with secure runtime registration:
  - `register(name, safe, idempotent, cacheable, body_allowed)`
  - `lookup(name)`
  - `exists(name)`
  - prevents overriding standard method semantics.
- Expanded QUERY support APIs:
  - `Request::query_with_body`, `Request::query_json`, `Request::query_stream`
  - `HttpClient::query`, `HttpClient::query_json`, `HttpClient::query_stream`
  - interpreter-facing `HTTP.query_json`, `HTTP.query_stream`.
- Added modern `Priority` header model:
  - parse/encode support for `u` and `i` plus extension members.
  - request/response helper APIs for priority header set/get.
- Upgraded Structured Fields core:
  - `parse_item`, `parse_list`, `parse_dictionary`
  - `encode_item`, `encode_list`, `encode_dictionary`
  - support for item, dictionary, list, inner list, parameters.
  - safety constraints: bounds on nesting, member count, string/byte sizes, and malformed syntax rejection.
- Expanded HTTP test suite coverage for the above new features.

## Current Confidence

- Baseline build and tests are green for the audited HTTP unit-test scope.
- New Phase 3 foundation features implemented here are tested and integrated into existing HTTP surfaces.
- Most of the 159-point Phase 3 roadmap remains outstanding and should be delivered iteratively as modular, feature-gated slices.


---

## Source: http\warnings.md

# AdeshLang HTTP Platform — Warning Diagnostics & Suppression

Refer to the main [AdeshLang Warning System Documentation](../warnings.md) for full details on compiler diagnostics, line:col location reporting, and warning suppression rules.

## Quick Summary

- **File-level directive:** Add `// @allow(warnings)` at the top of your HTTP script.
- **Line-level directive:** Add `// @allow(unused)` on or above a callback/variable.
- **Param naming:** Name unused HTTP request parameters `_req` instead of `req`.
- **CLI flag:** Run `cargo run --bin adeshlang -- run --no-warnings <script.adesh>`.
- **Env var:** Set `ADESHLANG_DISABLE_WARNINGS=1`.

