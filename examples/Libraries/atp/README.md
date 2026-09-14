# ATP — Adesh Transport Protocol Examples

## Overview

ATP is a **message-oriented, selectively reliable, multiplexed, secure transport
protocol** built on RUDP (Reliable UDP) for the AdeshLang programming language.

It overcomes the fundamental limitations of TCP, UDP, and QUIC:

| Problem with TCP | ATP Solution |
|---|---|
| Byte-stream only — no message boundaries | Message-first design |
| Head-of-line blocking | Independent streams |
| All data is reliable or nothing is | Selective reliability per message |
| Encryption is external (TLS) | Built-in ChaCha20-Poly1305 AEAD |
| No memory bounds (implementation-dependent) | Protocol-level memory budgets |

| Problem with UDP | ATP Solution |
|---|---|
| No reliability | RUDP with ACK ranges, retransmission |
| No ordering | Per-stream ordering (selective) |
| No security | X25519 + AEAD + replay protection |
| No flow control | Two-level flow control with backpressure |
| No congestion control | NewReno-style congestion controller |

| Problem with QUIC | ATP Solution |
|---|---|
| Stream-oriented (not message-oriented) | Messages are first-class |
| Reliability is per-stream (all or nothing) | Reliability is per-message |
| General-purpose (not language-integrated) | Adesh ownership/borrowing/async native |
| No explicit memory budgets | Memory budgets are a protocol concept |

## Files & Projects

| File / Folder | Description |
|---|---|
| [`atp_examples.adesh`](./atp_examples.adesh) | Comprehensive demo of all ATP features (14 demos) |
| [`atp_server.adesh`](./atp_server.adesh) | Practical multi-client echo/RPC/stream server |
| [`atp_client.adesh`](./atp_client.adesh) | Practical client with mixed traffic patterns & keepalive |
| [`atp_production.adesh`](./atp_production.adesh) | Production game-server microservice integrating 9 Adesh libraries |
| [`atp_identity.adesh`](./atp_identity.adesh) | Ed25519 endpoint identity authentication & impostor rejection |
| [`atp_migration.adesh`](./atp_migration.adesh) | Path migration (PATH_CHALLENGE / PATH_RESPONSE) |
| [`atp_retry_handshake.adesh`](./atp_retry_handshake.adesh) | Handshake DoS retry tokens & anti-amplification defense |
| [`atp_priority_streams.adesh`](./atp_priority_streams.adesh) | Two-level DRR priority + per-stream scheduling |
| [`atp_loss_recovery.adesh`](./atp_loss_recovery.adesh) | Reliable delivery under packet loss |
| [`mini_project/`](./mini_project/) | **Real-Time IoT & Fleet Telemetry Command Center** (Complete multi-node project) |

## Running

```bash
# 1. Run the Comprehensive Feature Tour
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_examples.adesh

# 2. Run the Real-Time Fleet Telemetry Mini-Project
cargo run --bin adeshlang -- run examples/Libraries/atp/mini_project/run_demo.adesh

# 3. Production Game Microservice Demo (9 Stdlib Libraries)
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_production.adesh

# 4. Feature-Specific Real Working Demos
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_identity.adesh
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_loss_recovery.adesh
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_migration.adesh
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_priority_streams.adesh
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_retry_handshake.adesh
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_server.adesh
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_client.adesh
```

## Testing (CI)

```bash
# Requires Rust nightly (edition 2024) — see rust-toolchain.toml
cargo test --lib atp
```

## Quick Start

```adesh
import ATP;

// Server
let server = ATP.listen("0.0.0.0", 9000);
let conn = server.accept();

// Client
let conn = ATP.connect("127.0.0.1", 9000);

// Send reliable data (like TCP)
conn.send("important data", true, true);

// Send unreliable data (like UDP)
conn.send("position update", false, false);

// Send a fire-and-forget datagram
conn.datagram("telemetry");

// Open a multiplexed stream
let stream = conn.openStream("high");
stream.send("RPC request", true, true);

// Cancel a message
conn.cancel(1, 1);

// Keepalive
conn.ping();

// Close
conn.close();
```

## Reliability Modes

| Mode | Reliable | Ordered | Use Case |
|---|---|---|---|
| Reliable Ordered | Yes | Yes | Chat, RPC, file transfers, payments |
| Reliable Unordered | Yes | No | Parallel RPC responses, independent operations |
| Unreliable Ordered | No | Yes | Frame-by-frame state (ordered but droppable) |
| Unreliable Unordered | No | No | Position updates, telemetry, sensor data |

## Security

- **Key Agreement**: X25519 ephemeral Diffie-Hellman (forward secrecy)
- **Endpoint Identity**: Ed25519 signatures on handshake flights (`identity_proof` field)
- **Key Derivation**: HKDF-SHA256 with role-separated labels (`atp c ap traffic` / `atp s ap traffic`)
- **Handshake**: Authenticated transcript with HMAC-SHA256 Finished verification
- **DoS Protection**: HMAC retry tokens + per-IP handshake rate limiting
- **Encryption**: ChaCha20-Poly1305 AEAD with header bytes as AAD
- **Replay Protection**: 64-packet sliding window per packet-number space
- **Key Rotation**: Epoch-based key update with previous-epoch overlap

## Cross-Platform

- **OS**: Linux, macOS, Windows, BSD
- **Architecture**: x86_64, ARM64, RISC-V (all pure-Rust, no unsafe code)
- **Network**: IPv4 and IPv6 (auto-detected from target address)
- **Error handling**: WouldBlock, Interrupted, ConnectionReset, ConnectionAborted

## Architecture

```
Application
    ↓
ATP API (atp.listen / atp.connect / conn.send / conn.receive)
    ↓
ATP Engine (background I/O thread)
    ├── Connection Manager (state machine)
    │   ├── Stream Manager (multiplexing)
    │   ├── Message Manager (fragmentation + reassembly)
    │   ├── Flow Control (connection + stream level)
    │   └── Memory Budgets (bounded allocation)
    ├── Reliability Engine (RUDP: ACKs, loss detection, retransmission)
    ├── Security Engine (X25519 + ChaCha20-Poly1305 + replay)
    ├── Congestion Controller (NewReno-style)
    ├── Timer Wheel (efficient timer management)
    └── Wire Codec (varint, packet headers, frames)
        ↓
    UDP Socket (std::net::UdpSocket)
        ↓
    IP → Network
```
