# AdeshLang WebSocket Library & Examples

Production-grade RFC 6455 compliant WebSocket implementation for AdeshLang supporting unencrypted `ws://` and encrypted `wss://` TLS connections, custom configuration, frame-level control, room-based broadcasting, keepalive heartbeats, and low-level transport integrations.

---

## Quick Start

Run any example script using the AdeshLang CLI runner:

```sh
# Example 01: Basic Client Connection & Echo Protocol
cargo run --bin adeshlang -- run examples/Libraries/websocket/01_basic_client.adesh

# Example 02: High-Performance Concurrent Echo Server
cargo run --bin adeshlang -- run examples/Libraries/websocket/02_basic_server.adesh

# Example 03: Secure WSS Client Connection over TLS
cargo run --bin adeshlang -- run examples/Libraries/websocket/03_secure_wss.adesh

# Example 04: Heartbeats, Backpressure & Protocol Edge Cases
cargo run --bin adeshlang -- run examples/Libraries/websocket/04_heartbeat_and_edge_cases.adesh

# Example 05: Real-Time Chat Room Broadcast Server
cargo run --bin adeshlang -- run examples/Libraries/websocket/05_room_broadcast_server.adesh

# Example 06: Production-Grade Secure WSS Server & Client
cargo run --bin adeshlang -- run examples/Libraries/websocket/06_wss_tls_server_and_client.adesh

# Example 07: Low-Level Net & TLS Custom Transport Integration
cargo run --bin adeshlang -- run examples/Libraries/websocket/07_lowlevel_net_tls_transport.adesh

# Example 08: Production-Grade Secure WSS Listening Server
cargo run --bin adeshlang -- run examples/Libraries/websocket/08_wss_listening_echo_server.adesh

# Example 09: End-to-End Secure WSS Interactive Messaging Demo
cargo run --bin adeshlang -- run examples/Libraries/websocket/09_wss_full_duplex_demo.adesh

# Example 10: Message Fragmentation State Machine
cargo run --bin adeshlang -- run examples/Libraries/websocket/10_fragmented_messages.adesh

# Example 11: Invalid Frames & Protocol Error Handlers
cargo run --bin adeshlang -- run examples/Libraries/websocket/11_invalid_frames.adesh

# Example 12: Subprotocol Negotiation
cargo run --bin adeshlang -- run examples/Libraries/websocket/12_subprotocols.adesh

# Example 13: Origin Security Validation Policy
cargo run --bin adeshlang -- run examples/Libraries/websocket/13_origin_validation.adesh

# Example 14: Concurrent Multi-Client Performance
cargo run --bin adeshlang -- run examples/Libraries/websocket/14_concurrent_clients.adesh

# Example 15: Outbound Backpressure & Buffer Configuration
cargo run --bin adeshlang -- run examples/Libraries/websocket/15_backpressure.adesh

# Example 16: Server Lifecycle & Graceful Shutdown
cargo run --bin adeshlang -- run examples/Libraries/websocket/16_graceful_shutdown.adesh

# Example 17: Heartbeat Keepalive Manager
cargo run --bin adeshlang -- run examples/Libraries/websocket/17_heartbeat.adesh

# Example 18: High-Throughput & Large Payload Streaming
cargo run --bin adeshlang -- run examples/Libraries/websocket/18_large_messages.adesh

# Example 19: Dual-Stack IPv4/IPv6 Addressing
cargo run --bin adeshlang -- run examples/Libraries/websocket/19_ipv6.adesh

# Example 20: Production TLS/WSS Enterprise Security
cargo run --bin adeshlang -- run examples/Libraries/websocket/20_proxy_wss.adesh

# Example 21: Real-Time Room & Broadcast System
cargo run --bin adeshlang -- run examples/Libraries/websocket/21_rooms.adesh

# Example 22: Permessage-Deflate Extension Architecture
cargo run --bin adeshlang -- run examples/Libraries/websocket/22_permessage_deflate.adesh
```

---

## Example Overview & Use Cases

### `01_basic_client.adesh` — Basic Client Connection
Demonstrates typed client configuration, establishing a `ws://` connection, sending text messages, receiving responses, and graceful close handshake.

**Use Case**: Lightweight client for real-time notifications, stock tickers, or API integration.

```adesh
import WebSocket;

let config = new WebSocket.ClientConfig();
config.connect_timeout = 5000;

let client = WebSocket.connect("ws://127.0.0.1:8099", config);
if (client instanceof WebSocket.WebSocketError) {
    print("Connection failed: " + client.toString());
    return;
}

client.sendText("Hello from AdeshLang!");
let msg = client.receive();
print("Received: " + msg.payload);
client.close(WebSocket.CloseCode.NormalClosure, "Client shutting down");
```

---

### `02_basic_server.adesh` — High-Performance Echo Server
Demonstrates starting a WebSocket server on a specified port, registering client connection handlers, receiving text/binary frames, and echoing data back.

**Use Case**: Standalone WebSocket server for duplex RPC or live echo services.

```adesh
import WebSocket;

let config = new WebSocket.ServerConfig();
config.bind_address = "127.0.0.1";
config.port = 8099;

let wsServer = WebSocket.server(config);

wsServer.onConnection(fn(conn) {
    print("New client connected!");
    while (conn.isOpen()) {
        let msg = conn.receive();
        if (msg.type == "text") {
            conn.sendText("Echo: " + msg.payload);
        } elif (msg.type == "close") {
            break;
        }
    }
});

wsServer.run();
```

---

### `03_secure_wss.adesh` — Encrypted TLS WSS Client
Demonstrates encrypted `wss://` client transport with TLS certificate and hostname SNI verification.

**Use Case**: Connecting to secure public or internal production WebSocket streams (`wss://`).

```adesh
import WebSocket;

let config = new WebSocket.ClientConfig();
config.verify_tls = true;

let client = WebSocket.connect("wss://echo.websocket.events", config);
client.sendText("Encrypted message over TLS");
let response = client.receive();
print("Received: " + response.payload);
```

---

### `04_heartbeat_and_edge_cases.adesh` — Control Frames & Edge Case Handling
Demonstrates ping/pong keepalives, payload size enforcement, control frame RFC validation (e.g. payload length limits <= 125 bytes), and error handling on closed connections.

**Use Case**: Robust client keepalive monitoring, preventing memory leaks, and shielding against malformed/oversized frames.

```adesh
import WebSocket;

let config = new WebSocket.ClientConfig();
config.max_message_size = 1048576; // 1 MiB limit
config.automatic_pong = true;

let client = WebSocket.connect("ws://127.0.0.1:8099", config);

// Send Heartbeat Ping
client.sendPing([1, 2, 3, 4]);

// Oversized Ping protection test (>125 bytes)
let err = client.sendPing(oversizedBytes);
if (WebSocket.isError(err)) {
    print("Edge Case Caught: " + err.toString());
}
```

---

### `05_room_broadcast_server.adesh` — Room-Based Multi-Client Broadcast
Demonstrates a multi-client room architecture where connected clients can publish messages to be broadcasted to all other active connections in the room.

**Use Case**: Real-time multiplayer games, collaborative editing tools, and live chat application backends.

```adesh
class ChatRoom {
    name: string;
    clients;

    ChatRoom(name: string) {
        self.name = name;
        self.clients = [];
    }

    fn broadcast(sender, message: string) {
        let i: int = 0;
        while (i < self.clients.length) {
            let client = self.clients[i];
            if (client != sender && client.isOpen()) {
                client.sendText("[" + self.name + "] " + message);
            }
            i = i + 1;
        }
    }
}
```

---

### `06_wss_tls_server_and_client.adesh` — Secure WSS TLS Server
Demonstrates binding a secure `wss://` server using local SSL/TLS certificate and key files (`cert.pem`, `key.pem`) overlaid on a `Net.tcpListen` socket via `TLS.bindServer`.

**Use Case**: Self-hosted production secure WebSocket servers with TLS 1.3 / 1.2 termination.

```adesh
import Net;
import TLS;
import WebSocket;
import IO;

let certPem = IO.readText("cert.pem");
let keyPem = IO.readText("key.pem");

let tcpListener = Net.tcpListen("127.0.0.1", 8443);
let tlsServer = TLS.bindServer(tcpListener, certPem, keyPem);
```

---

### `07_lowlevel_net_tls_transport.adesh` — Low-Level Transport Integration
Demonstrates manual TCP connection via `Net.tcpConnect`, wrapping with `TLS.wrap`, and manually executing the `performClientHandshake` on a raw `WebSocketConnection` state machine.

**Use Case**: Embedded network stacks, custom proxies, and protocol bridges requiring custom TCP/TLS socket control before HTTP Upgrade.

```adesh
import Net;
import TLS;
import WebSocket;
import URL;

let urlObj = URL.parse("wss://echo.websocket.events:443/chat");
let tcpSocket = Net.tcpConnect(urlObj.host, urlObj.port);
let tlsSocket = TLS.wrap(tcpSocket, urlObj.host);

let conn = new WebSocket.WebSocketConnection(tlsSocket, true, new WebSocket.ClientConfig());
conn.performClientHandshake(urlObj, "dGhlIHNhbXBsZSBub25jZQ==");
```

---

## API Reference

### Top-Level Module Functions

```adesh
// Connect to a WebSocket URI (ws:// or wss://)
WebSocket.connect(uri: string, config: ClientConfig): WebSocketConnection | WebSocketError

// Create a WebSocketServer instance with typed configuration
WebSocket.server(config: ServerConfig): WebSocketServer
```

---

### `WebSocket.ClientConfig`

Configurable properties for client connections:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `uri` | `string` | `null` | Target WebSocket URI |
| `headers` | `Map<string, string>` | `{}` | Custom HTTP handshake request headers |
| `protocols` | `Array<string>` | `[]` | Subprotocol negotiation list (e.g. `["json", "chat"]`) |
| `origin` | `string` | `null` | HTTP `Origin` header value |
| `connect_timeout` | `int` | `10000` | Connection timeout in milliseconds |
| `max_message_size` | `int` | `16777216` | Max accumulated message payload size (16 MiB) |
| `max_frame_size` | `int` | `4194304` | Max individual frame payload size (4 MiB) |
| `automatic_pong` | `bool` | `true` | Automatically reply to Ping frames with Pong |
| `verify_tls` | `bool` | `true` | Enforce TLS certificate verification on `wss://` |

---

### `WebSocket.ServerConfig`

Configurable properties for server instances:

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `bind_address` | `string` | `"127.0.0.1"` | IP interface address to bind |
| `port` | `int` | `8099` | Network port to listen on |
| `max_connections` | `int` | `10000` | Max concurrent active client connections |
| `protocols` | `Array<string>` | `[]` | Supported subprotocols list |
| `origins` | `Array<string>` | `[]` | Allowed origins list |

---

### `WebSocketConnection` Class

Represents an active WebSocket connection state machine.

```adesh
// Methods
conn.isOpen(): bool                         // Check if state == State.OPEN
conn.getState(): StateKind                  // Get current state enum (OPEN, CLOSING, CLOSED, etc.)

conn.sendText(text: string)                 // Send text frame (UTF-8 encoded)
conn.sendBinary(bytes: Array<int>)          // Send binary payload frame
conn.sendPing(payload: Array<int>)          // Send Ping control frame (payload <= 125 bytes)
conn.sendPong(payload: Array<int>)          // Send Pong control frame (payload <= 125 bytes)

conn.receive(): WebSocketMessage            // Receive next frame/message from remote peer
conn.close(code: int, reason: string)       // Perform RFC 6455 close handshake and close socket
```

---

### `WebSocketMessage` Structure

Returned by `conn.receive()`:

| Property | Type | Description |
|----------|------|-------------|
| `type` | `string` | Message type: `"text"`, `"binary"`, `"ping"`, `"pong"`, or `"close"` |
| `payload` | `string` \| `Array<int>` | Decoded message content (`string` for text, `Array<int>` for binary/ping/pong) |
| `closeCode` | `int` | Close status code (available when `type == "close"`) |
| `closeReason` | `string` | Disconnect reason text (available when `type == "close"`) |

---

### Constants & Enumerations

#### `Opcode`
- `Opcode.Continuation` (`0x0`)
- `Opcode.Text` (`0x1`)
- `Opcode.Binary` (`0x2`)
- `Opcode.Close` (`0x8`)
- `Opcode.Ping` (`0x9`)
- `Opcode.Pong` (`0xA`)

#### `State`
- `State.CONNECTING` (`0`)
- `State.HANDSHAKING` (`1`)
- `State.OPEN` (`2`)
- `State.CLOSING` (`3`)
- `State.CLOSED` (`4`)
- `State.FAILED` (`5`)

#### `CloseCode`
- `CloseCode.NormalClosure` (`1000`)
- `CloseCode.GoingAway` (`1001`)
- `CloseCode.ProtocolError` (`1002`)
- `CloseCode.UnsupportedData` (`1003`)
- `CloseCode.InvalidPayloadData` (`1007`)
- `CloseCode.PolicyViolation` (`1008`)
- `CloseCode.MessageTooBig` (`1009`)
- `CloseCode.InternalError` (`1011`)

---

## Implementation Classification Matrix

| Feature / Example | Classification | Notes |
|-------------------|----------------|-------|
| 01 Basic Client | `FULLY IMPLEMENTED` | `ws://` client handshake, framing, send/receive |
| 02 Basic Server | `FULLY IMPLEMENTED` | `ws://` server binding, Upgrade handshake |
| 03 WSS Client | `FULLY IMPLEMENTED` | TLS 1.3 client with certificate verification |
| 04 Heartbeat & Edge Cases | `FULLY IMPLEMENTED` | Automatic Pong, 125-byte control frame cap |
| 05 Room Broadcast Server | `FULLY IMPLEMENTED` | Process-wide room tracking and broadcasting |
| 06 WSS Server | `FULLY IMPLEMENTED` | TLS WSS server listener using PEM certs |
| 07 Low-Level Transport | `FULLY IMPLEMENTED` | Raw socket wrapping via `WebSocketConnection` |
| 08 WSS Listening Server | `FULLY IMPLEMENTED` | Interactive WSS listening server |
| 09 WSS Full Duplex Demo | `FULLY IMPLEMENTED` | End-to-end full duplex messaging with DTO schemas |
| 10 Fragmented Messages | `FULLY IMPLEMENTED` | RFC 6455 FIN=0/1 message reassembly across fragments |
| 11 Invalid Frames | `FULLY IMPLEMENTED` | RFC limits enforcement & close code wire validation |
| 12 Subprotocols | `FULLY IMPLEMENTED` | `Sec-WebSocket-Protocol` client/server negotiation |
| 13 Origin Validation | `FULLY IMPLEMENTED` | CORS/Origin whitelist validation policy |
| 14 Concurrent Clients | `FULLY IMPLEMENTED` | Bounded worker thread pool queue handling 10,000+ simultaneous connections safely |
| 15 Backpressure | `FULLY IMPLEMENTED` | Bounded queue size limits & max payload enforcement |
| 16 Graceful Shutdown | `FULLY IMPLEMENTED` | Dispatches 1001 Going Away close frame on server.close() |
| 17 Heartbeat Manager | `FULLY IMPLEMENTED` | Automated background `Ping` interval timer loop with `Pong` timeout disconnect policy & telemetry |
| 18 Large Messages | `FULLY IMPLEMENTED` | 64-bit frame payload length streaming |
| 19 IPv4/IPv6 | `FULLY IMPLEMENTED` | Dual-stack IP socket address resolution & IPv6 bracket parsing |
| 20 Proxy/WSS | `FULLY IMPLEMENTED` | HTTP `CONNECT` tunneling sequence for `ws://` and `wss://` |
| 21 Rooms & Broadcast | `FULLY IMPLEMENTED` | Native `joinRoom`, `leaveRoom`, `broadcastRoom`, `broadcastRoomExcept` |
| 22 Permessage-Deflate | `FULLY IMPLEMENTED` | Production DEFLATE stream compression/decompression engine (`flate2`) with `RSV1` & bomb bounds |

