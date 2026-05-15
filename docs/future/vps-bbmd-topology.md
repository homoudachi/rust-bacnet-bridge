# Future Plan: VPS BBMD + Site Foreign Device Topology

**Status:** Planned (not implemented)
**Date:** 2026-05-16
**Depends on:** Nothing — the rusty-bacnet library already supports the required primitives.

---

## 1. Problem

Currently the bridge has three deployment topologies:

| Topology | Transport | Tailscale Required? |
|---|---|---|
| SC hub-and-spoke | SC (WebSocket+TLS) | No |
| Tailscale fallback (Site Router as BBMD, laptop/iComm as FD) | BBMD + Foreign Device over Tailscale | **Yes** |
| Embedded hub (Site Router runs hub + router) | SC (loopback) | No |

There is no **Tailscale-free BBMD topology**. The user wants a VPS with a public IP running as a BBMD server, and site routers behind CGNAT registering as Foreign Device clients — no VPN, no Tailscale, no SC hub.

### Why this matters

Some operators cannot or prefer not to run an SC Hub (TLS cert management, WebSocket protocol overhead). Others have BACnet devices that speak BACnet/IP natively and don't need SC's features. A pure BBMD+FD topology over public IP is simpler at the protocol level — just BACnet/IP all the way through.

---

## 2. How It Works (Protocol Level)

```
Site 1 (CGNAT)                  VPS (Public IP)               Site 2 (CGNAT)
┌──────────────────┐            ┌──────────────┐            ┌──────────────────┐
│ bacnet-bridge    │ UDP:47808  │ bacnet-bridge│ UDP:47808  │ bacnet-bridge    │
│ (Foreign Device) ├────────────► (BBMD)       │◄────────────┤ (Foreign Device) │
│                  │ Register   │              │ Register   │                  │
│  LAN: network 1  │  ← Result  │  FDT:        │  Result →  │  LAN: network 1  │
│                  │  Forwarded │   Site1:port │  Forwarded │                  │
│                  │◄─ NPDU ────┤   Site2:port ├── NPDU ──► │                  │
└──────┬───────────┘            └──────────────┘            └──────┬───────────┘
       │                                                            │
       │ BACnet/IP (LAN)                                            │ BACnet/IP (LAN)
       ▼                                                            ▼
  [BACnet controllers]                                      [BACnet controllers]
```

1. Site router starts with `foreign_device_mode = true`, configured with VPS IP and port
2. Site router's `BipTransport` calls `register_as_foreign_device(vps_ip, vps_port, ttl)`
3. This sends a BVLC `Register-Foreign-Device` (0x81 0x05) UDP message to the VPS BBMD
4. CGNAT creates a UDP NAT mapping — the BBMD receives the packet from `[CGNAT-public-IP:random-port]`
5. VPS BBMD records the source IP:port in its FDT (rusty-bacnet's `BbmdState` handles this)
6. When VPS BBMD needs to forward a broadcast to the site, it sends `Forwarded-NPDU` (0x81 0x0A) to the FDT entry's IP:port — CGNAT reverses the mapping
7. The site router re-registers every `TTL/2` seconds to keep the CGNAT UDP mapping alive

### CGNAT UDP timeout risk

The critical variable is the re-registration interval. CGNAT UDP timeouts vary:

| Carrier Type | Typical UDP Timeout |
|---|---|
| Mobile/cellular CGNAT | 30–60s |
| Residential CGNAT | 2–5 min |
| Enterprise CGNAT | 5–30 min |

The default TTL in the current config is **300s** → re-reg at **150s**. This would fail through a cellular CGNAT (30–60s timeout) because the UDP mapping dies before re-registration fires.

**Solution:** For CGNAT deployments, configure `ttl = 60` → re-reg at `30s`. This keeps the mapping alive across all carrier types.

### Full device discovery flow

One Foreign Device registration per site exposes ALL devices on that site's LAN to remote laptops:

```
iComm (laptop)                 VPS BBMD                   Site Router               LAN
                              (public IP)
Who-Is broadcast ──► (FD) ──► BBMD ──► Broadcast to ──► (FD) ──► Router port 2 ──► Router port 1 ──► Who-Is broadcast
                                 all registered FDs          ↑         (network 2)        (network 1)           to all devices

                                                                  I-Am responses:       ◄── device1 ◄────────── device1
                                                                  device2 ◄─────────────── device2
                                                                  device3 ◄─────────────── device3

                              BBMD ◄── Distribute-Broadcast ── Router: port 1 ◄── port 2
                                │        (contains I-Am)
                                │
iComm ◄── Forwarded-NPDU ◄─────┘
(all I-Am responses)
```

The Site Router has **two networks** — LAN on port 0 (network 1) and the BBMD transport on port 1 (network 2). The BACnetRouter handles proper Clause 6 routing. When a broadcast arrives on port 1 from the VPS BBMD, the router re-broadcasts it on port 0 (LAN). Devices respond to port 0, the router forwards unicast responses back to port 1, and the Foreign Device transport sends them as `Distribute-Broadcast-to-Network` back to the VPS BBMD, which forwards them to the laptop.

The laptop can connect two ways:

| Way | Setup | Good for |
|---|---|---|
| A: Laptop Router as FD | iComm → Laptop Router:127.0.0.1 (FD) → VPS BBMD | Symmetric deployment, dashboard on laptop too |
| B: iComm directly as FD | iComm registers as FD with VPS BBMD | No extra software on laptop (iComm handles FD natively) |

Either way, the laptop discovers every BACnet controller on every site's LAN. The Site Router does the LAN-side broadcasting — it's a proper router, not a single-device proxy.

---

## 3. Code Changes Required

### 3.1 Config: `crates/bridge-core/src/config.rs`

Rename `TailscaleConfig` to `BipConfig` (backward-compat via `#[serde(alias)]`), and add Foreign Device client fields:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BipConfig {        // was TailscaleConfig
    pub interface: String,    // local IP to bind (unchanged)
    pub port: u16,            // local port (unchanged)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bdt: Option<Vec<BdtEntry>>,  // peer BBMDs — BBMD server mode

    // NEW: Foreign Device client mode
    pub foreign_device_mode: bool,
    pub bbmd_ip: String,           // VPS BBMD public IP
    pub bbmd_port: u16,            // VPS BBMD port (default 47808)
    pub foreign_device_ttl: u16,   // re-reg TTL in seconds (default 60 for CGNAT)
}
```

TOML config for the VPS (BBMD server):
```toml
[router]
transport = "bbmd"    # new transport name

[router.bbmd]         # was [router.tailscale]
interface = "0.0.0.0"
port = 47808
bdt = []              # empty BDT = standalone BBMD
```

TOML config for site routers (Foreign Device clients):
```toml
[router]
transport = "bbmd"

[router.bbmd]
interface = "0.0.0.0"           # LAN IP or 0.0.0.0
port = 47808
foreign_device_mode = true
bbmd_ip = "203.0.113.5"        # VPS public IP
bbmd_port = 47808
foreign_device_ttl = 60        # 30s re-reg interval
```

Env var overrides:
```bash
BACNET_BRIDGE_ROUTER__TRANSPORT=bbmd
BACNET_BRIDGE_ROUTER__BBMD__FOREIGN_DEVICE_MODE=true
BACNET_BRIDGE_ROUTER__BBMD__BBMD_IP=203.0.113.5
BACNET_BRIDGE_ROUTER__BBMD__BBMD_PORT=47808
BACNET_BRIDGE_ROUTER__BBMD__FOREIGN_DEVICE_TTL=60
```

### 3.2 Transport factory: `crates/bridge-core/src/transport.rs`

Add `"bbmd"` transport mode to the match:

```rust
"bbmd" => build_bbmd_transport(&config.router.bbmd).await,
// keep "tailscale" for backward compat, alias to bbmd
"tailscale" => build_bbmd_transport(&config.router.tailscale).await,
```

### 3.3 BBMD transport: `crates/bridge-core/src/bbmd_transport.rs`

Add Foreign Device client path:

```rust
pub async fn build_bbmd_transport(
    config: &BipConfig,
) -> Result<AnyTransport<NoSerial>, BridgeError> {
    let interface: Ipv4Addr = config.interface.parse()...;
    let broadcast = Ipv4Addr::new(255, 255, 255, 255);
    let mut transport = BipTransport::new(interface, config.port, broadcast);

    if config.foreign_device_mode {
        let bbmd_ip = config.bbmd_ip.parse::<Ipv4Addr>()...;
        transport.register_as_foreign_device(
            ForeignDeviceConfig {
                bbmd_ip: bbmd_ip.octets(),
                bbmd_port: config.bbmd_port,
                ttl: config.foreign_device_ttl,
            }
        );
        tracing::info!(
            "Foreign Device mode: registering with BBMD {}:{} (TTL={}s)",
            config.bbmd_ip, config.bbmd_port, config.foreign_device_ttl
        );
    } else if let Some(bdt) = &config.bdt {
        // Existing BBMD server path
        let bbmd_bdt: Vec<BbmdBdtEntry> = bdt.iter().map(convert_bdt_entry).collect...;
        transport.enable_bbmd(bbmd_bdt);
    } else {
        tracing::info!("BIP transport created on {}:{} (no BBMD, no FD mode)", ...);
    }

    Ok(AnyTransport::Bip(transport))
}
```

### 3.4 FDT sync: `crates/bridge-core/src/fdt.rs`

Currently `FdtManager` is a standalone mirror never populated from `BbmdState`. Need to:

1. Expose `BbmdState` from `RunningRouter` via a method like `fn bbmd_state(&self) -> Option<Arc<Mutex<BbmdState>>>`
2. In `router_cmd.rs`, periodically read `BbmdState::fdt()` and push entries into `FdtManager`

This is the only change >10 lines — but it's optional for MVP. The VPS BBMD's dashboard could show `[]` FDT until this is wired up. The routing still works without it.

### 3.5 Config backward compat: `crates/bridge-core/src/config.rs`

Existing `TailscaleConfig` becomes `BipConfig`. The `RouterConfig` struct keeps `tailscale` as a field that serde aliases from both `"tailscale"` and `"bbmd"` in TOML, and env override code paths `"tailscale"` and `"bbmd"` both map to the same underlying config:

```rust
pub struct RouterConfig {
    // ...
    #[serde(alias = "tailscale")]
    pub bbmd: BipConfig,    // was tailscale: TailscaleConfig
}
```

### 3.6 Env var overrides

Add entries in `apply_router_override` for `"bbmd"` (mirroring the existing `"tailscale"` block):

```rust
"bbmd" => {
    match parts[1] {
        "interface" => router.bbmd.interface = value.to_string(),
        "port" => { if let Ok(v) = value.parse::<u16>() { router.bbmd.port = v; } }
        "foreign_device_mode" => { if let Ok(v) = value.parse::<bool>() { router.bbmd.foreign_device_mode = v; } }
        "bbmd_ip" => router.bbmd.bbmd_ip = value.to_string(),
        "bbmd_port" => { if let Ok(v) = value.parse::<u16>() { router.bbmd.bbmd_port = v; } }
        "foreign_device_ttl" => { if let Ok(v) = value.parse::<u16>() { router.bbmd.foreign_device_ttl = v; } }
        "bdt" => {} // BDT array still not overridable via env
        _ => warn!("Unknown config key: router.bbmd.{}", parts[1]),
    }
}
```

### 3.7 Dashboard API: `src/web/api.rs`

Update `get_fdt()` to check for `"bbmd"` transport alongside `"tailscale"`. Currently it returns empty for non-tailscale. Change:

```rust
"tailscale" | "bbmd" => state.inner.fdt.lock().await.list(),
```

### 3.8 Dashboard transport switch

Add `"bbmd"` as a valid transport option in the UI and the transport switch API endpoint.

---

## 4. Files Changed (Summary)

| File | Change | Lines |
|---|---|---|
| `crates/bridge-core/src/config.rs` | Rename TailscaleConfig → BipConfig, add FD fields, add env overrides, keep backward compat | ~60 |
| `crates/bridge-core/src/bbmd_transport.rs` | Add Foreign Device client branch | ~25 |
| `crates/bridge-core/src/transport.rs` | Add `"bbmd"` transport, keep `"tailscale"` alias | ~3 |
| `crates/bridge-core/src/router.rs` | Expose bbmd_state() for FDT sync | ~5 |
| `crates/bridge-core/src/fdt.rs` | Wire BbmdState polling into FdtManager (optional, phase 2) | ~30 |
| `src/web/api.rs` | Accept `"bbmd"` in get_fdt(), transport switch | ~5 |
| `src/web/` (routes) | Add `"bbmd"` to transport switch endpoint | ~3 |
| `src/router_cmd.rs` | Wire BbmdState → FdtManager sync (optional, phase 2) | ~15 |

**Total: ~100 lines** (without FDT sync), ~150 lines (with FDT sync).

---

## 5. Testing Plan

### 5.1 Unit tests (in `config.rs`)

- Default `BipConfig` has `foreign_device_mode = false`, `bbmd_ip = ""`, `foreign_device_ttl = 60`
- TOML round-trip preserves all new fields
- Env override `BACNET_BRIDGE_ROUTER__BBMD__FOREIGN_DEVICE_MODE=true` works
- Backward compat: old `[router.tailscale]` TOML section still deserializes into `BipConfig`
- Backward compat: `BACNET_BRIDGE_ROUTER__TAILSCALE__INTERFACE` env var still works

### 5.2 Integration tests (in `transport.rs`)

- `build_bbmd_transport` with `foreign_device_mode = true` creates a BipTransport with FD registration
- `build_bbmd_transport` with BBMD mode and BDT entries still works (backward compat)
- `build_remote_transport` with `transport = "bbmd"` dispatches correctly

### 5.3 Docker e2e test

Add `docker-compose.bbmd-vps.yml`:
```
vps-bbmd:    # BipTransport + enable_bbmd() + empty BDT
site-router: # BipTransport + register_as_foreign_device(vps-bbmd, 60)
icomm-sim:   # registers as FD to site-router
```

Test: iComm sends Who-Is → site router forwards → VPS BBMD receives → VPS BBMD distributes to all registered FDs → site router receives → broadcasts to LAN devices → device responds with I-Am → flows back through router table → iComm receives I-Am.

### 5.4 BTL harness

The existing 72 BVLC/BBMD tests (9.3.1–9.3.72) in `bacnet-btl` already cover Register-FD, Distribute-Broadcast, Forwarded-NPDU. No new BTL tests needed — this is a config/code-path change, not a protocol change.

---

## 6. Limitations & Caveats

1. **FDT display on VPS is blank until FDT sync is wired.** The VPS BBMD dashboard won't show connected site routers until Phase 2 (BbmdState → FdtManager sync). Routing works regardless.

2. **No TLS for BBMD.** BVLC messages are plain UDP. This is acceptable for BACnet deployments (standard BACnet/IP is unencrypted). If encryption is needed, use the SC transport instead.

3. **Single BBMD, no failover.** The VPS is a single point of failure. Multi-BBMD with BDT can be added later for high availability.

4. **TTL must match carrier.** The operator must know their ISP's CGNAT UDP timeout and set `foreign_device_ttl` accordingly. A 60s default (30s re-reg) is safe for all known carriers.

5. **No auto-detection of CGNAT timeout.** The bridge doesn't probe the NAT timeout. The operator configures TTL manually.

6. **iComm still needs reconfiguration on transport switch.** If switching from SC → BBMD mode, iComm's Foreign Device IP must be changed. This is unchanged from the existing FSD.

---

## 7. Why This Over SC Hub?

| | SC Hub | VPS BBMD + FD |
|---|---|---|
| Protocol | WebSocket+TLS | Plain UDP |
| TLS cert management | Required (Let's Encrypt or self-signed) | None |
| NAT traversal | Spoke outbound TCP (automatic) | FD outbound UDP (TTL-dependent) |
| Encryption | TLS | None (standard BACnet/IP) |
| Complexity | Higher (TLS, WebSocket, VMAC) | Lower (just BVLC over UDP) |
| Throughput | WebSocket framing overhead | Raw UDP, lower overhead |
| BTL certification path | SC tests (9.9.x) | BBMD tests (9.3.x) |

For operators who don't need SC's features and are comfortable with unencrypted BACnet/IP, the BBMD+FD topology is simpler and lighter.
