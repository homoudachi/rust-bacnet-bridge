# BACnet Bridge

Dual-transport BACnet router bridging local BACnet/IP LAN to remote BACnet networks via BACnet/SC (primary) or Tailscale BBMD (fallback).

**Status:** Pre-implementation. See [docs/ROADMAP.md](docs/ROADMAP.md) for the plan.

## Architecture (planned)

- Single binary `bacnet-bridge` with subcommands: `router`, `hub`, `serve`
- Core routing via [rusty-bacnet](https://github.com/jscott3201/rusty-bacnet) `BACnetRouter` with two ports (LAN + remote transport)
- Web dashboard: axum + HTMX + Tailwind CSS
- Windows system tray: `tray-item` crate
- BTL compliance target: ~253 router-relevant tests from [rusty-bacnet-btl-harness](https://github.com/jscott3201/rusty-bacnet-btl-harness)

## Spike

`examples/spike-two-port-router/` proves the core routing pattern works:

```bash
cd examples/spike-two-port-router
cargo run
```

Validates Who-Is/I-Am broadcast cross-forwarding and ReadProperty unicast routing between two loopback transports.

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

Proprietary.
