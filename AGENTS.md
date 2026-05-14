# AGENTS.md

## Current state

No Rust code exists yet. The repo is in pre-implementation: `docs/FSD.md` describes the planned Rust rewrite. The Python prototype has been removed.

## Domain language

CONTEXT.md defines precise terminology. Use these terms, not casual synonyms:
- **Router** = BACnet NPDU forwarder (not IP router). The product is "BACnet Bridge", the component is "Router".
- **Hub** = BACnet/SC cloud relay. Not "server" or "broker".
- **Spoke** = BACnet/SC node connected to a Hub. Not "client".
- **Transport** = BACnet/SC (WebSocket+TLS) or BACnet/IP (UDP over Tailscale). Not "connection".
- **BBMD** / **Foreign Device** = BACnet/IP broadcast relay roles.
- **Site Router** = bridge at the facility. **Laptop Router** = bridge on service laptop alongside iComm.
- Network numbers: LAN = network 1, SC spoke side = network 2.

## Architecture (planned, not built)

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

## Source of truth

`docs/FSD.md` is the authoritative plan. ADRs in `docs/adr/` explain key decisions. The Python code in `bacnet_bbmd_tool_tailscale/` is a proof-of-concept with known defects (malformed packets visible in Wireshark, monkey-patched broadcast forwarding bypassing proper Clause 6 routing). Do not treat it as a specification.

## Key constraints

- BACnet/SC is the primary transport; Tailscale BBMD is a fallback for when the cloud Hub is unreachable
- No automatic transport failover — prevents flapping during intermittent SC connectivity
- Hub mode requires TLS (static certs, Let's Encrypt ACME, or self-signed for testing)
- BTL compliance target: ~253 router-relevant tests from rusty-bacnet's harness (BVLC/BBMD §9.3, SC §9.9, NPDU routing §10.1–10.5)

## When Rust toolchain arrives

- Expected root `Cargo.toml` workspace, `crates/bridge-core/`, `src/` binary
- Tests: `cargo test` (unit), `cargo test --test *` (integration), Docker compose e2e
- Feature flags: `router` (default), `hub` (default), `serve` (default), `windows-tray` (off)
- Build: `cargo build --release --features windows-tray` for Windows

## Agent skills

### Issue tracker

GitHub Issues on `homoudachi/rust-bacnet-bridge`. See `docs/agents/issue-tracker.md`.

### Triage labels

Standard five-role vocabulary: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context repo: `CONTEXT.md` + `docs/adr/`. See `docs/agents/domain.md`.
