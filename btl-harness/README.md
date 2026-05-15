# BTL Compliance Test Harness

This directory contains integration test infrastructure for running BACnet Testing
Laboratory (BTL) compliance tests against `bacnet-bridge`.

## Overview

The [rusty-bacnet-btl-harness](https://github.com/jscott3201/rusty-bacnet-btl-harness)
provides a `bacnet-test` CLI tool that:
- **serves** as a reference BTL-compliant BACnet device
- **runs** test suites against a target BACnet device/router

Target tests (~253 router-relevant):
| Section | Description | Count |
|---------|-------------|-------|
| §9.3    | BVLC/BBMD   | 72    |
| §9.9    | SC          | 100   |
| §10.1–5 | NPDU routing| 81    |

## Running tests locally

All tests run via Docker Compose. No local Rust toolchain needed.

```bash
# Build all images
docker compose -f docker/docker-compose.btl-sc.yml build

# Run all SC tests (§9)
docker compose -f docker/docker-compose.btl-sc.yml up --profile section-9

# Run all routing tests (§10)
docker compose -f docker/docker-compose.btl-sc.yml up --profile section-10
```

To run both sections one after another:
```bash
for section in 9 10; do
  docker compose -f docker/docker-compose.btl-sc.yml up --profile "section-$section"
done
```

## Test topology

```
btl-runner ──BIP── site-router ──SC── sc-hub
                         │
                    btl-server (BTL reference device)
```

1. **sc-hub** — BACnet/SC cloud relay (Dockerfile.hub)
2. **site-router** — `bacnet-bridge router` configured with SC transport
3. **btl-server** — BTL reference device connected to the same BACnet/IP LAN
4. **btl-runner** — sends test messages to the site-router and validates responses

The site-router forwards BACnet traffic between the SC hub side (network 2)
and the LAN side (network 1). The BTL tests exercise NPDU routing, BVLC, and
SC transport behavior through this topology.

## Interpreting results

The `bacnet-test run` command outputs JSON results to stdout. Each test
reports:
- `test_id` — BTL test case identifier
- `name` — human-readable test description
- `status` — `pass`, `fail`, or `skip`
- `details` — failure reason if applicable

A passing test suite should show `"status": "pass"` for all tests. Common
failure modes:
- **Connection refused**: site-router not ready; retry after services stabilize
- **Unexpected response**: routing or SC transport configuration mismatch
- **Timeout**: BACnet/IP packets not reaching the target

## Known limitations

- **Dependency ordering**: `depends_on` in Docker Compose guarantees container
  start order but not service readiness. The `btl-runner` may start before the
  site-router has fully initialized. The `bacnet-test` tool has built-in retry
  with exponential backoff, but in rare cases a manual re-run may be needed.
- **Transport switching**: Only SC transport is tested here. BBMD fallback
  testing via BTL is not yet configured (would require a separate topology
  with the `site-router` configured for `tailscale` transport).
- **BTL harness version**: The Dockerfile clones `master` of the BTL harness
  repo. Pin to a specific commit for reproducible builds.
- **Section coverage**: Section §9.3 (BBMD) may have test count variance
  depending on the BTL harness version. Section §9.9 (SC) tests cover SC
  transport behavior through the router.

## CI

The BTL tests run as a separate CI job (`btl-tests`) in `.github/workflows/ci.yml`,
split into two matrix entries (section 9 and section 10). Results are uploaded
as build artifacts (`btl-results-section-*.log`).
