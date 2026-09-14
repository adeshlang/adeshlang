# Real-Time IoT & Autonomous Fleet Telemetry Command Center (ATP Mini-Project)

A complete, production-grade real-time system built entirely on the **Adesh Transport Protocol (ATP)** in AdeshLang.

---

## 🎯 Architecture Overview

Autonomous connected vehicles stream high-frequency sensor readings, engine diagnostics, and GPS coordinates to the central Command Center over ATP.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                    Fleet Command Center (Server)                        │
│                   Listening on ATP Port 9150                            │
└─────────────────────────────────────────────────────────────────────────┘
        ▲                                 │
        │ [Unreliable Datagrams]          │ [Critical Priority Streams]
        │ • GPS Coordinates (30 Hz)       │ • Safety Interlock Commands
        │ • Speed, Battery, Temperature   │ • HMAC-SHA256 Authenticated
        │ • Zero Head-of-Line Blocking    │ • Reroute / Emergency Stop
        │                                 ▼
┌─────────────────────────────────────────────────────────────────────────┐
│              Connected Vehicle Edge Node (Client #101)                  │
│       • Local Sensor Ingestion & Throttling Controller                  │
│       • Path Migration (WiFi Depot ↔ 5G Cellular Tower)                │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## 🌟 Key ATP Features Demonstrated

1. **Selective Reliability**:
   - High-frequency GPS and temperature samples use **unreliable datagrams** (`conn.datagram()`). Drops are harmless because fresh readings arrive continuously.
   - Emergency brake orders and dispatch commands use **reliable ordered streams** (`conn.send(data, true, true)`).
2. **Deficit Round-Robin (DRR) Priority Scheduling**:
   - `Critical` streams preempt bulk diagnostic logs.
3. **Connection Migration (Path Roaming)**:
   - Vehicles roam from depot WiFi to 5G cellular using `conn.migrate()` with 8-byte `PATH_CHALLENGE` / `PATH_RESPONSE` cryptographic validation.
4. **Cryptographic Security**:
   - End-to-end encrypted with X25519 ephemeral Diffie-Hellman and ChaCha20-Poly1305 AEAD.
   - Command authorization signed via HMAC-SHA256.

---

## 📂 Project Structure

| File | Description |
|------|-------------|
| [`types.adesh`](./types.adesh) | Data structs (`VehicleTelemetry`, `FleetCommand`, `SafetyAlert`), JSON schemas, and HMAC signing helpers |
| [`server.adesh`](./server.adesh) | Fleet Command Operations Server with live anomaly detection, safety dispatch, and metrics |
| [`client.adesh`](./client.adesh) | Autonomous Vehicle edge simulator with live driving dynamics and command execution |
| [`run_demo.adesh`](./run_demo.adesh) | Single-command end-to-end live simulation with multi-vehicle concurrent ingestion |

---

## 🚀 Running the Mini-Project

### Run the Complete Simulation
```bash
cargo run --bin adeshlang -- run examples/Libraries/atp/mini_project/run_demo.adesh
```

### Run Server and Client Separately
In terminal 1:
```bash
cargo run --bin adeshlang -- run examples/Libraries/atp/mini_project/server.adesh
```

In terminal 2:
```bash
cargo run --bin adeshlang -- run examples/Libraries/atp/mini_project/client.adesh
```
