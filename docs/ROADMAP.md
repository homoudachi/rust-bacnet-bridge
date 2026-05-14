# Roadmap

## Status

**Phase:** Pre-implementation. Spike validated: `BACnetRouter` with two loopback ports correctly forwards global broadcasts (Who-Is, I-Am) and unicast (ReadProperty) with proper SNET/SADR and DNET/DADR handling. See `examples/spike-two-port-router/`.

## Implementation order

### Phase 1: Workspace scaffold + bridge-core
1. Root `Cargo.toml` workspace with members `crates/bridge-core`, `src/` binary
2. `bridge-core`: `config.rs` (TOML load/save), `transport.rs` (Transport enum), `error.rs`
3. `bridge-core`: `router.rs` — wrapper around `BACnetRouter` that:
   - Creates two `RouterPort`s (LAN BIP + remote transport)
   - Exposes start/stop lifecycle
4. `bridge-core`: `sc_transport.rs` — SC spoke via `ScTransport<TlsWebSocket>`
5. `bridge-core`: `bbmd_transport.rs` — BBMD via `BipTransport::enable_bbmd()`
6. `bridge-core`: `fdt.rs` — FDT management (Tailscale mode only)
7. `bridge-core`: `state.rs` — `AppState` with `tokio::sync::watch` for tray sync
8. Unit tests for config round-trip, transport construction, FDT lifecycle

### Phase 2: bridge-app binary
9. `main.rs` — clap CLI with `router`, `hub`, `serve` subcommands
10. `router_cmd.rs` — full app: web server + routing engine + optional tray
11. `hub_cmd.rs` — SC hub via `ScHub` (self-signed certs for dev)
12. `serve_cmd.rs` — web UI only (testing mode)
13. Manual transport switch (stop → rebuild → start)
14. Integration tests: router forwarding over loopback BIP

### Phase 3: Web dashboard
15. axum server with embedded HTMX + Tailwind CSS assets
16. Status bar (transport indicator, uptime)
17. Network config panel (interfaces dropdown, transport toggle)
18. FDT table (HTMX polling, Tailscale mode only)
19. Live log viewer (WebSocket streaming)
20. REST API endpoints per FSD section 7.3

### Phase 4: Windows system tray
21. `tray-item` integration with green/amber/red `.ico`
22. Right-click menu (Open Dashboard, Start/Stop, Switch Transport, Exit)
23. State sync via `tokio::sync::watch`

### Phase 5: Docker e2e + CI
24. `docker-compose.sc.yml` — BACnet/SC full topology
25. `docker-compose.bbmd.yml` — Tailscale BBMD topology
26. E2E test runner: Who-Is → I-Am → ReadProperty round-trip
27. GitHub Actions: `cargo test`, Docker compose e2e on push

### Phase 6: Polish
28. Dependabot for auto version bumps
29. BTL harness integration (aspirational)
30. Release build: `cargo build --release --features windows-tray`

## Next session start point

Run `examples/spike-two-port-router/` to re-verify router integration works, then begin Phase 1: create the Cargo workspace and `bridge-core` crate.
