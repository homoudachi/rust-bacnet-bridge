# Session summary: BTL #63 encoding fix → BTL full pass → issue cleanup

## Progress

| # | Issue | Status |
|---|-------|--------|
| [#63](https://github.com/homoudachi/rust-bacnet-bridge/issues/63) | Debug ReadProperty ACK encoding | CLOSED |
| [#59](https://github.com/homoudachi/rust-bacnet-bridge/issues/59) | BTL §9/§10 failure | CLOSED |
| [#60](https://github.com/homoudachi/rust-bacnet-bridge/issues/60) | DEVICE_ID=100→99999 | CLOSED |
| [#61](https://github.com/homoudachi/rust-bacnet-bridge/issues/61) | PROP 97/139 handlers | CLOSED |
| [#62](https://github.com/homoudachi/rust-bacnet-bridge/issues/62) | Docker BTL verification | CLOSED |
| [#64](https://github.com/homoudachi/rust-bacnet-bridge/issues/64) | Optimize BTL feedback loop | Triaged, kept open |
| [#65](https://github.com/homoudachi/rust-bacnet-bridge/issues/65) | §2 compliance failures | NEW |
| [#66](https://github.com/homoudachi/rust-bacnet-bridge/issues/66) | §3 command prioritization | NEW |

## 6 bugs fixed

### Bug 1: Context tag wrong (root cause of #59)
`local_device.rs` — Property value wrapper used context tag `[2]` (`0x2E`/`0x2F`). BACnet ReadProperty-ACK requires context tag `[3]` (`0x3E`/`0x3F`).

### Bug 2: Service dispatch reading wrong byte
`local_device.rs` — Read `apdu[1]` for service choice. Fixed to read `apdu[3]` for non-segmented ConfirmedRequest.

### Bug 3: Response NPDU routing loop
`local_device.rs` — Response NPDU used `lan_transport.local_mac()` (loopback MAC) instead of `config.lan_mac` (BIP MAC). Fixed with explicit destination network=1.

### Bug 4: Default property value invalid
`local_device.rs` — Default handler returned `vec![0x5F]` (closing tag 5). Fixed to `vec![0x21, 0x00]` (unsigned 0).

### Bug 5: BIP transport not receiving packets
`router.rs` — LAN BIP transport created without BBMD enabled. Fixed with `lan_bip.enable_bbmd(vec![])`.

### Bug 6: Docker BIP binding issue
`docker-compose.btl-sc.yml` — LAN interface set to `0.0.0.0` which doesn't work in Docker for BIP. Fixed to `172.20.0.3`.

## BTL results

| Section | Scope | Tests | Result |
|---------|-------|-------|--------|
| 0 | Smoke | 3 | 3/3 ✓ |
| 2 | Compliance | 19 | 16/19 ✓ (→ #65) |
| 3 | Objects | 834 | 826/834 ✓ (→ #66) |
| 9 | DLL | 494 | **494/494** ✓ |
| 10 | Routing | 15 | **15/15** ✓ |
| **Local** | cargo | 88 | **88/88** ✓ |

## Git commits

```
0e6d552 docs: update ROADMAP and AGENTS for BTL #63 completion
e72406d fix: BTL #63 ReadProperty ACK encoding + NPDU routing + BBMD LAN enable
9f08d61 docs: add BTL sections 0, 2, 3 results to ROADMAP
```

## Open issues for next session

- [#65](https://github.com/homoudachi/rust-bacnet-bridge/issues/65) — BTL §2: 3 compliance failures (Object_Name empty, undocumented property, APDU timeout)
- [#66](https://github.com/homoudachi/rust-bacnet-bridge/issues/66) — BTL §3: 8 command prioritization failures (low priority — application-layer, not router/transport)
- [#64](https://github.com/homoudachi/rust-bacnet-bridge/issues/64) — Optimize BTL testing feedback loop (enhancement)

## Key files changed

```
crates/bridge-core/src/local_device.rs  — encoding fix + service dispatch + NPDU routing + default value + SubscribeCOV
crates/bridge-core/src/router.rs        — enable BBMD on LAN transport + resolved_lan_ip()
docker/docker-compose.btl-sc.yml        — DEVICE_ID=99999, LAN IP=172.20.0.3
AGENTS.md                               — current state updated
docs/ROADMAP.md                         — BTL results, Phase 6 complete
```
