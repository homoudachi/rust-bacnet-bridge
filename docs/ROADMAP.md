# Roadmap

## Status

**Phase:** Pre-implementation. Spike validated: `BACnetRouter` with two loopback ports correctly forwards global broadcasts (Who-Is, I-Am) and unicast (ReadProperty) with proper SNET/SADR and DNET/DADR handling. See `examples/spike-two-port-router/`.

## Key design decisions (from grilling session)

- Router binary treats all deployments identically — no `role` field in config (Laptop vs Site is operational, not code path)
- Embedded Hub mode (`--with-hub`): Site Router can simultaneously run SC Hub, eliminating need for cloud VPS
- Port 0 is always `BipTransport` (LAN); port 1 is `AnyTransport` (SC or BBMD, built by factory function)
- Config auto-generated on first run with device_id 4194303 (max); operator adjusts before starting
- BTL target: ~253 router-relevant tests (not full 3,808); runs as separate CI job
- Docker for Hub + e2e testing only — NOT for Site/Laptop Router deployment
- Docker containers use env vars exclusively (no TOML file)
- Three-state tray: green (connected), amber (reconnecting/fallback), red (disconnected)
- Packet loss during manual transport switch is acceptable
- Tailwind CSS pre-built and committed to repo (no build.rs/npm dependency at compile time)
- Default config bind is `0.0.0.0` (accessible over Tailnet); dashboard serves 2-3 concurrent connections max

## Implementation order

### Phase 1: Workspace scaffold + bridge-core
1. Root `Cargo.toml` workspace with members `crates/bridge-core`, `src/` binary
2. `bridge-core`: `config.rs` — TOML load/save with auto-generation on first run, env var overrides
3. `bridge-core`: `transport.rs` — `build_remote_transport(config) -> AnyTransport` factory (SC or BBMD)
4. `bridge-core`: `router.rs` — wrapper around `BACnetRouter` that:
   - Creates two `RouterPort`s (LAN BIP + `AnyTransport` remote)
   - Exposes start/stop lifecycle
   - Handles `_local_rx` with minimal BACnet device (Who-Is/I-Am, ReadProperty on Device object)
5. `bridge-core`: `sc_transport.rs` — SC spoke via `ScTransport<TlsWebSocket>`
6. `bridge-core`: `bbmd_transport.rs` — BBMD via `BipTransport::enable_bbmd()`
7. `bridge-core`: `local_device.rs` — minimal BACnet device for network-layer + app-layer queries
8. `bridge-core`: `fdt.rs` — FDT management (Tailscale mode only)
9. `bridge-core`: `state.rs` — `AppState` state machine (Stopped/Starting/Running/Stopping) + `tokio::sync::watch` for tray sync
10. `bridge-core`: `error.rs` — error types
11. Unit tests for config round-trip, transport construction, FDT lifecycle

### Phase 2: bridge-app binary
12. `main.rs` — clap CLI with `router`, `hub`, `serve` subcommands
13. `router_cmd.rs` — full app: web server + routing engine + optional tray + optional `--with-hub` (embedded SC Hub)
14. `hub_cmd.rs` — SC hub via `ScHub` (self-signed certs for dev; ACME for cloud)
15. `serve_cmd.rs` — web UI only (testing + operational read-only mode), `--dev` flag for filesystem assets
16. Manual transport switch (stop → rebuild → start) with state machine gating
17. Integration tests: router forwarding over loopback BIP

### Phase 3: Web dashboard
18. axum server with embedded HTMX + Tailwind CSS assets (pre-built CSS committed to repo)
19. Status bar (transport indicator, uptime)
20. Network config panel (interfaces dropdown, transport toggle, Hub Mode: Cloud vs Embedded)
21. FDT table (HTMX polling, Tailscale mode only)
22. Live log viewer (WebSocket streaming, free-text search + severity dropdown)
23. REST API endpoints per FSD section 7.3 (409 Conflict for illegal state transitions)

### Phase 4: Windows system tray
24. `tray-item` integration with green/amber/red `.ico`
25. Right-click menu (Open Dashboard, Start/Stop, Switch Transport, Exit) — dynamically greyed by state
26. State sync via `tokio::sync::watch` (three-state: green=connected, amber=reconnecting/fallback, red=disconnected)

### Phase 5: Docker e2e + CI
27. `docker-compose.sc.yml` — BACnet/SC full topology (env-var-only config, no TOML volumes)
28. `docker-compose.bbmd.yml` — Tailscale BBMD topology
29. E2E test runner: Who-Is → I-Am → ReadProperty round-trip
30. GitHub Actions: `cargo test`, `cargo clippy`, `cargo fmt --check`, Docker compose e2e on push (all test tiers)

### Phase 6: Polish
31. Dependabot for auto version bumps
32. BTL harness integration as separate CI job (~253 router-relevant tests: BVLC/BBMD §9.3, SC §9.9, routing §10)
33. Release build: `cargo build --release --features windows-tray`

## Next session start point

Run `examples/spike-two-port-router/` to re-verify router integration works, then begin Phase 1: create the Cargo workspace and `bridge-core` crate. Pin rusty-bacnet deps to latest crates.io version at start of each session.
