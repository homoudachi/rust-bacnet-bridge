# Handover: Issue #59 — BTL (9,10) Failures

## Current CI Status (run #25950615615)

| Job | Result |
|-----|--------|
| Format / Test / Clippy / Build-Win / Test-Win | PASS |
| Docker E2E (SC) | **PASS** (previously passed) |
| Docker E2E (BBMD) | **PASS** (was failing — fixed) |
| BTL (9) — SC data link | **HANGING** (still in_progress) |
| BTL (10) — Routing | **FAIL** — 0/15 tests passed |

BBMD + SC E2E are green. Only BTL remains.

## BTL Root Cause (from artifact `btl-results-section-10.log`)

All 15 tests fail at step 1 with the **same error**:
```
ReadProperty failed: request timed out after 3s
```
Querying `DEVICE,99999` for `PROTOCOL_SERVICES_SUPPORTED` (PROP 97).

The BTL runner at `/tmp/btl10/btl-results-section-10.log` shows the IUT address is correctly discovered:
```json
"address": "[172, 20, 0, 3, 186, 192]"
```
So BIP connectivity works — the problem is at the BACnet application layer.

### Two Sub-Issues

**1. Device instance mismatch (easy fix)**
- BTL runner queries `DEVICE,99999`
- Our site-router is `DEVICE,100` (set via `BACNET_BRIDGE_ROUTER__DEVICE_ID=100`)
- Fix: change `DEVICE_ID=100` → `DEVICE_ID=99999` in `docker/docker-compose.btl-sc.yml`

**2. Missing BACnet property handlers (main work)**
`crates/bridge-core/src/local_device.rs:150-184` only handles 6 properties:
| PROP ID | Name | Handled? |
|---------|------|----------|
| 75 | OBJECT_NAME | Yes |
| 76 | VENDOR_IDENTIFIER | Yes |
| 85 | PROTOCOL_VERSION | Yes |
| 12 | APDU_SEGMENT_TIMEOUT | Yes |
| 13 | APDU_TIMEOUT | Yes |
| 512 | TRANSPORT_MODE (custom) | Yes |
| **97** | **PROTOCOL_SERVICES_SUPPORTED** | **NO** |
| **139** | **PROTOCOL_REVISION** | **NO** |

The BTL runner sends ReadProperty for PROP 97 (every test's first step) and PROP 139 (section 10.9 tests). Our router doesn't respond to either → timeout → all tests fail.

## Files to Modify

### Primary
- `crates/bridge-core/src/local_device.rs` — add ReadProperty cases for:
  - PROP 97 (PROTOCOL_SERVICES_SUPPORTED) — return a BitString of supported services
  - PROP 139 (PROTOCOL_REVISION) — return unsigned 24
  - Any other properties the BTL runner queries for BBMD/SC hub tests (check artifact after fixing the above)

- `docker/docker-compose.btl-sc.yml` — line 38: change `DEVICE_ID=100` → `DEVICE_ID=99999`

### Secondary (if needed)
- `crates/bridge-core/src/config.rs:318` — `"bdt" => {}` env-var override is a no-op (not blocking current tests but worth fixing)

## Local Testing First — DO NOT Push to CI Yet

Use Docker compose locally to verify fixes before pushing:

```bash
# 1. Build all images (one-time)
docker compose -f docker/docker-compose.btl-sc.yml build

# 2. Run section 10 (routing) — watch for the runner's exit code
docker compose -f docker/docker-compose.btl-sc.yml \
  --profile section-10 \
  up --abort-on-container-exit --exit-code-from btl-runner-routing

# 3. Check runner output
docker compose -f docker/docker-compose.btl-sc.yml logs btl-runner-routing

# 4. If stuck/hanging (test 9), stop and inspect
docker compose -f docker/docker-compose.btl-sc.yml down

# 5. After fixing, run section 9 too
docker compose -f docker/docker-compose.btl-sc.yml \
  --profile section-9 \
  up --abort-on-container-exit --exit-code-from btl-runner-sc
```

### Quick Test After Each Local Change
```bash
# Rebuild only the router (fast — layered cache)
docker compose -f docker/docker-compose.btl-sc.yml build site-router

# Re-run the target section
docker compose -f docker/docker-compose.btl-sc.yml \
  --profile section-10 \
  up --abort-on-container-exit --exit-code-from btl-runner-routing
```

### Verify Rust Before Docker
```bash
# Always run unit/integration tests first — fast feedback
cargo test --all-targets
```

## MCP Debugging Sidecar

A `rusty-bacnet-mcp` debugging agent is pre-configured in `docker/config/mcp.json`. It acts as a BACnet device on the same BIP network, useful for sending manual Who-Is / ReadProperty probes.

```bash
# Build the MCP sidecar
docker build -f docker/Dockerfile.mcp -t bacnet-mcp .

# Run alongside the BTL topology (add to compose or run separately)
docker run --rm --network docker_default bacnet-mcp
```

The MCP config (`docker/config/mcp.json`):
- Device instance: 389999
- BIP: 0.0.0.0:47808, broadcast 172.20.0.255, network 1

Use it to verify the router responds to ReadProperty for PROP 97/139 by sending manual queries after the router starts. If the router responds to MCP probes but not BTL runner probes, the issue is in the test harness config. If it doesn't respond to either, the property handler is broken.

## Subagent Strategy

Use subagents for parallelism — split work so each agent handles one piece:

1. **Agent 1**: Modify `local_device.rs` — add PROP 97/139 handlers + write/run unit tests
2. **Agent 2**: Modify `docker/docker-compose.btl-sc.yml` — fix device ID + test locally
3. **Agent 3** (after Agent 1 completes): Build Docker, run compose, verify BTL runner logs show tests passing

### Subagent Template
```
Work on fixing BTL test failures for issue #59 in homoudachi/rust-bacnet-bridge.

Task: [specific task]

Context:
- BTL runner sends ReadProperty for DEVICE,99999 PROP 97 (PROTOCOL_SERVICES_SUPPORTED)
- Our site-router is DEVICE,100 and doesn't handle PROP 97
- Device instance mismatch + missing property handler

File to modify: [file path]
Validation: cargo test --all-targets, then docker compose up to verify

See /home/matt/opencode/rust-bacnet-bridge/handover-btl-59-local-testing.md for full context.
```

## Known Dependencies

### rusty-bacnet#22 (BIP broadcast receive) — WORKAROUND APPLIED
- BIP bound to specific IP drops inbound broadcasts at kernel level
- Fix in `router.rs`: bind LAN BIP to `0.0.0.0` (INADDR_ANY), compute MAC from config IP
- Upstream PR: https://github.com/jscott3201/rusty-bacnet/pull/21
- Once upstream merges, our workaround can stay or be reverted — it's harmless

### BTL harness hostname limitation — FIXED
- `jscott3201/rusty-bacnet-btl-harness` `--target` only accepts bare IPv4
- Fixed in `docker/docker-compose.btl-sc.yml`: `site-router:47808` → `172.20.0.3:47808`
- Pinned to commit `ad874b0` in `docker/Dockerfile.btl`

## What Was Already Done This Session

- **git history**: Expunged legacy Python prototype with real IPs (`git filter-repo`, force-pushed)
- **.gitignore**: Added `*.pem`, `*.key`, `.env`, `credentials.*`
- **LICENSE**: MIT license file created
- **Docs**: README, AGENTS.md, ROADMAP.md updated to current state
- **BBMD fix**: Compute BIP MAC from IP+port, bind to INADDR_ANY
- **BTL compose**: Hostname→IP fix applied
- **Issue #59**: Updated with full diagnosis, label → `ready-for-human`
