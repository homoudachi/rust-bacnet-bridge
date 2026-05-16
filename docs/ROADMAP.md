# Roadmap

## Status

**Phase:** Phase 6 (polish) — **Complete.** All five implementation phases built; all polish items completed; BTL Section 9 (494/494) and Section 10 (15/15) passing; FSD alignment audit complete 2026-05-16.

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
32. BTL harness integration as CI job ~~(~253 router-relevant tests: BVLC/BBMD §9.3, SC §9.9, routing §10)~~ — **Complete: 494 DLL + 15 routing tests all pass**
33. Release build: `cargo build --release --features windows-tray`

## All Phase 6 items completed

- [x] ACME TLS support (tokio-rustls-acme with DirCache, staging)
- [x] Dependabot configuration (weekly Cargo + GitHub Actions)
- [x] Release build CI artifact (bacnet-bridge.exe upload)
- [x] BTL harness CI (health checks, improved compose)
- [x] Router Start command (state-gated stop-build-start)
- [x] Full transport switch cycle (stop-build-start)
- [x] rcgen version unified (0.14 across workspace)
- [x] AGENTS.md updated (~95% implementation state)

- [x] BTL harness commit pinning for reproducible builds (#31)
- [x] Full end-to-end integration tests for the transport switch cycle (#32)
- [x] Dependabot config verified and corrected (#33)
- [x] Production Let's Encrypt ACME — config flag + CLI (#30)

## Phase 6 FSD alignment work (2026-05-16)

- [x] State gating: `transport_stop` and `transport_switch` return 409 Conflict per FSD 8.5 (#38, #39)
- [x] CLI params: `--log-level` on `router`, `--port`/`--host` on `serve` (#40)
- [x] State machine unit tests: 6 tests for valid/invalid transitions (#41)
- [x] Transport unit tests: 4 tests for SC/BBMD dispatch and error handling (#41)
- [x] Dashboard API integration tests: 25 tests covering all REST endpoints (#44)
- [x] WebSocket `fields` key on log messages per FSD 7.4 (#45)
- [x] Local BACnet device: active transport mode property per FSD 5.2 (#47)
- [x] Router Info dashboard section with connected networks table per FSD 7.2.5 (#46)
- [x] System interface detection via `get_if_addrs` per FSD 7.2.2 (#42)
- [x] FDT manager wired to real BbmdState for runtime population (#49)
- [x] Docker BTL healthcheck port fix (8080 → 28821) (#43)
- [x] favicon.ico asset created

## 2026-05-16 hardening

- [x] BTL harness compose: fixed hostname→IP resolution (site-router → 172.20.0.3)
- [x] BBMD E2E: compute BIP MAC from IP+port (not local_mac() before start), bind to INADDR_ANY (workaround rusty-bacnet#22)
- [x] .gitignore hardened (secrets patterns)
- [x] MIT LICENSE added
- [x] Legacy Python prototype expunged from git history
- [x] BTL Section 9 (DLL): 494/494 tests passing (#63)
- [x] BTL Section 10 (Routing): 15/15 tests passing (#63)
- [x] ReadProperty ACK encoding fix: context tag [2]→[3], service dispatch apdu[1]→apdu[3] (#59, #63)
- [x] NPDU routing fix: use BIP MAC instead of loopback MAC, explicit destination network (#63)
- [x] BBMD LAN enable: `enable_bbmd(vec![])` on LAN transport for foreign device registration (#63)
- [x] SubscribeCOV handler: stub SimpleAck response for COV subscription requests (#63)

### Notes

- **BTL harness:** Full BTL compliance verified — Section 9 (494/494 DLL tests) and Section 10 (15/15 routing tests) all pass. The compose uses static IP `172.20.0.3` for the site-router since Docker compose networking does not resolve service hostnames from the BTL runner container.

- **rusty-bacnet#22:** BBMD transport requires `BipTransport::bind()` to `[0; 4]` (INADDR_ANY) before `enable_bbmd()`, and the BIP MAC address must be derived from the local IP+port combination rather than calling `local_mac()` pre-start. A PR (#21) is pending upstream.

### Known dependencies

- **rusty-bacnet#22** (PR#21 pending): BBMD transport workaround described above. Blocks removal of the INADDR_ANY bind and MAC computation workaround.

## Next steps (future)

### Docker E2E testing plan

The `docker/docker-compose.sc.yml` and `docker-compose.bbmd.yml` files exist and define full BACnet topology tests. They build Rust from source in multi-stage Docker builds (4-5 images). First build ~30-60 min; incremental builds fast after layer caching.

For a future phase:
1. Run `docker compose -f docker/docker-compose.sc.yml build` to bootstrap images
2. Run `docker compose -f docker/docker-compose.sc.yml up --abort-on-container-exit` for SC topology test
3. Repeat for BBMD topology (`docker-compose.bbmd.yml`)
4. Wire into CI if reliable (currently known pre-existing failures on master to harden)
5. Add a `docker-compose.test.yml` with a test-runner service that validates Who-Is/I-Am/ReadProperty round-trips

### BTL ACME production mode

The `--acme-production` flag on the `hub` subcommand toggles between Let's Encrypt staging and production. Staging is safe for CI/BTL testing; production requires a real public domain. No code changes needed — just set the flag when deploying a production Hub.
