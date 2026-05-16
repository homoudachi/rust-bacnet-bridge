#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_FILE="docker/docker-compose.btl-sc.yml"
TARGET="x86_64-unknown-linux-musl"

cd "$REPO_ROOT"
SECONDS=0

echo "=== Fast deploy: build + hot-reload bacnet-bridge ==="

# 1. Verify compose project is running with site-router
if ! docker compose -f "$COMPOSE_FILE" ps --services --filter "status=running" 2>/dev/null | grep -q "site-router"; then
    echo "ERROR: site-router container is not running."
    echo ""
    echo "Start the BTL topology first:"
    echo "  docker compose -f ${COMPOSE_FILE} up --profile section-9 -d"
    exit 1
fi

CONTAINER_ID=$(docker compose -f "$COMPOSE_FILE" ps -q site-router)
echo "site-router container: ${CONTAINER_ID:0:12}..."

# 2. Verify musl target is available
if ! rustup target list --installed 2>/dev/null | grep -q "$TARGET"; then
    echo "ERROR: Rust musl target '${TARGET}' is not installed."
    echo ""
    echo "Install it with:"
    echo "  rustup target add ${TARGET}"
    echo ""
    echo "You may also need musl development tools:"
    echo "  # Debian/Ubuntu: sudo apt-get install musl-tools"
    echo "  # Fedora:        sudo dnf install musl-devel"
    exit 1
fi

# 3. Build
echo "Building (target: ${TARGET})..."
cargo build --release --target "$TARGET" --no-default-features --features router,hub

BINARY="target/${TARGET}/release/bacnet-bridge"
if [ ! -x "$BINARY" ]; then
    echo "ERROR: Binary not found or not executable at ${BINARY}"
    exit 1
fi

# 4. Copy binary into running container
echo "Copying binary into container..."
docker cp "$BINARY" "${CONTAINER_ID}:/usr/local/bin/bacnet-bridge"

# 5. Restart site-router
echo "Restarting site-router..."
docker compose -f "$COMPOSE_FILE" restart site-router

# 6. Wait for healthcheck to pass (start_period=10s + retries)
echo "Waiting for site-router healthcheck..."
for i in $(seq 1 30); do
    status=$(docker inspect --format='{{.State.Health.Status}}' "$CONTAINER_ID" 2>/dev/null || echo "starting")
    if [ "$status" = "healthy" ]; then
        break
    fi
    sleep 1
done
if [ "$status" != "healthy" ]; then
    echo "WARNING: site-router healthcheck did not pass (status: ${status:-unknown})"
fi

echo "Done (${SECONDS}s)"
