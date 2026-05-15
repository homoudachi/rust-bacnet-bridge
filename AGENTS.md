# AGENTS.md

## Current state

~99% complete. All five implementation phases built; Phase 6 (polish) is complete.

**Built and working:**
- Workspace: root `Cargo.toml` with `crates/bridge-core` + `src/` binary
- Core routing: `BACnetRouter` with two ports (LAN BIP + SC or BBMD transport)
- Config: TOML load/save, env-var overrides, auto-generation on first run
- Transports: BACnet/SC spoke via `TlsWebSocket`, BBMD via `BipTransport::enable_bbmd()`
- FDT: foreign device table management (Tailscale mode only)
- State machine: `AppState` (Stopped/Starting/Running/Stopping) with `tokio::sync::watch`
- CLI: `router`, `hub`, `serve` subcommands via clap
- Embedded Hub mode: `--with-hub` runs SC Hub + Router simultaneously
- Web dashboard: axum + HTMX + Tailwind CSS, embedded assets (rust-embed)
- REST API: status, config, FDT, logs, transport switch, WebSocket log streaming, hub mode switch, system interface detection
- Windows system tray: green/amber/red DIB icons, right-click menu, state-gated items
- Docker: compose topologies for SC, BBMD, and BTL harness
- CI: GitHub Actions (fmt, clippy, test, docker e2e, BTL on push)
- Tests: 83 tests (unit + integration) covering config, FDT, routing, SC, BBMD, transport switch, dashboard API
- ACME TLS support for Hub mode (stubbed, staging-ready)
- Dependabot configuration (weekly Cargo + GitHub Actions)
- Release build CI artifact (`bacnet-bridge.exe` upload)
- BTL harness CI integration (health checks, improved compose)
- Router Start command (state-gated stop-build-start)
- Full transport switch cycle (stop-build-start)
- System interface detection via `get_if_addrs` (runtime, not just config)
- Dashboard API tests covering status, config, FDT, logs, transport lifecycle, hub mode

## Domain language

CONTEXT.md defines precise terminology. Use these terms, not casual synonyms:
- **Router** = BACnet NPDU forwarder (not IP router). The product is "BACnet Bridge", the component is "Router".
- **Hub** = BACnet/SC cloud relay. Not "server" or "broker".
- **Spoke** = BACnet/SC node connected to a Hub. Not "client".
- **Transport** = BACnet/SC (WebSocket+TLS) or BACnet/IP (UDP over Tailscale). Not "connection".
- **BBMD** / **Foreign Device** = BACnet/IP broadcast relay roles.
- **Site Router** = bridge at the facility. **Laptop Router** = bridge on service laptop alongside iComm.
- Network numbers: LAN = network 1, SC spoke side = network 2.

## Architecture

- Single binary `bacnet-bridge` with subcommands: `router`, `hub`, `serve`
- Crate split: `bridge-core` (routing engine) + binary in workspace `src/` (web UI, tray, CLI)
- Core routing delegated to `rusty-bacnet`'s `BACnetRouter` with two ports (LAN + remote transport)
- Port 0: always `BipTransport` (LAN). Port 1: `AnyTransport` — `ScTransport<TlsWebSocket>` or `BipTransport` with BBMD enabled
- Remote transport built by factory function `build_remote_transport()` — no custom `Transport` enum needed
- Exactly two transports, one active at a time — switching is manual only (no auto-failover)
- Router has no `role` field; Laptop and Site deployments are identical code paths
- Embedded Hub mode (`--with-hub`): Site Router runs SC Hub + Router simultaneously (no VPS needed)
- Web dashboard: axum + HTMX + Tailwind CSS, served from embedded assets (pre-built CSS committed to repo)
- System tray: Windows only, `tray-item` crate with green/amber/red `.ico` icons; three-state: green=connected, amber=reconnecting/fallback, red=disconnected
- Config: TOML on disk + env-var overrides (prefix `BACNET_BRIDGE__`), auto-generated on first run
- Testing: `cargo test` (unit + integration), Docker compose e2e, BTL harness for compliance

## Source of truth

`docs/FSD.md` is the authoritative plan. ADRs in `docs/adr/` explain key decisions. `docs/ROADMAP.md` tracks implementation progress.

## Key constraints

### Issue tracker

GitHub Issues on `homoudachi/rust-bacnet-bridge`. See `docs/agents/issue-tracker.md`.

### Triage labels

Standard five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context repo: `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.
