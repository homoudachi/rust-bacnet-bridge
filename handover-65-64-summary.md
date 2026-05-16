# Session summary: Issues #65 + #64 — subagent-driven development trial

## Approach

Subagent-driven development with sandbox git branches. Each issue on its own branch, two-stage review (spec then quality), controller merges.

## Completed

### [#65](https://github.com/homoudachi/rust-bacnet-bridge/issues/65) — BTL §2 compliance fixes
- **Branch:** `fix/65-section2-compliance` (commit `d6d4eaa`)
- **File:** `crates/bridge-core/src/local_device.rs` (+141/-10)
- **Bug 1:** Object_Name empty → fallback to "BACnet-Bridge" when config string empty
- **Bug 2:** Unknown property → Error PDU (PROPERTY/UNKNOWN_PROPERTY) using library `encode_apdu`
- **Bug 3:** APDU retry → `HashMap<u8, Vec<u8>>` cache of NPDU bytes by invoke_id
- **Tests:** 91/91 passing (3 new), clean clippy
- **BTL §2 verification:** 2.1.12 (Object_Name) **fixed** ✅. 2.1.16 and 2.1.22 still fail — see below.

### [#64](https://github.com/homoudachi/rust-bacnet-bridge/issues/64) — Fast BTL feedback loop
- **Branch:** `feat/64-feedback-loop` (commits `c80f295`, `d1dd3d7`)
- **Files:** `docker/scripts/fast-deploy.sh`, `docker/scripts/fast-btl.sh`, `AGENTS.md`
- `fast-deploy.sh` — builds musl binary, `docker cp` into running container, restarts, waits for healthcheck
- `fast-btl.sh` — wraps deploy + BTL run for `--section 9|10`
- Both spec-reviewed (compliant) and code-quality-reviewed (issues fixed: healthcheck poll, arg guard, symlink-safe paths, -x check)
- **Prerequisite:** `rustup target add x86_64-unknown-linux-musl && apt-get install musl-tools`

## BTL §2 results with #65 fixes

```
20 tests total (3 smoke + 17 section 2)
Before: 16/19 §2 tests passing
After:  17/20 total passed, 2 failed, 1 error
```

| Test | Status | Note |
|------|--------|------|
| 2.1.12 Object_Name | ✅ Pass (was Fail) | **Bug 1 fixed** |
| 2.1.15 Read-Only Property | ❌ Error (timeout 30s) | Regression: device doesn't handle WriteProperty. Needs WriteProperty error handler. |
| 2.1.16 Unknown Property 9999 | ❌ Fail | Error PDU encoding verified correct (7 bytes: `[50,2A,0C,91,02,91,20]`). BTL receives 15 bytes — suggests router forwarding request to btl-server reference device instead of delivering to local device. **Routing delivery bug.** |
| 2.1.22 APDU Timeout | ❌ Fail | Reads property 11 (APDU_TIMEOUT) — not in `known_props` list. Must add prop 11 with valid unsigned value. |

### Root causes of remaining failures

1. **Router delivery:** ReadProperty requests may be forwarded to the BTL reference server (172.20.0.4) instead of delivered to our loopback local device (network 0, MAC [0x01,0x02]). The router receives on LAN port (network 1) with destination MAC matching LAN port — may deliver to LAN handler or forward incorrectly. Needs investigation of `BACnetRouter` delivery logic for local devices.

2. **Missing property:** APDU_TIMEOUT (prop 11) is a required Device object property. Must be added to `known_props` with a reasonable unsigned default (e.g., 3000ms = `vec![0x22, 0x0B, 0xB8]`).

3. **Missing WriteProperty handler:** Test 2.1.15 sends WriteProperty to read-only property (e.g., Object_Identifier). Device ignores it (no handler in match), BTL times out after 30s. Need to add a `(0x00, 0x0F)` handler that returns Error PDU (WRITE_ACCESS_DENIED).

## Open branches

| Branch | Contents | Status |
|--------|----------|--------|
| `fix/65-section2-compliance` | 3 BTL §2 encoding fixes | Ready to merge (encoding correct, delivery bug separate) |
| `feat/64-feedback-loop` | Fast deploy + BTL scripts | Ready to merge |

## Recommended follow-ups

- **New issue:** Router delivery bug — ReadProperty not reaching local device
- **New issue:** Add remaining required properties to local device (APDU_TIMEOUT, WriteProperty handler)
- **Merge strategy:** Merge #65 and #64 to master; open new issues for routing + property gaps

## Lessons: subagent-driven development

- **Sandboxing with git branches:** Works perfectly. No cross-contamination between #65 and #64.
- **Two-stage review caught real bugs:** Code quality review found missing healthcheck poll, script crash on missing `--section` argument.
- **Encoding bugs:** Providing exact expected byte sequences in the prompt helped the subagent succeed. The previous session's bugs (deepseek failing on byte-level encoding) were likely due to model pattern-matching instead of literal byte tracing.
- **BTL verification:** Docker build is the slowest part (~5 min). Fast-deploy script would reduce this to ~30s when musl-tools is installed.
- **Model choice:** For encoding tasks, prefer literal/models that trace bytes over fast pattern-matchers. The general-purpose agent succeeded here because the prompt specified exact byte expectations.

## Build progress tracking

Added conversation about build progress bars. Options:
- `cargo build --timings` — HTML report after build
- Simple wrapper script counting Compiling lines vs total crate count
- Consider adding to fast-deploy.sh for feedback during long builds
