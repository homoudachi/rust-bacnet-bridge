#!/usr/bin/env bash
set -euo pipefail

# Count total crates (cached first run per session)
get_total() {
    cargo metadata --format-version=1 --no-deps 2>/dev/null \
        | jq '.packages | length' 2>/dev/null || echo "?"
}

TOTAL=$(get_total)
COUNT=0
START=$(date +%s)

cargo build --release "$@" 2>&1 | while IFS= read -r line; do
    if [[ "$line" == *"Compiling"* ]]; then
        ((COUNT++))
        ELAPSED=$(($(date +%s) - START))
        printf "\r  [%3d/%s] %s" "$COUNT" "$TOTAL" "${line:0:55}"
    fi
done
echo ""
echo "Build complete in $(($(date +%s) - START))s"
