#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_FILE="docker/docker-compose.btl-sc.yml"
SECTION=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --section)
            SECTION="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 --section {9|10}"
            exit 0
            ;;
        *)
            echo "ERROR: Unknown argument: $1"
            echo "Usage: $0 --section {9|10}"
            exit 1
            ;;
    esac
done

if [ -z "$SECTION" ]; then
    echo "ERROR: --section is required"
    echo "Usage: $0 --section {9|10}"
    exit 1
fi

if [ "$SECTION" != "9" ] && [ "$SECTION" != "10" ]; then
    echo "ERROR: Invalid section '${SECTION}'. Must be 9 or 10."
    exit 1
fi

echo "=== [1/2] Deploy binary ==="
"$SCRIPT_DIR/fast-deploy.sh"

echo ""
echo "=== [2/2] Run BTL section ${SECTION} ==="

RUNNER_SERVICE="btl-runner-routing"
if [ "$SECTION" = "9" ]; then
    RUNNER_SERVICE="btl-runner-sc"
fi

cd "$REPO_ROOT"

docker compose -f "$COMPOSE_FILE" \
    --profile "section-${SECTION}" \
    run --rm "$RUNNER_SERVICE"

EXIT_CODE=$?
echo ""
if [ $EXIT_CODE -eq 0 ]; then
    echo "BTL section ${SECTION} PASSED"
else
    echo "BTL section ${SECTION} FAILED (exit code: ${EXIT_CODE})"
fi
exit $EXIT_CODE
