# Session summary: BTL #59 fix

## Progress

| # | Issue | Status |
|---|-------|--------|
| [#59](https://github.com/homoudachi/rust-bacnet-bridge/issues/59) | Parent: BTL (9,10) failures | Diagnosed, 3 children created |
| [#60](https://github.com/homoudachi/rust-bacnet-bridge/issues/60) | Device ID 100→99999 in compose | ✅ Done |
| [#61](https://github.com/homoudachi/rust-bacnet-bridge/issues/61) | PROP 97/139 handlers | ✅ Code written, BTL test FAILS |
| [#62](https://github.com/homoudachi/rust-bacnet-bridge/issues/62) | Docker BTL verification | ❌ Not verified |

## Uncommitted changes (3 files)

### docker/docker-compose.btl-sc.yml
- `DEVICE_ID=100` → `DEVICE_ID=99999` (line 38)

### crates/bridge-core/src/local_device.rs (3 iterations over session)
**Final encoding (still failing BTL):**
- Uses `encode_ctx_object_id(0, &oid)` for object identifier
- Uses `encode_ctx_unsigned(1, prop_id)` for property identifier
- `0x2E` (ctx_open(2)) before value, `0x2F` (ctx_close(2)) after
- PROP 97: BitString `[0x85, 5, 5, 0x84, 0x0B, 0x00, 0x20]`
- PROP 139: unsigned `[0x21, 0x18]`
- 83 tests pass locally

### crates/bridge-core/src/router.rs (from over-scoped subagent, needs review)
- Added `resolve_local_ip()` to detect actual LAN IP
- Changed MAC computation to use resolved IP

## BTL test results

**Latest run: all 15 tests FAIL, same error:**
```
ReadProperty failed: decoding error at offset 8: missing closing tag 3
```
This is the result with the fully-rewritten encoding (library functions + ctx_open/close). The encoding rewrite didn't change the error.

**Possible remaining issues:**
1. Docker image wasn't rebuilt with latest code (btl-test-site-router built at 17:04, our code changes were at 17:50+)
2. The BACnet encoding is still wrong at some other level
3. The BTL runner expects a different SEQUENCE structure than what we produce

**Unused debugging tool:** `docker/Dockerfile.mcp` + `docker/config/mcp.json` — a BACnet MCP sidecar (device 389999) that could send manual ReadProperty probes and capture raw response bytes for comparison.

## Process failures this session

1. **Main context bloat** — I spent most of the session analyzing BACnet encoding byte-by-byte in the main context instead of delegating to subagents for experimentation. Multiple 30k+ context messages going in circles.

2. **Subagent over-scoping** — The second implementer subagent rewrote request parsing, changed router.rs, and broke tests. Causes: unclear boundaries, too much context in prompt, no `don't change X` guardrails.

3. **Slow feedback loop** — Docker rebuild takes ~5 min per iteration (Rust compile from scratch in Docker). `cargo test` passes instantly but BTL validation requires Docker. No incremental approach was attempted (volume mounts, pre-built binary).

4. **No MCP usage** — The MCP debugging sidecar was never deployed. It could have given us raw response bytes without Docker rebuilds.

5. **TODOs not maintained** — I didn't update the todo list after each subagent finished, making progress tracking fuzzy.

6. **Spec reviews unreliable** — One spec reviewer got confused by seeing the full diff across both tasks. Code quality review approved broken encoding.

## New issues created

| # | Issue |
|---|-------|
| [#63](https://github.com/homoudachi/rust-bacnet-bridge/issues/63) | Debug ReadProperty ACK encoding with MCP sidecar |
| [#64](https://github.com/homoudachi/rust-bacnet-bridge/issues/64) | Optimize BTL testing feedback loop (avoid full Docker rebuilds) |

## Next session plan

1. Load `diagnose` skill, use MCP sidecar (#63) to capture raw router response bytes
2. Compare against known-good BACnet ReadProperty-ACK structure
3. Fix encoding, verify with MCP (fast feedback loop)
4. Rebuild Docker once encoding confirmed, run BTL section 10 + 9
5. Revert unnecessary router.rs changes from over-scoped subagent
6. Implement Docker feedback loop optimization (#64)
7. Commit and push

## Orchestration & Context Management Protocol (to apply next session)

- Break every task into minimal TODOs; one `in_progress` at a time
- Delegate ALL implementation to subagents with hyper-focused prompts
- Subagent prompts must have: single objective, explicit boundaries (what NOT to change), no global context dump
- Never deep-dive encoding/analysis in main context — delegate to subagent
- After each subagent: update TODO, brief result, move to next
- Fast feedback preferred over comprehensive analysis
