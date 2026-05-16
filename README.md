# BACnet Bridge

Dual-transport BACnet router bridging local BACnet/IP LAN to remote BACnet networks via BACnet/SC (primary) or Tailscale BBMD (fallback).

**Status:** ~99% complete, Phase 6 polish. See [docs/ROADMAP.md](docs/ROADMAP.md).

## Architecture

- Single binary `bacnet-bridge` with subcommands: `router`, `hub`, `serve`
- Core routing via [rusty-bacnet](https://github.com/jscott3201/rusty-bacnet) `BACnetRouter` with two ports (LAN + remote transport)
- Web dashboard: axum + HTMX + Tailwind CSS (embedded assets via rust-embed)
- Windows system tray: `tray-item` crate with green/amber/red DIB icons
- Config: TOML load/save, env-var overrides (`BACNET_BRIDGE__*`), auto-generation on first run
- Embedded Hub mode: `--with-hub` runs SC Hub + Router simultaneously
- 83 tests (unit + integration): config, FDT, routing, SC, BBMD, transport switch, dashboard API

## Quick start

```bash
# Run the router (auto-generates config on first launch)
cargo run -- router

# Run with embedded SC Hub
cargo run -- router --with-hub

# Docker e2e test topologies (SC, BBMD, BTL harness)
cd docker && docker compose -f compose-sc.yml up
```

## Docs

| File | Purpose |
|------|---------|
| [docs/FSD.md](docs/FSD.md) | Functional specification — the authoritative plan |
| [docs/ROADMAP.md](docs/ROADMAP.md) | Implementation sequence |
| [docs/adr/](docs/adr/) | Architecture decision records |
| [CONTEXT.md](CONTEXT.md) | Domain terminology |
| [AGENTS.md](AGENTS.md) | Instructions for AI coding agents |

## Domain language

Precise terms defined in [CONTEXT.md](CONTEXT.md):
- **Router** — BACnet NPDU forwarder (not IP router)
- **Hub** — BACnet/SC cloud relay
- **Spoke** — BACnet/SC node connected to a Hub
- **Transport** — BACnet/SC or BACnet/IP over Tailscale
- **BBMD** / **Foreign Device** — BACnet/IP broadcast relay roles

## License

MIT
