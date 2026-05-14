# Functional Specification: BACnet Bridge

**Version:** 0.1.0 — Draft
**Date:** 2026-05-14
**Status:** In Progress

## Table of Contents
1. [Overview](#1-overview)
2. [Architecture](#2-architecture)
3. [Crate Layout](#3-crate-layout)
4. [Transport Layer](#4-transport-layer)
5. [Routing Engine](#5-routing-engine)
6. [Application Modes](#6-application-modes)
7. [Web Dashboard](#7-web-dashboard)
8. [System Tray](#8-system-tray)
9. [Configuration](#9-configuration)
10. [Logging](#10-logging)
11. [Testing Strategy](#11-testing-strategy)
12. [Build & Packaging](#12-build--packaging)
13. [Deployment Topologies](#13-deployment-topologies)
14. [Dependencies](#14-dependencies)

---

## 1. Overview

BACnet Bridge is a dual-transport BACnet router that bridges a local BACnet/IP LAN to a remote BACnet network. It enables Innotech iComm (BACnet/IP only) running on a service laptop to discover and interact with on-site BACnet devices, even when the laptop is at a remote location.

### 1.1 Purpose

The primary use case is **remote field service**: a technician at any location runs the Laptop Router alongside iComm, connects via BACnet/SC to a cloud Hub, and discovers devices on a remote site's LAN as if locally connected.

A secondary use case is **permanent site-to-site bridging**: the Site Router runs continuously at a facility, maintaining a persistent BACnet/SC connection so remote monitoring tools can access the site's devices at any time.

### 1.2 Key Features

- **BACnet/SC primary transport** — WebSocket+TLS hub-and-spoke, NAT-friendly, no VPN required
- **Tailscale fallback transport** — BACnet/IP BBMD + Foreign Device over Tailscale VPN
- **Manual transport switching** — operator chooses active transport; no auto-failover (avoids flapping)
- **Standard BACnet routing** — Clause 6 network-layer routing between networks, not monkey-patched broadcast forwarding
- **Local web dashboard** — configuration, status, FDT table, live logs
- **Windows system tray** — colored status icon (green/amber/red), right-click menu
- **Single binary** — `bacnet-bridge.exe` with subcommands: `router`, `hub`, `serve`
- **Docker e2e testing** — full topology testable on a dev machine with no physical BACnet hardware

### 1.3 Topology

```
                         BACnet/IP            BACnet/SC            BACnet/SC            BACnet/IP
iComm ──────────────────► Laptop Router ────► Hub ────► Site Router ────► LAN devices
(Foreign Device or UDP)  (Spoke)          (Cloud)    (Spoke)           (network 1)
```

Fallback mode (Tailscale):

```
                         BACnet/IP over Tailscale VPN             BACnet/IP
iComm ──────────────────► Site Router (BBMD) ────► LAN devices
(Foreign Device)                                     (same subnet)
```

---

## 2. Architecture

### 2.1 High-Level Component Diagram

```
┌─────────────────────────────────────────────────────────────────────┐
│                          bacnet-bridge.exe                           │
│                                                                      │
│  ┌──────────────┐  ┌──────────────────┐  ┌───────────────────────┐  │
│  │  Web Server  │  │  System Tray     │  │   Routing Engine      │  │
│  │  (axum)      │  │  (tray-item)     │  │   (bridge-core)       │  │
│  │              │  │                  │  │                       │  │
│  │  - Config    │  │  - Status icon   │  │  ┌─────────────────┐  │  │
│  │  - Status    │  │  - Menu items    │  │  │  BACnetRouter   │  │  │
│  │  - FDT table │  │  - State sync    │  │  │  (bacnet-       │  │  │
│  │  - Live logs │  │                  │  │  │   network)      │  │  │
│  └──────┬───────┘  └────────┬─────────┘  │  └────────┬────────┘  │  │
│         │                   │             │           │           │  │
│         └─────────┬─────────┘             │  ┌────────┴────────┐  │  │
│                   │                       │  │  Transport      │  │  │
│            ┌──────┴──────┐                │  │  Adapter        │  │  │
│            │  App State  │◄───────────────┤  │  (SC or BBMD)   │  │  │
│            │  (shared)   │                │  └─────────────────┘  │  │
│            └──────┬──────┘                │                       │  │
│                   │                       └───────────────────────┘  │
│            ┌──────┴──────┐                                           │
│            │  Config      │                                           │
│            │  (TOML file) │                                           │
│            └──────────────┘                                           │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Data Flow

```
[Web UI / Tray] ──(commands)──► [AppState] ──► [Routing Engine]
                                                 │
                    ┌────────────────────────────┘
                    ▼
            ┌───────────────┐
            │  RouterPort 1 │──► [LAN BIP Transport] ──► [Local BACnet devices]
            │  (network 1)  │
            └───────────────┘
            ┌───────────────┐
            │  RouterPort 2 │──► [SC Transport] ──► [Hub] ──► [Remote Spokes]
            │  (network 2)  │      OR
            └───────────────┘      [Tailscale BBMD Transport] ──► [Foreign Devices]

Broadcast forwarding: Who-Is from network 2 → Router → network 1 broadcast
Unicast responses: I-Am from network 1 → Router → network 2 (via routing table)
```

---

## 3. Crate Layout

### 3.1 Workspace Structure

```
bacnet-bridge/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── CONTEXT.md                    # Domain language
├── docs/
│   ├── FSD.md                    # This document
│   └── adr/
│       ├── 0001-pure-rust-rewrite.md
│       └── 0002-dual-transport.md
├── crates/
│   └── bridge-core/              # Shared BACnet routing engine
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs            # Re-exports
│           ├── router.rs         # Router lifecycle: start/stop, port management
│           ├── transport.rs      # Transport enum (SC, BBMD), factory
│           ├── sc_transport.rs   # BACnet/SC spoke wrapper
│           ├── bbmd_transport.rs # Tailscale BBMD + BIPSimple wrapper
│           ├── fdt.rs            # Foreign Device Table management
│           ├── state.rs          # Shared application state (tokio broadcast)
│           ├── config.rs         # TOML config loading/saving
│           └── error.rs          # Error types
├── src/                          # Main binary (bridge-app)
│   ├── main.rs                   # CLI entry point, subcommand dispatch
│   ├── cli.rs                    # clap CLI definition
│   ├── router_cmd.rs             # `router` subcommand: full app with UI+tray
│   ├── hub_cmd.rs                # `hub` subcommand: SC hub only
│   ├── serve_cmd.rs              # `serve` subcommand: web UI only (testing)
│   ├── web/                      # Web server module
│   │   ├── mod.rs
│   │   ├── routes.rs             # Axum routes
│   │   ├── ws.rs                 # WebSocket log streaming
│   │   └── api.rs                # REST API handlers
│   ├── tray.rs                   # System tray integration
│   └── assets/                   # Embedded web assets
│       ├── index.html
│       ├── style.css
│       ├── app.js
│       └── favicon.ico
├── tests/                        # Integration tests (non-Docker)
│   ├── router_tests.rs           # Router forwarding tests
│   ├── fdt_tests.rs              # FDT management tests
│   └── config_tests.rs           # Config round-trip tests
├── docker/                       # Docker e2e test infrastructure
│   ├── docker-compose.yml
│   ├── docker-compose.sc.yml     # BACnet/SC topology
│   ├── docker-compose.bbmd.yml   # Tailscale BBMD topology
│   ├── Dockerfile.router         # Router container
│   ├── Dockerfile.hub            # Hub container
│   ├── Dockerfile.simulator      # iComm simulator container
│   ├── Dockerfile.device         # BACnet device simulator container
│   └── test-runner/              # E2E test harness
│       ├── Cargo.toml
│       └── src/
│           └── main.rs           # Who-Is/I-Am/RP round-trip tests
└── bacnet_bbmd_tool_tailscale/   # Legacy Python prototype (reference only)
```

### 3.2 Crate Dependencies

```
bridge-app ──► bridge-core ──► bacnet-network (rusty-bacnet)
                             ├── bacnet-transport (rusty-bacnet)
                             ├── bacnet-types (rusty-bacnet)
                             ├── bacnet-encoding (rusty-bacnet)
                             ├── tokio
                             ├── rustls
                             ├── toml + serde
                             └── tracing

bridge-app ──► axum
             ├── tower-http
             ├── rust-embed
             ├── tray-item
             ├── tokio-rustls-acme (hub mode only)
             └── clap
```

---

## 4. Transport Layer

### 4.1 Transport Abstraction

The routing engine operates on a transport-agnostic interface. rusty-bacnet's `TransportPort` trait (`bacnet-transport::port`) provides this:

```rust
pub trait TransportPort: Send + Sync {
    async fn start(&mut self) -> Result<mpsc::Receiver<ReceivedNpdu>, Error>;
    async fn stop(&mut self) -> Result<(), Error>;
    async fn send_unicast(&self, npdu: &[u8], mac: &[u8]) -> Result<(), Error>;
    async fn send_broadcast(&self, npdu: &[u8]) -> Result<(), Error>;
    fn local_mac(&self) -> &[u8];
}
```

Our bridge-core wraps this in a `Transport` enum:

```rust
pub enum Transport {
    /// BACnet/SC via cloud hub (primary)
    Sc {
        hub_url: String,
        tls_config: Arc<ClientConfig>,
        local_vmac: Vmac,
        transport: AnyTransport<TlsWebSocket>,
    },
    /// Tailscale BBMD + Foreign Device (fallback)
    Tailscale {
        bbmd_ip: Ipv4Addr,
        bbmd_port: u16,
        lan_ip: Ipv4Addr,
        lan_port: u16,
        transport: AnyTransport<BipTransport>,
        bbmd_state: Arc<Mutex<BbmdState>>,
    },
}
```

### 4.2 BACnet/SC Transport (Primary)

**Source crate:** `bacnet-transport::sc::ScTransport<TlsWebSocket>`

**Spoke (Laptop Router & Site Router):**
1. Create `TlsWebSocket::connect(hub_url, tls_config).await`
2. Create `ScTransport::new(ws, local_vmac)`
3. Optionally configure: `.with_device_uuid(uuid)`, `.with_reconnect(config)`, `.with_failover(backup_ws)`
4. Wrap in `AnyTransport`
5. Pass to `BACnetRouter::start(vec![lan_port, sc_port])`

**TLS Configuration:**
```rust
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
// Client: standard root cert store (validates hub's Let's Encrypt cert)
let tls_config = build_client_tls_config(None, None, None)?;
// Optional mTLS: provide client cert + key
let tls_config = build_client_tls_config(None, Some(client_cert_path), Some(client_key_path))?;
```

**Reconnection behavior:**
- `ScReconnectConfig` supports exponential backoff
- Our router exposes this as config parameters: `reconnect_initial_ms`, `reconnect_max_ms`, `reconnect_max_attempts`

### 4.3 Tailscale BBMD Transport (Fallback)

**Source crate:** `bacnet-transport::bbmd::BbmdState`

**BBMD setup (Site Router):**
1. Create `BipTransport::new(tailscale_ip, bbmd_port, broadcast_addr)`
2. Call `transport.enable_bbmd(bdt_entries)`
3. Foreign devices on the Tailscale network send `RegisterForeignDevice` to this BBMD
4. FDT is managed by `BbmdState` internally

**Foreign Device setup (Laptop Router or iComm directly):**
- iComm handles Foreign Device registration natively — passes through transparently
- If laptop has a Router, the Laptop Router uses `BipTransport::register_as_foreign_device(config)`

**FDT Management:**
- Use `BbmdState::fdt()` to read current foreign devices (returns `&[FdtEntry]`)
- `FdtEntry` fields: `ip: [u8; 4]`, `port: u16`, `ttl: u16`, `registered_at: Instant`
- FDT is exposed to the web dashboard via REST API
- `BbmdState::purge_expired()` called on a timer (every 10 seconds)
- Dashboard polls FDT every 2 seconds

### 4.4 Manual Transport Switching

```rust
pub async fn switch_transport(
    router: &mut AppRouter,
    new_transport: Transport,
) -> Result<(), BridgeError> {
    // 1. Stop current router (drains pending messages)
    router.stop().await?;
    // 2. Build new transport stack
    let new_router = build_router(new_transport).await?;
    // 3. Start new router
    new_router.start().await?;
    // 4. Update app state
    *router = new_router;
    Ok(())
}
```

The switch is triggered by:
- Web dashboard: "Switch Transport" button in config pane
- System tray: "Switch to Tailscale" / "Switch to BACnet/SC" menu item
- CLI: `bacnet-bridge router --transport tailscale` or `--transport sc`
- Not automatic — no health-check-based failover

---

## 5. Routing Engine

### 5.1 BACnetRouter Integration

The core routing is delegated to rusty-bacnet's `bacnet_network::router::BACnetRouter`. Our `bridge-core` wraps it with lifecycle management and transport port construction.

**RouterPorts:**
```
Port 0: LAN BIP Transport (network 1) — talks to local BACnet devices
Port 1: Remote Transport (network 2) — SC spoke or Tailscale BBMD
```

**Startup sequence:**
```rust
pub async fn start_router(config: &RouterConfig) -> Result<RunningRouter, BridgeError> {
    // 1. Build LAN transport (always BIP, always network 1)
    let lan_transport = BipTransport::new(
        config.lan.ip,
        config.lan.port,  // default 47808
        config.lan.broadcast_addr,
    );

    // 2. Build remote transport (network 2) — SC or Tailscale
    let remote_transport = match &config.remote {
        RemoteConfig::Sc { hub_url, local_vmac, .. } => {
            build_sc_transport(hub_url, local_vmac, &config.tls).await?
        }
        RemoteConfig::Tailscale { bbmd_ip, bbmd_port, .. } => {
            build_bbmd_transport(bbmd_ip, bbmd_port).await?
        }
    };

    // 3. Start BACnetRouter with both ports
    let (mut router, rx) = BACnetRouter::start(vec![
        RouterPort { transport: lan_transport, network_number: 1 },
        RouterPort { transport: remote_transport, network_number: 2 },
    ]).await?;

    // 4. Spawn APDU handler (for forwarding received APDUs)
    let router_table = router.table().clone();
    tokio::spawn(handle_received_apdus(rx, router_table));

    Ok(RunningRouter { router, /* ... */ })
}
```

### 5.2 Broadcast Forwarding

The `BACnetRouter` handles broadcast forwarding automatically:
- When a Who-Is broadcast arrives on Port 2 (network 2), `BACnetRouter` broadcasts it on Port 1 (network 1)
- When an I-Am response arrives on Port 1, the router learns the source device's reachability and stores it in the `RouterTable`
- Subsequent unicast requests are routed directly (not broadcast)

### 5.3 Router Table

The `RouterTable` maintains learned routes:
- `add_direct(network, port_index)` — directly connected networks (1 and 2)
- `add_learned(network, port_index, next_hop_mac)` — learned from forwarded responses
- `lookup(network)` — find route for a destination network
- `purge_stale(max_age)` — remove entries older than threshold

### 5.4 Foreign Device Table (Tailscale Mode Only)

When operating in Tailscale mode, the BBMD maintains an FDT. This is exposed via:

```rust
impl FdtManager {
    /// Returns current FDT entries with remaining TTL
    pub fn list(&self) -> Vec<FdtDisplayEntry> { ... }
    /// Called by timer every 2 seconds to update remaining TTLs
    pub fn tick(&mut self) { ... }
}
```

---

## 6. Application Modes

The single binary supports three subcommands:

### 6.1 `bacnet-bridge router`

Full application with web UI + system tray + routing engine.

```
bacnet-bridge router [OPTIONS]
  --config <PATH>       Path to config file [default: %APPDATA%\bacnet-bridge\config.toml]
  --transport <MODE>    Override transport: sc | tailscale
  --log-level <LEVEL>   Log level [default: info]
```

**Lifecycle:**
1. Load config from TOML file
2. If running as Windows GUI app, initialize system tray
3. Start axum web server on configured port
4. Build and start routing engine
5. Open browser to dashboard URL (configurable)
6. Block until exit signal (tray "Exit" or SIGTERM)

### 6.2 `bacnet-bridge hub`

Dedicated BACnet/SC hub binary for cloud deployment.

```
bacnet-bridge hub [OPTIONS]
  --bind <ADDR>          Bind address [default: 0.0.0.0:443]
  --cert <PATH>          TLS certificate PEM (if not using ACME)
  --key <PATH>           TLS private key PEM (if not using ACME)
  --acme-domain <DOMAIN> Enable Let's Encrypt ACME for this domain
  --acme-cache <PATH>    ACME cache directory [default: ./acme-cache]
```

**TLS modes:**
- **Static certs:** Provide `--cert` and `--key` paths
- **Let's Encrypt:** Provide `--acme-domain`; uses `tokio-rustls-acme` for auto-provisioning and renewal
- **Self-signed (dev):** Neither provided; generates `rcgen` cert for testing

**Hub configuration:**
- Max 256 connected spokes (rusty-bacnet default)
- 30-second heartbeat interval, 60-second client idle timeout
- 512 concurrent pre-handshake connections

### 6.3 `bacnet-bridge serve`

Web UI only (no routing engine, no tray). Useful for testing the dashboard during development.

```
bacnet-bridge serve [OPTIONS]
  --port <PORT>     Web server port [default: 28821]
  --host <HOST>     Bind address [default: 127.0.0.1]
```

---

## 7. Web Dashboard

### 7.1 Technology Stack

| Layer | Technology |
|-------|-----------|
| Server | axum 0.8 + tokio |
| Static files | rust-embed (embedded at compile time) |
| Frontend | HTMX 2.0 + vanilla JS |
| Styling | Tailwind CSS (pinned build, embedded) |
| Live updates | WebSocket (axum ws) + HTMX polling |
| Icons | Lucide icons (inline SVG, no icon font) |

### 7.2 Pages / Sections

**Dashboard URL:** `http://localhost:28821` (configurable)

#### 7.2.1 Status Bar
- **Transport indicator:** Green circle = BACnet/SC connected, Amber circle = Tailscale active, Red circle = disconnected
- **Text label:** "BACnet/SC connected to hub.example.com" or "Tailscale BBMD on 100.92.39.75:20000"
- **Uptime counter:** "Running for 2h 13m"

#### 7.2.2 Network Configuration Panel
- **LAN Interface dropdown:** Lists all non-Tailscale IPs from system; populates on page load via REST API `GET /api/interfaces`
- **LAN Port input:** Numeric, default 47808
- **Remote Transport selector:** Radio buttons or toggle: "BACnet/SC Hub" | "Tailscale BBMD"
- **SC Hub URL input:** Only shown when SC is selected; default `wss://hub.example.com:443`
- **Tailscale Interface dropdown:** Only shown when Tailscale is selected; lists IPs starting with `100.`
- **Tailscale BBMD Port input:** Only shown when Tailscale is selected; default 20000
- **Apply button:** Saves config and triggers transport switch via `POST /api/transport/switch`
- **Config changes persist to TOML file automatically**

#### 7.2.3 Foreign Device Table (Tailscale Mode Only)
- **Table columns:** Device Address (IP:Port), TTL (seconds), Remaining (seconds)
- **Auto-refresh:** HTMX polling every 2 seconds via `GET /api/fdt`
- **Empty state:** "No foreign devices registered" when FDT is empty
- **Hidden when:** BACnet/SC mode is active (FDT only exists in BBMD mode)

#### 7.2.4 Live Log
- **Log viewer:** Scrollable monospace area (last 500 lines, ring buffer)
- **Auto-scroll:** Pinned to bottom when new log entries arrive
- **Live streaming:** WebSocket endpoint `/ws/logs` pushes new entries in real-time
- **Filter controls:**
  - Severity dropdown: All | Info+ | Warn+ | Error+
  - Search text input: Filter by message content (client-side)
- **Export:** "Download Logs" button → `GET /api/logs?format=text` (last 10,000 lines)

#### 7.2.5 Router Info
- **BACnet Device ID:** Configurable (default 999)
- **Vendor ID:** Configurable (default 15 = Innotech)
- **Connected networks table:**
  - Network 1: LAN (IP, port, adapter name)
  - Network 2: Remote (transport type, hub URL or BBMD address)

### 7.3 REST API Endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/status` | Current router status (transport, uptime, networks) |
| GET | `/api/interfaces` | List detected network interfaces |
| GET | `/api/config` | Current configuration |
| PUT | `/api/config` | Update configuration (writes TOML) |
| POST | `/api/transport/switch` | Switch transport mode (body: `{"mode": "sc" \| "tailscale"}`) |
| POST | `/api/transport/stop` | Stop routing engine |
| POST | `/api/transport/start` | Start routing engine |
| GET | `/api/fdt` | Foreign Device Table (empty if SC mode) |
| GET | `/api/logs` | Recent log entries (query: `?limit=500&level=info`) |
| WS | `/ws/logs` | Live log streaming |

### 7.4 WebSocket Log Protocol

```
Server → Client messages (JSON):
{
  "timestamp": "14:32:05.123",
  "level": "INFO",
  "target": "bacnet_bridge::router",
  "message": "Foreign device registered: 100.65.12.34:20000 (TTL: 300)",
  "fields": { "fd_addr": "100.65.12.34:20000", "ttl": 300 }
}
```

---

## 8. System Tray

### 8.1 Technology

**Crate:** `tray-item` v0.10+ (MIT, active maintenance, Windows support)

**Icon assets:** Three `.ico` files embedded via Windows `.rc` resource script:
- `green.ico` — Router running, BACnet/SC connected
- `amber.ico` — Router running, Tailscale fallback active
- `red.ico` — Router stopped or disconnected

### 8.2 Menu Structure

```
BACnet Bridge
├── Open Dashboard          → Opens http://localhost:28821 in default browser
├── ─────────────────       (separator)
├── Stop Router             → Stops routing engine; disabled when already stopped
├── Start Router            → Starts routing engine; disabled when already running
├── ─────────────────       (separator)
├── Switch to BACnet/SC     → Triggers transport switch; only shown in Tailscale mode
├── Switch to Tailscale     → Triggers transport switch; only shown in SC mode
├── ─────────────────       (separator)
├── Exit                    → Stops router, closes web server, exits process
```

### 8.3 State Synchronization

The tray icon color updates in real-time based on router state:
- **Green:** BACnet/SC connected to hub + LAN active
- **Amber:** Tailscale BBMD active + LAN active
- **Red:** Router stopped or both transports failed

State is pushed from the routing engine to the tray thread via `tokio::sync::watch`:
```rust
let (tray_tx, tray_rx) = tokio::sync::watch::channel(TrayState::Stopped);
// Router loop sends: tray_tx.send(TrayState::Running(TransportMode::Sc))?;
// Tray thread receives and updates icon
```

### 8.4 Start/Stop from Tray

Starting from the tray uses the same code path as starting from the web dashboard:
1. Read current config from TOML file
2. Build transport based on `transport` field
3. Start `BACnetRouter`
4. Update state broadcast to all watchers (tray icon, web UI status, log stream)

---

## 9. Configuration

### 9.1 File Format: TOML

**Location:**
- Windows: `%APPDATA%\bacnet-bridge\config.toml`
- Linux: `~/.config/bacnet-bridge/config.toml`
- Override: `--config <PATH>` CLI flag or `BACNET_BRIDGE_CONFIG` env var

### 9.2 Schema

```toml
# BACnet Bridge Configuration

[router]
# Currently active transport: "sc" or "tailscale"
transport = "sc"

# BACnet device identity
device_id = 999
vendor_id = 15
device_name = "BACnet-Bridge"

[router.lan]
# Local LAN interface for BACnet/IP communication with on-site devices
interface = "192.168.1.100"  # IP address to bind
port = 47808                  # BACnet/IP standard port

[router.sc]
# BACnet/SC hub connection (used when transport = "sc")
hub_url = "wss://hub.example.com:443"
# Optional: client certificate for mTLS (PEM file paths)
# client_cert = "certs/client.pem"
# client_key = "certs/client-key.pem"
# Reconnection backoff
reconnect_initial_ms = 1000
reconnect_max_ms = 30000
reconnect_max_attempts = 0  # 0 = unlimited

[router.tailscale]
# Tailscale BBMD configuration (used when transport = "tailscale")
interface = "100.92.39.75"   # Tailscale IP address
port = 20000                  # BBMD listener port
# Optional: BDT entries for peer BBMDs
# [[router.tailscale.bdt]]
# ip = "100.92.39.76"
# port = 20000
# broadcast_mask = [255, 255, 255, 255]

[web]
# Dashboard configuration
host = "0.0.0.0"             # Bind address (0.0.0.0 = accessible via Tailscale)
port = 28821
open_browser = true           # Auto-open dashboard on startup

[hub]
# Hub mode configuration (only used by `bacnet-bridge hub` subcommand)
bind = "0.0.0.0:443"
# TLS: use either static certs or ACME (Let's Encrypt)
# cert = "certs/hub.pem"
# key = "certs/hub-key.pem"
acme_domain = "hub.example.com"
acme_cache = "./acme-cache"
```

### 9.3 Environment Variable Overrides

All config keys can be overridden via environment variables with `BACNET_BRIDGE_` prefix:

```powershell
$env:BACNET_BRIDGE_ROUTER__TRANSPORT = "tailscale"
$env:BACNET_BRIDGE_ROUTER__LAN__INTERFACE = "10.0.0.5"
$env:BACNET_BRIDGE_ROUTER__SC__HUB_URL = "wss://staging-hub.example.com:443"
```

Nested keys use double underscore (`__`) separator.

---

## 10. Logging

### 10.1 Framework

**Crate:** `tracing` + `tracing-subscriber`

### 10.2 Log Destinations

1. **Stdout** (when running with console): Formatted with timestamps, levels, targets
2. **Ring buffer** (always): Last 10,000 entries in memory, exposed via REST API
3. **WebSocket** (when web server is running): Live push to dashboard log viewer
4. **File** (optional): Configurable log file path with rotation via `tracing-appender`

### 10.3 Log Levels

| Level | Usage |
|-------|-------|
| ERROR | Transport failures, socket bind errors, protocol violations |
| WARN | Transport switch, foreign device expiry, reconnection attempts |
| INFO | Router start/stop, foreign device registration, SC connect/disconnect |
| DEBUG | Individual NPDU forwarding, FDT state changes, heartbeat messages |
| TRACE | Raw packet hex dumps, full APDU contents |

### 10.4 Key Events to Log

- Router engine started/stopped
- Transport mode changed (SC ↔ Tailscale)
- SC hub connection established / lost / reconnecting
- Foreign device registered / expired (Tailscale mode)
- LAN interface bound / unbound
- ForwardedNPDU sent to foreign device (count, not content)
- Error responses from remote BACnet devices
- Unknown NPDU types received

---

## 11. Testing Strategy

### 11.1 Unit Tests (`cargo test`)

Target: `bridge-core` crate. No external dependencies, no network.

| Test Suite | What It Tests |
|------------|--------------|
| `config_tests` | TOML round-trip: write config → read config → verify equality |
| `fdt_tests` | FDTEntry add/remove, TTL expiry, purge logic, BDT validation |
| `state_tests` | AppState transitions: stopped → starting → running → stopping → stopped |
| `transport_tests` | Transport enum construction from config, error handling for invalid configs |

### 11.2 Integration Tests (`cargo test --test *`)

Target: `tests/` directory. Requires loopback network, no Docker.

```rust
// router_tests.rs — test forwarding logic
#[tokio::test]
async fn test_who_is_forwarded_from_remote_to_lan() {
    // Spin up router with loopback transports
    // Send Who-Is on port 2
    // Assert it's received as broadcast on port 1
}

#[tokio::test]
async fn test_iam_response_routed_back() {
    // Send I-Am on port 1 (from a simulated LAN device)
    // Assert router table learns the route
    // Send ReadProperty to that device on port 2
    // Assert it's unicast-routed to port 1 with correct DADR
}
```

### 11.3 Docker E2E Tests

**Full topology test with docker compose.** Simulates the complete end-to-end flow.

#### 11.3.1 BACnet/SC Topology (`docker-compose.sc.yml`)

```
┌─────────────────┐     ┌──────────┐     ┌─────────────────┐     ┌──────────────┐
│ iComm Simulator │────►│ Laptop   │────►│ SC Hub          │────►│ Site Router  │
│ (BACnet/IP)     │     │ Router   │     │ (container)     │     │ (container)  │
│ port: 47808      │     │          │     │ port: 443       │     │              │
└─────────────────┘     └──────────┘     └─────────────────┘     └──────┬───────┘
                                                                        │
                                                                 ┌──────┴───────┐
                                                                 │ LAN Devices  │
                                                                 │ (simulator)  │
                                                                 │ port: 47808   │
                                                                 └──────────────┘
```

All containers on a Docker bridge network. SC hub uses self-signed TLS certs for testing.

#### 11.3.2 Tailscale BBMD Topology (`docker-compose.bbmd.yml`)

```
┌─────────────────┐         ┌─────────────────┐     ┌──────────────┐
│ iComm Simulator │────────►│ Site Router     │────►│ LAN Devices  │
│ (Foreign Device)│         │ (BBMD)          │     │ (simulator)  │
└─────────────────┘         └─────────────────┘     └──────────────┘
```

iComm simulator acts as Foreign Device, registers with the BBMD, then sends Who-Is and receives I-Am responses.

#### 11.3.3 Container Specifications

**iComm Simulator:**
- Built from rusty-bacnet's `bacnet-device` benchmark binary
- Configured as BACnet/IP UDP client (no SC)
- Test harness sends command to it: "discover devices" → triggers Who-Is → collects I-Am → selects first device → reads Device object name
- Validates: device name is non-empty and matches expected LAN device name

**LAN Device Simulator:**
- Built from rusty-bacnet's `bacnet-device` benchmark binary  
- Configured as a BACnet/IP server with 5 AnalogInput objects
- Device name: "Test-Device-01", instance: 1001
- Responds to Who-Is with I-Am, responds to ReadProperty

**SC Hub:**
- Built from rusty-bacnet's `bacnet-sc-hub` benchmark binary
- Self-signed TLS for test environment
- Port 443 exposed

**Laptop Router & Site Router:**
- Our `bacnet-bridge router` binary in a container
- Configured via environment variables (TOML env overrides)

#### 11.3.4 E2E Test Cases

| Test | Flow | Assertion |
|------|------|-----------|
| SC: Who-Is/I-Am discovery | iComm → Laptop Router → Hub → Site Router → LAN Device → I-Am back | Device 1001 discovered with correct name |
| SC: ReadProperty round-trip | After discovery, read AnalogInput:1 present-value | Value returned, no error |
| SC: Router table learning | After I-Am, check RouterTable on Site Router | Route to network 1 is learned |
| SC: Multiple LAN devices | 3 device simulators respond to Who-Is | All 3 I-Am responses received |
| Tailscale: Foreign Device registration | iComm simulator registers with BBMD | FDT shows 1 entry with correct IP:port |
| Tailscale: Who-Is/I-Am via BBMD | FD sends Who-Is, BBMD forwards to LAN, I-Am returned | Device 1001 discovered |
| Tailscale: Broadcast loop prevention | I-Am from LAN forwarded to FD, FD re-broadcasts | No echo (forwarded NPDU has correct source) |
| Transport switch | Start SC, switch to Tailscale, verify FDT appears | FDT table populated after switch |
| Reconnection | Kill hub, verify router detects disconnect, restart hub | Router reconnects and resumes forwarding |
| Config persistence | Change LAN port via web UI, restart router | New port is used after restart |

### 11.4 BTL Harness Integration

The rusty-bacnet BTL harness provides 3,808 tests that validate BACnet protocol correctness. We will:

1. **Use the `bacnet-test serve` command** to spin up a reference BTL-compliant server (65 objects, all types) as our LAN device simulator
2. **Run `bacnet-test run --target <router-ip>:47808 --section 9`** to validate BVLC/BBMD/FDT/Broadcast behavior through our router
3. **Add custom router test definitions** using the BTL harness engine's `TestDef` registration, targeting router-specific behavior:
   - NPDU forwarding correctness (DNET, DADR, SNET, SADR fields)
   - Router table learning and purging
   - Cross-transport forwarding (I-Am from LAN appears on SC spoke with correct network annotation)
   - Broadcast distribution to all Foreign Devices (one ForwardedNPDU per FD)
   - Foreign Device TTL management and expiry

### 11.5 MCP Integration for Debugging

The rusty-bacnet MCP server can run as a Docker sidecar during development:

```yaml
bacnet-mcp:
  image: ghcr.io/jscott3201/rusty-bacnet-mcp:latest
  command: ["--config", "/config/mcp.json", "--transport", "http", "--bind", "0.0.0.0:3000"]
  network_mode: host
```

Key tools for debugging: `probe_bbmd` (reads BDT+FDT), `discover_devices` (Who-Is sweep through router), `ping_device` (reachability test), `read_property` (verify read-through).

---

## 12. Build & Packaging

### 12.1 Build

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Windows: build with icon resource
cargo build --release --features windows-tray

# Hub mode only (no web UI, smaller binary)
cargo build --release --no-default-features --features hub
```

### 12.2 Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `router` | yes | Full router binary with web UI + tray + routing engine |
| `hub` | yes | SC hub mode |
| `serve` | yes | Web UI only mode (development/testing) |
| `windows-tray` | no | Windows system tray integration (`tray-item` crate) |

### 12.3 Windows Packaging

- Single `.exe` via static linking (rustls avoids OpenSSL DLL dependency)
- Embed `.ico` resources via `winres` crate
- Optional: MSI installer via `cargo wix`
- Optional: code signing for production distribution

### 12.4 Linux Packaging

- Single statically-linked binary (musl target for portability)
- Systemd service unit file for persistent router or hub deployment
- Docker image for hub and router containers

---

## 13. Deployment Topologies

### 13.1 Site Router (Windows)

```
[Site PC running Windows 10/11]
    │
    ├── BACnet Bridge (system tray app)
    │   ├── LAN BIP transport → physical Ethernet → BACnet controllers
    │   └── SC Spoke → cloud Hub (primary) / Tailscale BBMD (fallback)
    │
    └── Optional: Tailscale client (for fallback VPN)
```

### 13.2 Laptop Router (Windows)

```
[Service Laptop running Windows 10/11]
    │
    ├── iComm (Innotech BACnet client)
    │   └── BACnet/IP → localhost:47808 or Foreign Device mode
    │
    └── BACnet Bridge (system tray app)
        ├── Local BIP transport → iComm
        └── SC Spoke → cloud Hub
```

### 13.3 Cloud Hub (Linux)

```
[AWS EC2 / DigitalOcean Droplet / any VPS]
    │
    ├── bacnet-bridge hub
    │   ├── Binds 0.0.0.0:443
    │   ├── Let's Encrypt ACME auto-renewal
    │   └── Relays SC frames between connected spokes
    │
    └── Firewall: allow 443/tcp inbound
```

### 13.4 Docker Testing (Developer Machine)

```
[Dev PC]
    │
    └── docker compose up
        ├── bacnet-device-1 (LAN simulator, instance 1001)
        ├── bacnet-device-2 (LAN simulator, instance 1002)
        ├── site-router (our binary, port 0: LAN, port 1: SC)
        ├── sc-hub (TLS hub, self-signed)
        ├── laptop-router (our binary, port 0: local, port 1: SC)
        ├── icomm-simulator (test runner)
        └── test-runner (e2e orchestrator)
```

---

## 14. Dependencies

### 14.1 Core (rusty-bacnet crates)

| Crate | Version | Purpose |
|-------|---------|---------|
| `bacnet-types` | ^0.9 | Core BACnet enums, primitives, errors |
| `bacnet-encoding` | ^0.9 | ASN.1/BER, APDU/NPDU codec, segmentation |
| `bacnet-transport` | ^0.9 | BIP, SC, BBMD, FDT, `TransportPort` trait |
| `bacnet-network` | ^0.9 | `BACnetRouter`, router table, network layer |
| `bacnet-services` | ^0.9 | Service structs (WhoIs, IAm, etc.) — for testing |

### 14.2 Application

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio` | 1.x | Async runtime |
| `axum` | 0.8 | Web server |
| `tower-http` | 0.6 | Static file serving, CORS |
| `rust-embed` | 8.x | Embed web assets in binary |
| `serde` + `serde_json` | 1.x | JSON serialization for API |
| `toml` + `serde` | 0.8 | TOML config parsing |
| `clap` | 4.x | CLI argument parsing |
| `tracing` | 0.1 | Structured logging |
| `tracing-subscriber` | 0.3 | Log output formatting |
| `rustls` | 0.23 | TLS for BACnet/SC |
| `tokio-rustls-acme` | 0.6 | Let's Encrypt for hub mode |
| `tray-item` | 0.10+ | Windows system tray |
| `dirs` | 5.x | Platform-appropriate config directories |

### 14.3 Testing

| Crate | Version | Purpose |
|-------|---------|---------|
| `tokio-test` | 0.4 | Async test utilities |
| `testcontainers` | 0.23 | Docker container management in tests |
| `reqwest` | 0.12 | HTTP client for web API tests |
| `tungstenite` | 0.24 | WebSocket client for log stream tests |

---

## Appendix A: Feature Parity Checklist (Python → Rust)

| Python Feature | Rust Implementation |
|----------------|-------------------|
| Network interface detection (`psutil`) | `get_if_addrs` or `netdev` crate; Tailscale detection via `100.` prefix |
| BBMD on Tailscale IP | `BipTransport::enable_bbmd()` |
| BIPSimple on LAN IP | `BipTransport::new()` |
| Foreign Device registration | `BbmdState::register_foreign_device()` |
| FDT table display | REST API `GET /api/fdt` → HTML table |
| Broadcast forwarding (monkey-patch) | `BACnetRouter` with 2 ports (proper Clause 6 routing) |
| Who-Is/I-Am cross-forwarding | Router's built-in broadcast handling |
| Config persistence (JSON) | TOML file at known path + env overrides |
| Web dashboard (NiceGUI) | axum + HTMX + Tailwind CSS |
| System tray (pystray) | `tray-item` crate + `.ico` resources |
| Dynamic tray icon (red/green) | `.ico` swap via `tray-item` |
| VPN-only log filter | Client-side search filter on log viewer |
| Windows Firewall instructions | Documentation only (same PowerShell command) |
| Single .exe packaging (PyInstaller) | Static linking → single .exe |

## Appendix B: Terminology

See [CONTEXT.md](../CONTEXT.md) for the full domain glossary.

## Appendix C: References

- [rusty-bacnet](https://github.com/jscott3201/rusty-bacnet) — Core BACnet library (v0.9.0)
- [rusty-bacnet-btl-harness](https://github.com/jscott3201/rusty-bacnet-btl-harness) — BTL compliance test harness (3,808 tests)
- [rusty-bacnet-mcp](https://github.com/jscott3201/rusty-bacnet-mcp) — MCP server for BACnet debugging
- [ASHRAE 135-2020](https://www.ashrae.org/technical-resources/bookstore/standard-135-2020) — BACnet standard
- [BACnet/SC Addendum (Annex AB)](https://bacnet.org) — Secure Connect specification
- [Tailscale](https://tailscale.com) — VPN for fallback transport
- [iComm](https://innotech.com) — Innotech BACnet client software
