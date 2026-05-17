# Testing Windows EXE — Sandboxing Guide

`bacnet-bridge.exe` binds real network interfaces. Running it on a production LAN
can interfere with real BACnet devices. Use this guide to sandbox your testing.

## Sandboxing options (ordered by safety)

1. **Isolated machine** — Dedicated test PC with no production BACnet devices on
   its LAN segment. No risk of interference.

2. **VM (Hyper-V)** — Windows VM with a virtual switch that has no physical
   BACnet devices attached. The web dashboard at `localhost:28821` on the host
   is still accessible via VM port forwarding.

3. **Bind to loopback** — Set `router.lan.interface = "127.0.0.1"` in config.
   The router won't actually route BACnet traffic, but the dashboard and UI are
   fully functional for testing.

4. **Production LAN (caution)** — Only if you are certain no other BACnet
   devices share the same subnet. The bridge may respond to Who-Is broadcasts
   and could interfere with device discovery.

## Quick-test checklist

| Verify                       | Ignore in sandbox              |
|------------------------------|--------------------------------|
| Tray icon appears (green)    | No real BACnet devices found   |
| Dashboard loads at `:28821`  | FDT table is empty             |
| Config panel loads & saves   | Transport may not connect      |
| Transport switch UI responds | (no real hub / BBMD available) |

## Ports used

| Port  | Purpose                         |
|-------|---------------------------------|
| 47808 | BACnet/IP (LAN interface)       |
| 28821 | Web dashboard                   |
| 8443  | Embedded SC Hub (if `--with-hub`) |

## Troubleshooting common first-run issues

- **Tray panic on startup** — Fixed in PR #85. Update to latest build.
- **"Port already in use"** — Another BACnet app or a prior instance is still
  running. Check with `netstat -ano | findstr :47808`.
- **Dashboard not loading** — Check Windows Defender firewall. Try
  `http://127.0.0.1:28821` directly (bypass any DNS weirdness).
- **Windows Defender SmartScreen** — The EXE is unsigned. Click **More info**
  → **Run anyway**.
