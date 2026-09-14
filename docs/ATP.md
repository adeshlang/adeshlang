# ATP — Adesh Transport Protocol

## Implementation Documentation

**Version:** ATP/1.0  
**Protocol Version:** 1  
**Transport substrate:** UDP/IP  
**Implementation:** Rust modules in `src/runtime/stdlib_src/atp/`  
**Language integration:** AdeshLang standard library (`import ATP;`)  
**Status:** Production-hardened implementation (see [Known Limitations](#known-limitations))

---

## Table of Contents

1. [Overview](#1-overview)
2. [Why ATP Exists](#2-why-atp-exists)
3. [Design Goals](#3-design-goals)
4. [Architecture](#4-architecture)
5. [Source File Map](#5-source-file-map)
6. [Wire Format](#6-wire-format)
7. [Frame Types](#7-frame-types)
8. [Security Engine](#8-security-engine)
9. [Reliability Engine (RUDP)](#9-reliability-engine-rudp)
10. [Memory Management](#10-memory-management)
11. [Flow Control](#11-flow-control)
12. [Congestion Control](#12-congestion-control)
13. [Timer Wheel](#13-timer-wheel)
14. [Message Model](#14-message-model)
15. [Stream Model](#15-stream-model)
16. [Connection Model](#16-connection-model)
17. [Engine](#17-engine)
18. [AdeshLang API](#18-adeshlang-api)
19. [Error Model](#19-error-model)
20. [Cross-Platform Support](#20-cross-platform-support)
21. [Handshake Protocol](#21-handshake-protocol)
22. [Packet Processing Pipeline](#22-packet-processing-pipeline)
23. [Comparison with TCP, UDP, QUIC](#23-comparison-with-tcp-udp-quic)
24. [Examples](#24-examples)
25. [Testing](#25-testing)

---

## 1. Overview

ATP is a **message-oriented, selectively reliable, multiplexed, secure transport
protocol** built on RUDP (Reliable UDP) for the AdeshLang programming language.
It runs as a background I/O engine inside the AdeshLang runtime and exposes a
simple object-oriented API to AdeshLang programs.

### Key Properties

| Property | Description |
|---|---|
| Message-oriented | Application data is framed as discrete messages, not a byte stream |
| Selectively reliable | Each message chooses: reliable/unreliable, ordered/unordered |
| Multiplexed | Multiple independent streams within one connection |
| Secure | X25519 key agreement, ChaCha20-Poly1305 AEAD, replay protection |
| Memory-bounded | Per-connection and per-stream memory budgets with backpressure |
| Low overhead | QUIC-style varints, CID-based short headers, buffer pooling |
| Cross-platform | Pure Rust, no unsafe code, no C dependencies, IPv4 + IPv6 |

---

## 2. Why ATP Exists

### Problems with TCP

| TCP Problem | ATP Solution |
|---|---|
| Byte-stream only, no message boundaries | Message-first design with explicit framing |
| Head-of-line blocking | Independent streams, per-message reliability |
| All-or-nothing reliability | Selective reliability per message |
| Encryption is external (TLS) | Built-in ChaCha20-Poly1305 AEAD |
| No protocol-level memory bounds | Memory budgets are a first-class concept |

### Problems with UDP

| UDP Problem | ATP Solution |
|---|---|
| No reliability | RUDP with ACK ranges, loss detection, retransmission |
| No ordering | Per-stream ordering (selective) |
| No security | X25519 + AEAD + replay protection |
| No flow control | Two-level flow control with backpressure |
| No congestion control | NewReno-style congestion controller |

### Problems with QUIC

| QUIC Problem | ATP Solution |
|---|---|
| Stream-oriented (not message-oriented) | Messages are first-class citizens |
| Reliability is per-stream (all or nothing) | Reliability is per-message |
| General-purpose (not language-integrated) | Native AdeshLang integration |
| No explicit memory budgets | Memory budgets are a protocol concept |

---

## 3. Design Goals

| Goal | How Achieved |
|---|---|
| Low overhead | QUIC-style varints, short header compression, buffer pooling |
| Low memory | Per-connection budgets, sparse reassembly, bounded pools |
| High throughput | Multiplexing, zero-copy buffer pool, batch ACK ranges |
| Low latency | Unreliable mode for time-sensitive data, no head-of-line blocking |
| Selective reliability | 5 reliability modes per message |
| Security | X25519 + HKDF-SHA256 + ChaCha20-Poly1305 + replay window |
| Cross-platform | Pure Rust, std::net, no unsafe, explicit big-endian wire format |

---

## 4. Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    AdeshLang Application                 │
│                   import ATP;                            │
│                   ATP.listen() / ATP.connect()            │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────┐
│                  ATP API (api.rs)                        │
│   Builds Value objects: server, connection, stream      │
│   Bridges AdeshLang ↔ Rust engine                       │
└──────────────────────┬──────────────────────────────────┘
                       │
┌──────────────────────▼──────────────────────────────────┐
│              ATP Engine (engine.rs)                      │
│         Background I/O thread (non-blocking UDP)         │
│  ┌──────────────────────────────────────────────────┐   │
│  │            Connection (connection.rs)            │   │
│  │  ┌────────────┐ ┌──────────┐ ┌────────────────┐  │   │
│  │  │  Streams   │ │ Messages │ │  Flow Control   │  │   │
│  │  │ (stream.rs)│ │(message  │ │   (flow.rs)     │  │   │
│  │  │            │ │   .rs)   │ │                 │  │   │
│  │  └────────────┘ └──────────┘ └────────────────┘  │   │
│  │  ┌────────────────────────────────────────────┐   │   │
│  │  │        Memory Budgets (memory.rs)          │   │   │
│  │  └────────────────────────────────────────────┘   │   │
│  └──────────────────────────────────────────────────┘   │
│  ┌──────────────┐ ┌──────────────┐ ┌────────────────┐  │
│  │ Reliability  │ │  Security    │ │  Congestion    │  │
│  │ (reliability │ │ (security   │ │  (congestion   │  │
│  │    .rs)      │ │    .rs)     │ │    .rs)        │  │
│  └──────────────┘ └──────────────┘ └────────────────┘  │
│  ┌──────────────┐ ┌──────────────┐                      │
│  │ Timer Wheel  │ │  Wire Codec  │                      │
│  │ (timer.rs)   │ │  (wire.rs)   │                      │
│  └──────────────┘ └──────────────┘                      │
└──────────────────────┬──────────────────────────────────┘
                       │
                  UDP Socket
                  (std::net::UdpSocket)
                       │
                  IP → Network
```

### Data Flow (Send Path)

```
Application
  → API: conn.send("hello", true, true)
  → SendRequest::Message queued
  → Engine: process_send_requests()
  → Connection: send_message()
    → Fragment payload
    → Queue DATA frames
    → Check flow control + memory budget
  → Engine: process_outgoing()
    → Encode frames → plaintext
    → Security: encrypt (ChaCha20-Poly1305)
    → Wire: encode short header
    → UDP: send_to()
```

### Data Flow (Receive Path)

```
UDP: recv_from()
  → Engine: handle_packet()
  → Wire: decode packet header
  → Security: decrypt (ChaCha20-Poly1305 + replay check)
  → Wire: decode frames
  → Connection: process_frames()
    → DATA frame → on_data_frame()
      → Flow control check
      → Add fragment to reassembly
      → If complete → reassemble → deliver
    → ACK frame → on_ack_frame()
      → Update reliability tracker
      → Update congestion controller
    → Other frames → state transitions
  → Engine: deliver() → API: receive()
```

---

## 5. Source File Map

All source files live in `src/runtime/stdlib_src/atp/`:

| File | Lines | Responsibility |
|---|---|---|
| `mod.rs` | 35 | Module declarations, cross-platform docs, public exports |
| `errors.rs` | 70 | 8 error categories, `AtpResult<T>` type alias |
| `wire.rs` | 550 | Varint codec, packet headers, 18 frame types, encode/decode |
| `security.rs` | 300 | X25519 DH, HKDF-SHA256, ChaCha20-Poly1305 AEAD, replay, key rotation |
| `reliability.rs` | 400 | RTT estimator, ACK tracker, reliability tracker (RUDP) |
| `memory.rs` | 250 | Buffer pool, memory budgets, backpressure |
| `flow.rs` | 130 | Two-level flow control (connection + stream) |
| `congestion.rs` | 180 | NewReno-style congestion controller |
| `timer.rs` | 160 | Hierarchical timer wheel |
| `message.rs` | 280 | Fragmentation, sparse reassembly, per-fragment ACK tracking |
| `stream.rs` | 220 | Multiplexed streams, priority, cancellation |
| `connection.rs` | 400 | State machine, ties all subsystems together |
| `engine.rs` | 500 | Background I/O thread, handshake, packetization, encryption |
| `api.rs` | 400 | AdeshLang integration, Value objects, builtins |

**Total: ~3,500 lines of Rust**

---

## 6. Wire Format

### Varint Encoding (QUIC-style)

ATP uses QUIC-style variable-length integers for compact encoding of lengths,
IDs, and counters. The top 2 bits of the first byte encode the length:

| Tag (bits 7-6) | Length | Value Range |
|---|---|---|
| `00` | 1 byte | 0 — 63 |
| `01` | 2 bytes | 64 — 16,383 |
| `10` | 4 bytes | 16,384 — 1,073,741,823 |
| `11` | 8 bytes | up to 4,611,686,018,427,387,903 |

**Implementation** (`wire.rs`):

```rust
pub fn encode_varint(mut value: u64) -> Vec<u8> {
    if value <= 63 {
        vec![value as u8]
    } else if value <= 16383 {
        value |= 0x4000;
        let bytes = (value as u16).to_be_bytes();
        vec![bytes[0], bytes[1]]
    } else if value <= 1_073_741_823 {
        value |= 0x8000_0000;
        let bytes = (value as u32).to_be_bytes();
        vec![bytes[0], bytes[1], bytes[2], bytes[3]]
    } else {
        value |= 0xC000_0000_0000_0000;
        value.to_be_bytes().to_vec()
    }
}
```

All multi-byte fields use **big-endian** byte order for cross-platform consistency.

### Packet Header — Long Form

Used during handshake and context creation:

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+
|1|1|  Fixed   |     Bit 7 = 1 (long), Bit 6 = 1 (fixed)
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|                        Version (4 bytes)                      |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Dst CID Len  |       Destination Connection ID ...           |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
| Src CID Len  |       Source Connection ID ...                |
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Packet Number (varint) ...
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Encrypted Payload (frames + AEAD tag) ...
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Fields:**

| Field | Type | Description |
|---|---|---|
| First byte | u8 | `0x80 \| 0x40` = long header with fixed bit |
| Version | u32 BE | Protocol version (currently 1) |
| Dst CID Len | u8 | Length of destination connection ID |
| Dst Connection ID | variable | Destination connection ID |
| Src CID Len | u8 | Length of source connection ID |
| Src Connection ID | variable | Source connection ID |
| Packet Number | varint | Monotonic packet number |
| Payload | bytes | Encrypted frame data + 16-byte AEAD tag |

### Packet Header — Short Form

Used for established connections (context-compressed):

```
 0                   1                   2                   3
 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1 2 3 4 5 6 7 8 9 0 1
+-+-+-+-+-+-+-+-+
|0|1|1|  Ctx   |     Bit 7 = 0 (short), Bit 6 = 1 (fixed), Bit 5 = 1 (ctx ID)
+-+-+-+-+-+-+-+-+
| Context ID   |
+-+-+-+-+-+-+-+-+
| Packet Number (varint) ...
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
|   Encrypted Payload (frames + AEAD tag) ...
+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+
```

**Fields:**

| Field | Type | Description |
|---|---|---|
| First byte | u8 | `0x40 \| 0x20` = short header, fixed bit, context ID present |
| Context ID | u8 | Connection context identifier |
| Packet Number | varint | Monotonic packet number |
| Payload | bytes | Encrypted frame data + 16-byte AEAD tag |

### Header Constants

```rust
pub const ATP_VERSION: u32 = 1;
pub const MAX_PACKET_PAYLOAD: usize = 1450;  // Safe for MTU
pub const FLAG_LONG_HEADER: u8 = 0x80;
pub const FLAG_FIXED_BIT: u8 = 0x40;
pub const FLAG_CONTEXT_ID: u8 = 0x20;
```

---

## 7. Frame Types

ATP defines 18 frame types. Each frame starts with a 1-byte type identifier
followed by type-specific fields encoded with varints.

### Frame Type Registry

| Type | Hex | Name | Direction | Description |
|---|---|---|---|---|
| 1 | `0x01` | `ConnectionInit` | C→S | Connection initiation with ephemeral key |
| 2 | `0x02` | `ConnectionInitAck` | S→C | Server response with ephemeral key |
| 3 | `0x03` | `Handshake` | Both | Key confirmation |
| 4 | `0x04` | `Data` | Both | Application data fragment |
| 5 | `0x05` | `Ack` | Both | Acknowledgement with ranges |
| 6 | `0x06` | `StreamOpen` | Both | Open a new stream |
| 7 | `0x07` | `StreamClose` | Both | Gracefully close a stream |
| 8 | `0x08` | `StreamReset` | Both | Abruptly reset a stream |
| 9 | `0x09` | `MaxData` | Both | Connection-level flow control update |
| 10 | `0x0A` | `MaxStreamData` | Both | Stream-level flow control update |
| 11 | `0x0B` | `Ping` | Both | Keepalive / RTT probe |
| 12 | `0x0C` | `Pong` | Both | Ping response |
| 13 | `0x0D` | `PathChallenge` | Both | Connection migration challenge |
| 14 | `0x0E` | `PathResponse` | Both | Migration response |
| 15 | `0x0F` | `ConnectionClose` | Both | Close connection with reason |
| 16 | `0x10` | `KeyUpdate` | Both | Key rotation epoch update |
| 17 | `0x11` | `Cancel` | Both | Cancel a pending message |
| 18 | `0x12` | `Datagram` | Both | Unreliable datagram (no retransmission) |

### Frame Encodings

#### ConnectionInit (0x01)
```
[1-byte type] [4-byte version] [1-byte src_cid_len] [src_cid] [1-byte eph_key_len] [eph_pubkey] [varint tp_len] [transport_params]
```

#### ConnectionInitAck (0x02)
```
[1-byte type] [4-byte version] [1-byte src_cid_len] [src_cid] [1-byte dst_cid_len] [dst_cid] [1-byte eph_key_len] [eph_pubkey] [varint tp_len] [transport_params]
```

#### Data (0x04)
```
[1-byte type] [varint stream_id] [varint message_id] [varint fragment_num] [varint total_fragments] [1-byte flags] [varint payload_len] [payload bytes]
```

**Data frame flags:**

| Bit | Mask | Name | Meaning |
|---|---|---|---|
| 0 | `0x01` | `RELIABLE` | Message requires retransmission |
| 1 | `0x02` | `ORDERED` | Message requires in-order delivery |
| 2 | `0x04` | `FIN` | Last fragment of message |
| 3 | `0x08` | `FIRST` | First fragment of message |

#### Ack (0x05)
```
[1-byte type] [varint largest_acked] [varint ack_delay] [varint range_count] [range_count × (varint gap, varint range_len)]
```

ACK ranges encode gaps and contiguous blocks of acknowledged packet numbers,
working downward from `largest_acked`. This compresses ACK information when
many consecutive packets are acknowledged.

#### StreamOpen (0x06)
```
[1-byte type] [varint stream_id]
```

#### StreamClose (0x07)
```
[1-byte type] [varint stream_id]
```

#### StreamReset (0x08)
```
[1-byte type] [varint stream_id] [varint reason]
```

#### MaxData (0x09)
```
[1-byte type] [varint max_data]
```

#### MaxStreamData (0x0A)
```
[1-byte type] [varint stream_id] [varint max_data]
```

#### Ping (0x0B) / Pong (0x0C)
```
[1-byte type]
```

#### PathChallenge (0x0D) / PathResponse (0x0E)
```
[1-byte type] [8-byte data]
```

#### ConnectionClose (0x0F)
```
[1-byte type] [varint error_code] [varint reason_len] [reason bytes]
```

#### KeyUpdate (0x10)
```
[1-byte type] [varint epoch]
```

#### Cancel (0x11)
```
[1-byte type] [varint stream_id] [varint message_id]
```

#### Datagram (0x12)
```
[1-byte type] [varint payload_len] [payload bytes]
```

### Frame Aggregation

Multiple frames are encoded into a single packet payload:

```rust
// Encode
let payload = Frame::encode_all(&[frame1, frame2, frame3]);

// Decode
let frames = Frame::decode_all(&payload)?;
```

---

## 8. Security Engine

**File:** `security.rs`  
**Cryptography:** Established algorithms only — ATP does not invent new crypto.

### Cryptographic Stack

| Layer | Algorithm | Crate | Key/Output Size |
|---|---|---|---|
| Key agreement | X25519 (ECDH) | `x25519-dalek` 2.0.1 | 32-byte shared secret |
| Key derivation | HKDF-SHA256 | `hkdf` + `sha2` | 32-byte keys |
| Encryption | ChaCha20-Poly1305 AEAD | `chacha20poly1305` | 32-byte key, 12-byte nonce, 16-byte tag |
| Random | OS RNG | `rand` | — |
| Zeroization | `zeroize` | `zeroize` | Secrets wiped on drop |

### Key Agreement

```rust
// Client generates ephemeral key pair
let client_keys = EphemeralKeyPair::generate();

// Server generates its own pair and computes shared secret
let (server_keys, shared_secret) = server_handshake(&client_pubkey)?;

// Client computes the same shared secret
let shared_secret = client_handshake_complete(&client_keys, &server_pubkey)?;
```

The `EphemeralKeyPair` struct zeroizes the secret key on drop.

### Key Derivation

```rust
let (send_key, recv_key) = derive_keys(
    &shared_secret,  // 32-byte X25519 output
    b"atp-salt",     // HKDF salt
    b"atp-info",     // HKDF info context
)?;
```

HKDF-SHA256 produces two 32-byte keys:
- **send_key**: Used for encrypting outgoing packets
- **recv_key**: Used for decrypting incoming packets

The client and server swap which key is "send" vs "recv" so that each side's
send key matches the other's recv key.

### AEAD Encryption

```rust
// Nonce construction: 4-byte static IV XOR'd with 8-byte packet number
let nonce = build_nonce(&static_iv, packet_number);

// Encrypt: AAD = cleartext packet header bytes
let ciphertext = encrypt_packet(&key, &nonce, plaintext, &header_bytes)?;
// ciphertext = encrypted data + 16-byte Poly1305 auth tag

// Decrypt: verifies authenticity and decrypts
let plaintext = decrypt_packet(&key, &nonce, &ciphertext, &header_bytes)?;
```

The nonce is constructed by XOR'ing a 4-byte static IV (derived from the key)
with the 8-byte big-endian packet number. This ensures each packet number
produces a unique nonce without storing per-packet state.

### Replay Protection

The `SecurityContext` maintains a 64-bit sliding window for received packet
numbers:

```rust
struct DirectionalKeys {
    key: [u8; 32],
    static_iv: [u8; 4],
    largest_pn: u64,       // Highest packet number received
    recv_window: u64,      // Bitmask: bit i = packet (largest_pn - i) seen
}
```

**Algorithm:**
1. If `pn > largest_pn`: shift the window left by `(pn - largest_pn)` bits, set bit 0, update `largest_pn`
2. If `pn <= largest_pn` and `diff = largest_pn - pn < 64`: check bit `diff`. If set → replay detected. If not set → mark as received.
3. If `diff >= 64`: packet is too old → reject as potential replay.

### Key Rotation

```rust
// Rotate to a new epoch
ctx.rotate_keys();
// → Derives new keys from previous keys via HKDF
// → Keeps old keys in prev_send/prev_recv for overlap period
// → Increments epoch counter

// After overlap period:
ctx.drop_prev_keys();
```

Key rotation uses HKDF-SHA256 to derive new epoch keys from the previous
epoch's keys. During the overlap period, the receiver tries decrypting with
both the current and previous epoch keys.

### Security Context Structure

```rust
pub struct SecurityContext {
    pub epoch: u64,
    pub send: DirectionalKeys,
    pub recv: DirectionalKeys,
    pub prev_send: Option<DirectionalKeys>,  // Previous epoch (overlap)
    pub prev_recv: Option<DirectionalKeys>,
    pub is_client: bool,
}
```

---

## 9. Reliability Engine (RUDP)

**File:** `reliability.rs`  
**Model:** Selective reliability — only reliable messages are retransmitted.

### RTT Estimator (RFC 6298 style)

```rust
pub struct RttEstimator {
    pub smoothed_rtt: Duration,   // EWMA of RTT
    pub rttvar: Duration,         // RTT variance
    pub min_rtt: Duration,        // Minimum observed RTT
    pub max_ack_delay: Duration, // Peer's max ACK delay
    pub latest_rtt: Duration,
    pub initialized: bool,
}
```

**Update formula (EWMA):**
```
rttvar = rttvar * 3/4 + |adjusted_rtt - smoothed_rtt| / 4
smoothed_rtt = smoothed_rtt * 7/8 + adjusted_rtt / 8
```

Where `adjusted_rtt = max(rtt_sample - ack_delay, 0)`.

**RTO (Retransmission Timeout):**
```
RTO = max(smoothed_rtt + rttvar * 4 + max_ack_delay, 25ms)
RTO = min(RTO, 60,000ms)
```

### ACK Tracker (Receive Side)

Tracks received packet numbers and builds ACK frames with ranges:

```rust
pub struct AckTracker {
    received: BTreeSet<u64>,     // Sorted set of received packet numbers
    largest: u64,                // Largest packet number received
    largest_time: Option<Instant>,
    pub ack_queued: bool,        // Whether an ACK should be sent
    packets_since_ack: u32,
    ack_frequency: u32,         // ACK every ~10 packets
}
```

**ACK generation:**
1. Packets are inserted into a sorted `BTreeSet`
2. When `ack_queued` is true, `build_ack_frame()` groups consecutive packet numbers into ranges
3. Ranges are encoded as `(gap, range_len)` pairs, working downward from `largest_acked`
4. Maximum 64 ACK ranges per frame

### Reliability Tracker (Send Side)

```rust
pub struct ReliabilityTracker {
    pub in_flight: BTreeMap<u64, SentPacket>,  // Packet number → metadata
    pub lost: Vec<SentPacket>,                 // Detected as lost
    pub rtt: RttEstimator,
    pub next_packet_number: u64,
    pub bytes_in_flight: usize,
}
```

**Loss detection — two mechanisms:**

1. **Gap-based (N-ACK rule):** When an ACK for packet N arrives, any in-flight packet with number ≤ (N - 3) is declared lost.

2. **Time-based:** Any in-flight packet older than the current RTO is declared lost.

**ACK processing:**
```rust
pub fn process_ack(
    &mut self,
    largest_acked: u64,
    ack_delay_ms: u64,
    ranges: &[(u64, u64)],
    now: Instant,
) -> Vec<u64>
```

Reconstructs the set of acknowledged packet numbers from the ranges, removes
them from `in_flight`, updates the RTT estimator with the first acked packet's
sample, and then runs gap-based loss detection.

### Sent Packet Metadata

```rust
pub struct SentPacket {
    pub packet_number: u64,
    pub stream_id: u64,
    pub message_id: u64,
    pub fragment_num: u64,
    pub sent_time: Instant,
    pub bytes_sent: usize,
    pub is_reliable: bool,
    pub tx_count: u32,          // Transmission count
}
```

---

## 10. Memory Management

**File:** `memory.rs`  
**Principle:** No remote peer can force unbounded allocation.

### Buffer Pool

```rust
pub struct BufferPool {
    free: Mutex<Vec<Vec<u8>>>,       // Reusable free buffers
    default_capacity: usize,         // Default buffer size
    max_pool_size: usize,            // Max buffers to keep
    total_allocated: AtomicUsize,   // Total bytes allocated
    bytes_in_use: AtomicUsize,       // Bytes checked out
}
```

**PooledBuffer** returns to the pool on drop:

```rust
pub struct PooledBuffer {
    data: Vec<u8>,
    len: usize,
    pool: Option<Arc<BufferPool>>,  // Returns to pool on drop
}
```

- `acquire()` — Get a buffer from the pool (or allocate new)
- `acquire_with_capacity(min_cap)` — Get a buffer with at least `min_cap` bytes
- `return_buffer()` — Return a buffer (called automatically on drop)
- `into_vec()` — Detach from pool and take ownership

### Memory Budget

```rust
pub struct MemoryBudget {
    limit: usize,   // Maximum bytes allowed
    used: usize,    // Current bytes in use
}
```

- `try_reserve(amount)` — Reserve bytes, returns error if over budget
- `release(amount)` — Release bytes back
- `should_apply_backpressure()` — Returns true when usage > 90%
- `pct_used()` — Usage as a fraction (0.0 to 1.0)

### Connection Memory Configuration

```rust
pub struct ConnectionMemoryConfig {
    pub total_budget: usize,         // 16 MB default
    pub max_stream_memory: usize,    // 2 MB per stream
    pub max_reassembly_memory: usize, // 4 MB for reassembly
    pub max_send_queue: usize,       // 2 MB send queue
    pub max_receive_queue: usize,    // 2 MB receive queue
}
```

### Backpressure

When `MemoryBudget::should_apply_backpressure()` returns true (>90% used),
the connection should stop accepting new data from the application until
memory is released. This prevents a slow consumer from causing unbounded
memory growth.

---

## 11. Flow Control

**File:** `flow.rs`  
**Model:** Two-level credit-based flow control.

### Connection-Level Flow Control

```rust
pub struct ConnectionFlowControl {
    pub send: FlowWindow,   // How much we can send (peer's limit on us)
    pub recv: FlowWindow,   // How much the peer can send (our limit on them)
    initial_recv_window: u64,
}
```

### Stream-Level Flow Control

```rust
pub struct StreamFlowControl {
    pub send: FlowWindow,
    pub recv: FlowWindow,
    initial_recv_window: u64,
}
```

### FlowWindow

```rust
pub struct FlowWindow {
    max: u64,       // Maximum bytes allowed
    consumed: u64,  // Bytes consumed so far
}
```

- `try_consume(amount)` — Consume from the window, error if over budget
- `update_max(new_max)` — Increase the window (from MAX_DATA frame)
- `available()` — Remaining bytes

### Automatic Window Updates

When the receive window drops below half of its initial value, a `MAX_DATA`
(or `MAX_STREAM_DATA`) frame is automatically queued:

```rust
pub fn should_send_max_data(&self) -> Option<u64> {
    if self.recv.available() < self.initial_recv_window / 2 {
        Some(self.recv.consumed + self.initial_recv_window)
    } else {
        None
    }
}
```

---

## 12. Congestion Control

**File:** `congestion.rs`  
**Algorithm:** NewReno-style (slow start, congestion avoidance, fast recovery)

### Congestion States

```
                    ┌────────────┐
                    │ Slow Start │  (exponential growth)
                    └─────┬──────┘
                          │ cwnd ≥ ssthresh
                          ▼
                    ┌─────────────────────┐
                    │ Congestion Avoidance │  (linear growth)
                    └─────┬───────────────┘
                          │ loss detected
                          ▼
                    ┌──────────────┐
                    │ Fast Recovery │
                    └─────┬────────┘
                          │ recovery complete
                          ▼
                    ┌─────────────────────┐
                    │ Congestion Avoidance │
                    └─────────────────────┘
```

### CongestionController

```rust
pub struct CongestionController {
    cwnd: usize,            // Congestion window (bytes)
    ssthresh: usize,        // Slow-start threshold
    state: CongestionState,  // Current state
    initial_window: usize,   // ~14 KB (10 × MSS)
    minimum_window: usize,   // 2 × MSS (safety floor)
    maximum_window: usize,   // Cap
    bytes_acked: usize,      // For congestion avoidance accounting
    mss: usize,              // Maximum segment size
}
```

### State Machine

| Event | Slow Start | Congestion Avoidance | Fast Recovery |
|---|---|---|---|
| **ACK received** | `cwnd += acked_bytes` (exponential) | `cwnd += MSS` per cwnd bytes acked (linear) | `cwnd += MSS` (inflate) |
| **cwnd ≥ ssthresh** | → Congestion Avoidance | — | — |
| **Loss detected** | `ssthresh = cwnd/2`, `cwnd = ssthresh`, → Congestion Avoidance | Same | Same |
| **Timeout (RTO)** | `ssthresh = cwnd/2`, `cwnd = initial_window`, → Slow Start | Same | Same |
| **Enter fast recovery** | `ssthresh = cwnd/2`, `cwnd = ssthresh + 3×MSS`, → Fast Recovery | Same | — |
| **Exit fast recovery** | — | — | `cwnd = ssthresh`, → Congestion Avoidance |

---

## 13. Timer Wheel

**File:** `timer.rs`  
**Design:** Hierarchical timer wheel — avoids creating an OS timer per packet.

### Timer Kinds

| Kind | Description |
|---|---|
| `AckDelay` | Send a delayed ACK |
| `Retransmission` | Retransmission timeout |
| `Handshake` | Handshake timeout |
| `Idle` | Idle connection timeout |
| `Keepalive` | Send a PING |
| `PathValidation` | Connection migration validation |
| `KeyUpdate` | Key rotation timer |
| `CloseDrain` | Connection close drain period |

### TimerWheel

```rust
pub struct TimerWheel {
    slots: Vec<Vec<TimerEntry>>,  // Fixed number of slots
    current_tick: usize,          // Current tick position
    tick_duration: Duration,      // Resolution (default 10ms)
    last_advance: Instant,
    timer_count: usize,
}
```

**Operations:**
- `schedule(deadline, kind, stream_id, message_id)` — Add a timer
- `advance(now)` — Advance the wheel, return expired timers
- `cancel_stream(stream_id)` — Cancel all timers for a stream
- `cancel_message(stream_id, message_id)` — Cancel timers for a specific message
- `cancel_where(predicate)` — Cancel timers matching a predicate

Timers are hashed into slots by their deadline. When `advance()` is called,
all timers in the slots between the old and new tick position are examined.
Expired timers are returned; unexpired ones are re-hashed into future slots.

---

## 14. Message Model

**File:** `message.rs`  
**Principle:** Messages are the primary application-level abstraction.

### Reliability Modes

| Mode | Reliable | Ordered | Use Case |
|---|---|---|---|
| `Unreliable` | No | No | Position updates, telemetry, sensor data |
| `Reliable` | Yes | No | Parallel RPC responses, independent operations |
| `ReliableUnordered` | Yes | No | Same as Reliable |
| `ReliableOrdered` | Yes | Yes | Chat, RPC, file transfers, payments |
| `OrderedBestEffort` | No | Yes | Frame-by-frame state (ordered but droppable) |

**Flag mapping:**

```rust
impl ReliabilityMode {
    pub fn from_flags(flags: u8) -> Self {
        let reliable = flags & DATA_FLAG_RELIABLE != 0;
        let ordered = flags & DATA_FLAG_ORDERED != 0;
        match (reliable, ordered) {
            (true, true) => ReliabilityMode::ReliableOrdered,
            (true, false) => ReliabilityMode::Reliable,
            (false, true) => ReliabilityMode::OrderedBestEffort,
            (false, false) => ReliabilityMode::Unreliable,
        }
    }
}
```

### Fragmentation

Large messages are split into packet-sized fragments:

```rust
pub fn fragment_payload(
    payload: &[u8],
    max_fragment_size: usize,  // Default: ~1386 bytes (1450 - 64 header overhead)
) -> (Vec<Vec<u8>>, u64)
```

Each fragment is carried in a `Data` frame with:
- `fragment_num`: Index within the message (0-based)
- `total_fragments`: Total fragment count
- `flags`: `FIRST` on fragment 0, `FIN` on the last fragment, plus reliability flags

### Send-Side Message Tracking

```rust
pub struct SendMessage {
    pub message_id: u64,
    pub stream_id: u64,
    pub total_fragments: u64,
    pub reliability: ReliabilityMode,
    pub acked_fragments: BTreeMap<u64, bool>,
    pub total_size: usize,
    pub complete: bool,
    pub cancelled: bool,
}
```

- `ack_fragment(fragment_num)` — Mark a fragment as acknowledged
- `unacked_fragments()` — List fragments needing retransmission
- `all_acked()` — True when all fragments are acknowledged (reliable messages)
- `cancel()` — Mark as cancelled

### Receive-Side Sparse Reassembly

```rust
pub struct MessageReassembly {
    pub message_id: u64,
    pub stream_id: u64,
    pub total_fragments: u64,
    pub total_size: usize,
    pub reliability: ReliabilityMode,
    fragments: BTreeMap<u64, Vec<u8>>,  // Only received fragments stored
    budget: MemoryBudget,                // Bounded by max_reassembly_memory
    pub complete: bool,
}
```

**Key property:** Only received fragments consume memory. If fragments 0, 1,
and 3 arrive but fragment 2 is lost, only fragments 0, 1, and 3 are stored.
Fragment 2 is listed in `missing_fragments()` for retransmission requests.

- `add_fragment(num, payload)` — Add a fragment, returns `Ok(true)` if complete
- `reassemble()` — Concatenate all fragments in order
- `missing_fragments()` — List which fragments haven't arrived
- `memory_used()` — Current memory consumption

---

## 15. Stream Model

**File:** `stream.rs`  
**Design:** Independent, multiplexed channels within a single connection.

### Stream States

```
    ┌──────┐
    │ Idle │
    └──┬───┘
       │ open()
       ▼
    ┌──────────┐
    │   Open   │◄──────────────────┐
    └──┬───┬───┘                   │
       │   │                       │
  close_  close_                   │
  local() remote()                 │
       │   │                       │
       ▼   ▼                       │
┌────────────┐  ┌────────────────┐ │
│HalfClosed  │  │HalfClosed      │ │
│  Local     │  │  Remote        │ │
└─────┬──────┘  └───────┬────────┘ │
      │ close_remote()  │ close_local()
      ▼                 ▼
    ┌────────┐         ┌────────┐
    │ Closed │         │ Closed │
    └────────┘         └────────┘

    Any state ──reset()──► ┌────────┐
                           │ Reset  │
                           └────────┘
```

### Priority Levels

| Level | Value | Use Case |
|---|---|---|
| `Low` | 0 | Background file transfers, log streaming |
| `Medium` | 1 | Default — chat, general data |
| `High` | 2 | RPC, interactive requests |
| `Critical` | 3 | Control messages, emergency alerts |

### AtpStream

```rust
pub struct AtpStream {
    pub stream_id: u64,
    pub state: StreamState,
    pub priority: Priority,
    pub flow: StreamFlowControl,
    pub send_messages: BTreeMap<u64, SendMessage>,
    pub next_message_id: u64,
    pub send_queue_bytes: usize,
    pub recv_bytes: usize,
}
```

**Operations:**
- `open()` — Transition to Open
- `close_local()` / `close_remote()` — Half-close
- `reset()` — Abrupt reset, clear all queued messages
- `can_send()` — True if state allows sending
- `allocate_message_id()` — Get next message ID
- `register_message(msg)` — Track an outgoing message
- `ack_fragment(message_id, fragment_num)` — Mark fragment as acked
- `cancel_message(message_id)` — Cancel and remove a message
- `cleanup_completed()` — Remove completed/cancelled messages
- `messages_with_unacked()` — Messages needing retransmission

---

## 16. Connection Model

**File:** `connection.rs`  
**Role:** Ties all ATP subsystems together.

### Connection State Machine

```
┌─────────┐     initiate      ┌──────────────┐    handshake     ┌─────────────┐
│ Initial │ ────────────────► │ Handshaking  │ ──────────────► │ Established │
└─────────┘                   └──────────────┘                  └──────┬──────┘
                                                                     │
                                    ┌────────────────────────────────┤
                                    │                                │
                               close()                          migrate()
                                    │                                │
                                    ▼                                ▼
                              ┌─────────┐                     ┌──────────┐
                              │ Closing │                     │ Migrating │
                              └────┬────┘                     └─────┬────┘
                                   │                                │
                                   ▼                          path validated
                              ┌─────────┐                            │
                              │  Closed │◄───────────────────────────┘
                              └─────────┘
```

### Transport Parameters

Exchanged during handshake to negotiate limits:

```rust
pub struct TransportParams {
    pub max_streams: u64,          // 256
    pub max_data: u64,             // 16 MB (connection-level flow control)
    pub max_stream_data: u64,      // 2 MB (stream-level flow control)
    pub max_message_size: u64,     // 64 MB
    pub idle_timeout: Duration,    // 30 seconds
    pub max_packet_size: u64,      // 1450 bytes
}
```

### AtpConnection

```rust
pub struct AtpConnection {
    pub local_conn_id: Vec<u8>,
    pub remote_conn_id: Vec<u8>,
    pub state: ConnectionState,
    pub is_client: bool,
    pub remote_addr: SocketAddr,
    pub security: Option<SecurityContext>,
    pub streams: BTreeMap<u64, AtpStream>,
    pub reliability: ReliabilityTracker,
    pub ack_tracker: AckTracker,
    pub congestion: CongestionController,
    pub flow: ConnectionFlowControl,
    pub mem_config: ConnectionMemoryConfig,
    pub memory_budget: MemoryBudget,
    pub reassembly: BTreeMap<(u64, u64), MessageReassembly>,
    pub timers: TimerWheel,
    pub local_params: TransportParams,
    pub peer_params: TransportParams,
    pub last_activity: Instant,
    pub pending_frames: Vec<Frame>,
    pub context_id: u8,
    pub context_established: bool,
}
```

### Key Operations

| Method | Description |
|---|---|
| `open_stream(priority)` | Open a new stream, queue `StreamOpen` frame |
| `close_stream(stream_id)` | Gracefully close, queue `StreamClose` frame |
| `reset_stream(stream_id, reason)` | Abrupt reset, queue `StreamReset` frame |
| `send_message(stream_id, payload, reliability, now)` | Fragment + queue `Data` frames |
| `send_datagram(payload)` | Queue `Datagram` frame (unreliable) |
| `cancel_message(stream_id, message_id)` | Queue `Cancel` frame, free memory |
| `on_data_frame(...)` | Process incoming data fragment, reassemble |
| `on_ack_frame(...)` | Process ACK, update reliability + congestion |
| `process_frames(frames, now)` | Dispatch all frame types |
| `drain_pending_frames()` | Get outgoing frames for packetization |
| `check_timeouts(now)` | Process timer expirations |
| `schedule_keepalive(now)` | Schedule a PING timer |
| `schedule_idle_check(now)` | Schedule an idle timeout check |

### ReceivedData

```rust
pub enum ReceivedData {
    Message { stream_id: u64, message_id: u64, data: Vec<u8> },
    Datagram { data: Vec<u8> },
}
```

---

## 17. Engine

**File:** `engine.rs`  
**Role:** Background I/O thread that manages UDP sockets, handshakes,
packetization, encryption, and delivery.

### Engine Creation

```rust
// Server (listening)
let engine = AtpEngine::new_server(listen_addr)?;
engine.start();

// Client (connecting)
let engine = AtpEngine::new_client(target_addr)?;
engine.start();
```

**IPv4/IPv6 auto-detection:** The client bind address is chosen to match the
target's address family — `0.0.0.0:0` for IPv4 targets, `[::]:0` for IPv6
targets. This ensures the socket can reach the target on all platforms.

### Event Loop

```
┌─────────────────────────────────────────────────────────┐
│                    run_loop()                            │
│                                                         │
│  while running:                                         │
│    1. process_send_requests()  ← from API               │
│    2. process_outgoing()       → encrypt + send UDP     │
│    3. process_incoming()       ← recv UDP + decrypt     │
│    4. process_timers()         → retransmission, idle   │
│    5. sleep(1ms)               → yield CPU              │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

### SendRequest (API → Engine)

```rust
pub enum SendRequest {
    Message { stream_id: u64, data: Vec<u8>, reliable: bool, ordered: bool },
    Datagram { data: Vec<u8> },
    OpenStream { priority: Priority },
    CloseStream { stream_id: u64 },
    Cancel { stream_id: u64, message_id: u64 },
    Close,
    Ping,
}
```

### Delivery (Engine → API)

```rust
pub enum Delivery {
    Message { stream_id: u64, message_id: u64, data: Vec<u8> },
    Datagram { data: Vec<u8> },
    StreamOpen { stream_id: u64 },
    StreamClose { stream_id: u64 },
    Connected,
    Closed { reason: String },
    Error { error: AtpError },
}
```

### Outgoing Packet Pipeline

```
1. Drain pending frames from connection
2. Check if ACK should be sent → queue Ack frame
3. Check flow control → queue MaxData / MaxStreamData frames
4. Encode all frames into plaintext payload
5. If payload + 64 > MAX_PACKET_PAYLOAD:
     → Split: one frame per packet
   Else:
     → Single packet with all frames
6. For each packet:
     a. Allocate packet number
     b. Build short header
     c. Encrypt: ChaCha20-Poly1305 (AAD = header bytes)
     d. Track sent packet in reliability tracker
     e. Send via UDP
```

### Incoming Packet Pipeline

```
1. Receive UDP datagram (non-blocking, up to 32 per cycle)
2. Decode packet header (long or short)
3. If long header (handshake):
     a. Decode frames (plaintext during handshake)
     b. Process ConnectionInit / ConnectionInitAck / Handshake
4. If short header (established):
     a. Find connection by source address
     b. Decrypt with ChaCha20-Poly1305 (try current + previous epoch)
     c. Check replay window
     d. Record packet number for ACK
     e. Decode frames
     f. Process frames (Data, Ack, StreamOpen, etc.)
     g. Deliver received messages/datagrams to API
```

### Engine Metrics

```rust
pub struct EngineMetrics {
    pub connections: usize,
    pub established: usize,
    pub streams: usize,
    pub memory_used: usize,
}
```

---

## 18. AdeshLang API

**File:** `api.rs`  
**Integration:** ATP is exposed as a standard library module via `import ATP;`.

### Import

```adesh
import ATP;
```

### Module Functions

#### `ATP.listen(host, port) → Server`

Creates a listening ATP engine on the given host and port.

```adesh
let server = ATP.listen("0.0.0.0", 9000);
// IPv6:
let server = ATP.listen("::", 9000);
```

#### `ATP.connect(host, port) → Connection`

Creates a client ATP engine and connects to the given host and port.

```adesh
let conn = ATP.connect("127.0.0.1", 9000);
```

### Server Object

| Method | Signature | Description |
|---|---|---|
| `accept()` | `() → Connection \| null` | Accept a new connection (returns null if none pending) |
| `close()` | `() → bool` | Stop the server and release the socket |
| `metrics()` | `() → object` | Return `{ connections, established, streams, memoryUsed }` |

```adesh
let server = ATP.listen("0.0.0.0", 9000);
let conn = server.accept();
if (conn != null) {
    let msg = conn.receive();
    // ...
}
server.close();
```

### Connection Object

| Method | Signature | Description |
|---|---|---|
| `send(data, [reliable], [ordered])` | `(string\|bytes, bool?, bool?) → bool` | Send a message on the default stream (stream 1) |
| `receive()` | `() → object \| null` | Poll for received data |
| `openStream([priority])` | `(string\|number?) → Stream` | Open a new multiplexed stream |
| `datagram(data)` | `(string\|bytes) → bool` | Send an unreliable datagram (no stream, no retransmission) |
| `ping()` | `() → bool` | Send a PING for keepalive/RTT measurement |
| `cancel(streamId, messageId)` | `(u64, u64) → bool` | Cancel a pending message |
| `close()` | `() → bool` | Close the connection and stop the engine |
| `metrics()` | `() → object` | Return engine metrics |

**`send()` parameters:**
- `data`: String or byte array (`[u8]`)
- `reliable`: Boolean, default `true`. If true, message is retransmitted until acked.
- `ordered`: Boolean, default `true`. If true, message requires in-order delivery.

**`receive()` return values:**

| Delivery Type | Return Object |
|---|---|
| Message | `{ streamId, messageId, data: [u8] }` |
| Datagram | `{ data: [u8], type: "datagram" }` |
| Connected | `{ type: "connected" }` |
| Closed | `{ type: "closed", reason: string }` |
| StreamOpen | `{ type: "streamOpen", streamId: u64 }` |
| StreamClose | `{ type: "streamClose", streamId: u64 }` |
| Error | `{ type: "error", error: string }` |
| (none) | `null` |

```adesh
let conn = ATP.connect("127.0.0.1", 9000);

// Send reliable ordered message (like TCP)
conn.send("important data", true, true);

// Send unreliable unordered message (like UDP)
conn.send("position update", false, false);

// Send a fire-and-forget datagram
conn.datagram("telemetry");

// Open a high-priority stream
let stream = conn.openStream("high");
stream.send("RPC request", true, true);

// Receive
let msg = conn.receive();
if (msg != null) {
    print("Received on stream", msg.streamId);
}

// Cancel a pending message
conn.cancel(1, 5);

// Keepalive
conn.ping();

// Close
conn.close();
```

### Stream Object

| Method | Signature | Description |
|---|---|---|
| `send(data, [reliable], [ordered])` | `(string\|bytes, bool?, bool?) → bool` | Send a message on this stream |
| `receive()` | `() → object \| null` | Poll for messages on this stream |
| `close()` | `() → bool` | Close the stream |

**Priority values for `openStream()`:**

| String | Number | Level |
|---|---|---|
| `"low"` | 0 | Low |
| `"medium"` | 1 | Medium (default) |
| `"high"` | 2 | High |
| `"critical"` | 3 | Critical |

### Internal Architecture

The API layer maintains a static registry of active engines:

```rust
static ENGINES: Lazy<Mutex<HashMap<u64, Arc<AtpEngine>>>> = ...;
```

Each `listen()` or `connect()` call creates an `AtpEngine`, starts its
background thread, and stores it in the registry with a unique handle.
The returned `Value::Object` contains closures that capture the engine
`Arc` and submit requests / poll deliveries through the engine's
thread-safe channels.

---

## 19. Error Model

**File:** `errors.rs`

### Error Categories

```rust
pub enum AtpError {
    Transport(String),    // Generic transport failure
    Protocol(String),     // Malformed frame, invalid state
    Security(String),     // Auth, decryption, replay
    Connection(String),   // Closed, handshake failed, migration
    Stream(String),       // Reset, limit exceeded, invalid ID
    Message(String),      // Too large, cancelled, expired
    Timeout(String),      // Handshake, idle, retransmission
    Memory(String),       // Budget exceeded
}
```

Each variant has a constructor helper (e.g., `AtpError::transport(msg)`)
and the `Display` implementation formats as `CategoryError: message`.

### Result Type

```rust
pub type AtpResult<T> = Result<T, AtpError>;
```

Used throughout the engine. `std::io::Error` is automatically converted
to `AtpError::Transport`.

---

## 20. Cross-Platform Support

### Operating Systems

| OS | Status | Notes |
|---|---|---|
| Linux | Full support | Standard non-blocking UDP |
| macOS | Full support | Standard non-blocking UDP |
| Windows | Full support | ConnectionReset/ConnectionAborted handling for ICMP errors |
| BSD | Full support | Standard non-blocking UDP |

### CPU Architectures

| Architecture | Status | Notes |
|---|---|---|
| x86_64 | Full support | Primary development platform |
| ARM64 | Full support | Pure Rust, no arch-specific code |
| RISC-V | Full support | Pure Rust, no arch-specific code |
| x86 | Full support | Pure Rust |

### How Cross-Platform Is Achieved

1. **All networking** uses `std::net::UdpSocket` — no platform-specific APIs
2. **All cryptography** uses pure-Rust crates (`x25519-dalek`, `chacha20poly1305`, `hkdf`, `sha2`) — no C dependencies
3. **Wire format** uses explicit big-endian byte order — no endianness ambiguity
4. **No unsafe code** — all memory safety is enforced by the Rust compiler
5. **IPv4 and IPv6** both fully supported — address family auto-detected from target
6. **Non-blocking I/O** — `set_nonblocking(true)` works on all platforms
7. **Error handling** — WouldBlock, Interrupted, ConnectionReset, ConnectionAborted all handled

### Windows-Specific Handling

On Windows, a previous `send_to()` that triggers an ICMP port-unreachable
can surface as `ConnectionReset` on the next `recv_from()`. ATP handles
this by catching `ConnectionReset` and `ConnectionAborted` errors and
continuing to the next datagram rather than treating them as fatal.

```rust
Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionReset => continue,
Err(ref e) if e.kind() == std::io::ErrorKind::ConnectionAborted => continue,
```

---

## 21. Handshake Protocol

### 1-RTT Handshake with X25519

```
Client                                          Server
  │                                               │
  │  ──── CONNECTION_INIT ──────────────────────►  │
  │  Long header:                                 │
  │    version, src_cid, eph_pubkey, params       │
  │                                               │
  │  ◄──── CONNECTION_INIT_ACK ─────────────────  │
  │  Long header:                                 │
  │    version, src_cid, dst_cid,                  │
  │    eph_pubkey, params                          │
  │                                               │
  │  Both sides derive shared secret via X25519   │
  │  Both sides derive keys via HKDF-SHA256        │
  │                                               │
  │  ──── HANDSHAKE (key confirm) ──────────────►  │
  │  Long header, encrypted                        │
  │                                               │
  │  ◄──── HANDSHAKE (key confirm) ──────────────  │
  │  Long header, encrypted                        │
  │                                               │
  │  ═══════════ ESTABLISHED ═══════════════════  │
  │  (Short headers + AEAD from here on)          │
```

### Handshake Steps (Implementation)

**Client side (`initiate_handshake`):**
1. Generate ephemeral X25519 key pair
2. Generate 8-byte local connection ID
3. Build `ConnectionInit` frame with version, src_cid, ephemeral pubkey, transport params
4. Encode as long header packet (plaintext during handshake)
5. Send to target via UDP
6. Create connection in `Handshaking` state

**Server side (`handle_init`):**
1. Receive `ConnectionInit` from client
2. Generate server ephemeral X25519 key pair
3. Compute shared secret via X25519 DH
4. Derive send/recv keys via HKDF-SHA256
5. Create `SecurityContext` (server role)
6. Generate 8-byte server connection ID
7. Create connection in `Handshaking` state
8. Build `ConnectionInitAck` frame with server's ephemeral pubkey
9. Send `ConnectionInitAck` + `Handshake` (key confirmation) to client

**Client side (`handle_init_ack`):**
1. Receive `ConnectionInitAck` from server
2. Retrieve stored ephemeral key pair
3. Compute shared secret via X25519 DH with server's pubkey
4. Derive send/recv keys via HKDF-SHA256
5. Create `SecurityContext` (client role)
6. Send `Handshake` (key confirmation) to server
7. Transition to `Established`

**Server side (on receiving `Handshake`):**
1. Receive `Handshake` frame from client
2. Transition from `Handshaking` to `Established`
3. Deliver `Connected` event to application

---

## 22. Packet Processing Pipeline

### Complete Send Path

```
Application calls conn.send("hello", true, true)
  │
  ▼
API (api.rs): submit_request(SendRequest::Message { ... })
  │  → Thread-safe: pushes to Arc<Mutex<Vec<SendRequest>>>
  ▼
Engine (engine.rs): process_send_requests()
  │  → Locks connections map
  │  → Calls conn.send_message(stream_id, data, reliability, now)
  ▼
Connection (connection.rs): send_message()
  │  1. Check connection is established
  │  2. Check stream exists and can_send()
  │  3. Check message size ≤ max_message_size
  │  4. Check connection flow control: flow.send.try_consume()
  │  5. Check stream flow control: stream.flow.send.try_consume()
  │  6. Check memory budget: memory_budget.try_reserve()
  │  7. Fragment payload: fragment_payload(payload, max_frag)
  │  8. Allocate message ID: stream.allocate_message_id()
  │  9. Create SendMessage tracker
  │ 10. Queue DATA frames with FIRST/FIN flags
  ▼
Engine: process_outgoing()
  │  1. Check ACK: conn.ack_tracker.build_ack_frame()
  │  2. Check flow control: conn.flow.should_send_max_data()
  │  3. Check stream flow: stream.should_send_max_stream_data()
  │  4. Drain pending frames: conn.drain_pending_frames()
  │  5. Encode frames: Frame::encode_all(&frames)
  │  6. If too large → split one frame per packet
  │  7. For each packet:
  │     a. Allocate packet number
  │     b. Build short header
  │     c. Encrypt: security.encrypt(pn, plaintext, header_bytes)
  │     d. Track sent packet: reliability.on_packet_sent()
  │     e. Send: send_udp(final_packet, addr)
  ▼
UDP Socket: send_to()
  ▼
IP → Network
```

### Complete Receive Path

```
Network → IP → UDP
  │
  ▼
Engine: process_incoming()
  │  → recv_from(buf) — non-blocking, up to 32 per cycle
  │  → Handle WouldBlock, Interrupted, ConnectionReset, ConnectionAborted
  ▼
Engine: handle_packet(data, src_addr)
  │  1. Decode header: PacketHeader::decode(data)
  │  2. If Long header (handshake):
  │     → Decode frames (plaintext)
  │     → Process ConnectionInit / ConnectionInitAck / Handshake
  │  3. If Short header (established):
  │     a. Find connection by src_addr
  │     b. Check connection is established
  │     c. Decrypt: security.decrypt(pn, ciphertext, header_bytes)
  │        → Tries current epoch, then previous epoch
  │        → Replay check via sliding window
  │     d. Record packet number: ack_tracker.on_packet_received()
  │     e. Decode frames: Frame::decode_all(&plaintext)
  │     f. Process frames: conn.process_frames(frames, now)
  ▼
Connection: process_frames()
  │  → Data frame → on_data_frame()
  │    → Check flow control
  │    → Add fragment to MessageReassembly
  │    → If complete → reassemble → return ReceivedData::Message
  │  → Ack frame → on_ack_frame()
  │    → Process ACK ranges
  │    → Update RTT estimator
  │    → Detect loss (gap-based)
  │    → Update congestion controller
  │  → StreamOpen → Create/open stream
  │  → StreamClose → Close stream (remote side)
  │  → StreamReset → Reset stream
  │  → MaxData → Update connection flow control send window
  │  → MaxStreamData → Update stream flow control send window
  │  → Ping → Queue Pong frame
  │  → ConnectionClose → Transition to Closed
  │  → KeyUpdate → Rotate keys
  │  → Cancel → Cancel message, free reassembly memory
  │  → PathChallenge → Queue PathResponse
  │  → PathResponse → Validate path (migration complete)
  ▼
Engine: deliver(Delivery::Message { ... })
  │  → Thread-safe: pushes to Arc<Mutex<Vec<Delivery>>>
  ▼
API: conn.receive()
  │  → Polls deliveries
  │  → Returns Value::Object with { streamId, messageId, data: [u8] }
  │  → Or null if no deliveries
  ▼
Application
```

---

## 23. Comparison with TCP, UDP, QUIC

| Feature | TCP | UDP | QUIC | ATP |
|---|---|---|---|---|
| **Abstraction** | Byte stream | Datagram | Stream | Message |
| **Reliability** | All or nothing | None | Per-stream | Per-message |
| **Ordering** | Strict total | None | Per-stream | Per-message (selective) |
| **Multiplexing** | No | No | Yes (streams) | Yes (streams) |
| **Head-of-line blocking** | Yes | No | Per-stream | Per-message |
| **Security** | External (TLS) | None | Built-in (TLS 1.3) | Built-in (X25519 + AEAD) |
| **Flow control** | Yes | No | Yes (connection + stream) | Yes (connection + stream) |
| **Congestion control** | Yes | No | Yes (CUBIC/BBR) | Yes (NewReno) |
| **Memory bounds** | Implementation-dependent | No | Implementation-dependent | Protocol-level budgets |
| **Fragmentation** | Transparent | IP-level | Transparent | Message-level (sparse) |
| **Connection migration** | No | No | Yes | Yes (PathChallenge/Response) |
| **Key rotation** | No (TLS renegotiation) | No | Yes | Yes (epoch-based) |
| **Replay protection** | N/A | No | Yes | Yes (64-bit sliding window) |
| **0-RTT** | No | N/A | Yes | Not yet (planned) |
| **Language integration** | None | None | None | AdeshLang native |

---

## 24. Examples

### Example Files

| File | Description |
|---|---|
| `examples/Libraries/atp/atp_examples.adesh` | 14 comprehensive demos |
| `examples/Libraries/atp/atp_server.adesh` | Practical echo server |
| `examples/Libraries/atp/atp_client.adesh` | Practical client with mixed traffic |
| `examples/Libraries/atp/atp_production.adesh` | Production-grade game server (9 library integrations) |

### Running Examples

```bash
# Comprehensive examples
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_examples.adesh

# Server
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_server.adesh

# Client
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_client.adesh

# Production example (integrates ATP + JSON + Crypto + Compression + Encoding + Collections + Env + Time + fs)
cargo run --bin adeshlang -- run examples/Libraries/atp/atp_production.adesh
```

### Quick Start

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

### Production Example

The production example (`atp_production.adesh`) demonstrates integration with
9 AdeshLang standard libraries:

- **ATP** — Transport (reliable/unreliable, streams, datagrams)
- **JSON** — Message serialization
- **Crypto** — SHA-256, HMAC-SHA256
- **Compression** — zstd, gzip, lz4, deflate (auto-compression)
- **Encoding** — Base64
- **Collections** — HashMap (player registry), Queue (event queue)
- **Env** — Configuration from environment variables
- **Time** — Timestamps, elapsed time measurement
- **fs** — State persistence (save/load game state to JSON file)

The production example implements a game server microservice with:
- Config from environment variables
- Player registry using `Collections.HashMap`
- Message pipeline: JSON → optional compress → Base64 → HMAC → JSON envelope
- Client with login, chat, position updates, game state requests, heartbeats
- Stream multiplexing with different priorities
- Message cancellation
- State persistence to disk
- Compression ratio demonstration (zstd: 6.9%, gzip: 10.6%, lz4: 17.3%, deflate: 10.5%)

---

## 25. Testing

### Test Coverage

The ATP implementation includes 39 tests across all source files:

| File | Tests | Coverage |
|---|---|---|
| `wire.rs` | 6 | Varint roundtrip, short/long header, data frame, ACK frame, multi-frame decode, truncated packet |
| `security.rs` | 3 | AEAD encrypt/decrypt + tampering detection, replay protection, key rotation, nonce uniqueness |
| `reliability.rs` | 5 | RTT estimator, ACK tracker, reliability tracker ACK, loss detection by gap, timeout detection |
| `memory.rs` | 3 | Buffer pool reuse, memory budget, backpressure |
| `flow.rs` | 2 | Flow window, connection flow control |
| `congestion.rs` | 4 | Slow start → congestion avoidance, loss halves window, timeout resets, can_send |
| `timer.rs` | 2 | Timer fires, cancel stream timers |
| `message.rs` | 5 | Fragment payload, reassembly, reassembly memory limit, send message acking, reliability mode flags |
| `stream.rs` | 3 | Stream lifecycle, message management, reset |
| `connection.rs` | 4 | State transitions, open stream, send + receive message, send datagram |

**Test results: 38 passed, 0 failed, 1 ignored** (the ignored test is for
X25519 DH symmetry on Windows due to a known `x25519-dalek` 2.0.1 issue;
the AEAD encryption/decryption is verified separately).

### Running Tests

```bash
cargo test --lib atp
```

### Build Status

```
cargo build    → 0 warnings, 0 errors
cargo test     → 38 passed, 0 failed, 1 ignored
```

---

## Appendix A: Constants

| Constant | Value | Location |
|---|---|---|
| `ATP_VERSION` | 1 | wire.rs |
| `MAX_PACKET_PAYLOAD` | 1450 bytes | wire.rs |
| `MAX_VARINT` | 0x3FFF_FFFF_FFFF_FFFF | wire.rs |
| `X25519_KEY_LEN` | 32 bytes | security.rs |
| `AEAD_KEY_LEN` | 32 bytes | security.rs |
| `AEAD_NONCE_LEN` | 12 bytes | security.rs |
| `AEAD_TAG_LEN` | 16 bytes | security.rs |
| `INITIAL_RTT_MS` | 100 ms | reliability.rs |
| `MIN_RTO_MS` | 25 ms | reliability.rs |
| `MAX_RTO_MS` | 60,000 ms | reliability.rs |
| `MAX_ACK_RANGES` | 64 | reliability.rs |
| `REORDER_THRESHOLD` | 3 | reliability.rs |
| `TIME_THRESHOLD` | 9/8 × RTT | reliability.rs |
| `max_streams` | 256 | connection.rs |
| `max_data` | 16 MB | connection.rs |
| `max_stream_data` | 2 MB | connection.rs |
| `max_message_size` | 64 MB | connection.rs |
| `idle_timeout` | 30 seconds | connection.rs |
| `max_packet_size` | 1450 bytes | connection.rs |
| `total_budget` | 16 MB | memory.rs |
| `max_stream_memory` | 2 MB | memory.rs |
| `max_reassembly_memory` | 4 MB | memory.rs |

## Appendix B: Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `x25519-dalek` | 2.0.1 | X25519 ephemeral key agreement |
| `chacha20poly1305` | — | ChaCha20-Poly1305 AEAD encryption |
| `hkdf` | — | HKDF-SHA256 key derivation |
| `sha2` | — | SHA-256 hash |
| `rand` | — | OS random number generation |
| `zeroize` | — | Secret key zeroization |
| `once_cell` | — | Lazy static initialization |

All dependencies are pure Rust with no C code, ensuring cross-platform
compilation without platform-specific build tools.

---

## Known Limitations

ATP/1 is a production-hardened transport implementation. The following areas have intentional trade-offs or remaining work:

| Area | Status |
|---|---|
| Endpoint identity | **Optional** Ed25519 proofs on handshake flights. Default mode (`require_peer_identity = false`, empty `trusted_peer_keys`) provides encryption only — any valid proof is accepted when present. Configure `trusted_peer_keys` + `require_peer_identity` for authenticated endpoints. |
| Handshake DoS protection | HMAC retry tokens + per-IP rate limiting (bounded map, TTL eviction). Tokens support secret rotation via `RetryTokenIssuer::rotate_secret()`. |
| Path migration | Full state machine (`Stable → Validating → Completed`). Candidate paths accept only `PATH_RESPONSE` until validated. Bad responses are ignored; timeout returns to `Stable`. |
| Stream priority scheduling | Two-level weighted DRR: priority classes (weights 4096/2048/1024/512) + per-stream round-robin within each class. |
| Key update overlap | Epoch rotation supported; extended overlap windows are minimal. |
| API async model | `receive()` and `accept()` are non-blocking polls returning `null` when idle. |
| Maximum packet size | Conservative 1200-byte UDP payload limit (no IP fragmentation). Receive buffers sized to `UDP_RECV_BUFFER_SIZE` (1328 bytes). |

### Security defaults

- **Encryption is always on** after handshake (X25519 + ChaCha20-Poly1305).
- **Endpoint identity is opt-in**: set `AtpConfig::local_identity`, `trusted_peer_keys`, and `require_peer_identity = true`.
- When `trusted_peer_keys` is non-empty, peers must present a proof from a trusted key.
- Identity proofs use domain-separated context: `ATP/1 identity proof || role || version || CIDs || keys || params || token || transcript_hash`.

### ATP/1 Protocol Specification (Summary)

**Packet headers:**
- **Long header** (handshake): `0xC0 | version(4) | dst_cid_len | dst_cid | src_cid_len | src_cid | pn(varint) | payload`
- **Short header** (application): `0x40 | dst_cid(8) | pn(varint) | AEAD_ciphertext`

**Security:**
- Handshake transcript hashed with SHA-256; Finished = HMAC-SHA256(finished_key, transcript_hash)
- AEAD AAD = exact cleartext header bytes (sender and receiver must match byte-for-byte)
- Separate handshake and application packet-number spaces with independent replay windows

**Connection routing:**
- Connections keyed by 8-byte wire connection IDs, not `SocketAddr`
- API requests target a specific `connection_id` — never broadcast to all connections

**Reliability:**
- ACK ranges: `largest_acked`, `ack_delay`, `(gap, range_len)*` with `decode(encode(set)) == set`
- Fragment-level retransmission via `RetransmittableRef { stream_id, message_id, fragment_num }`
- Congestion control uses actual `bytes_acked`, not `packet_count × MSS`
