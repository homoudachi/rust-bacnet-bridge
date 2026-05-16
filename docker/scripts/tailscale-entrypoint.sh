#!/bin/sh
set -e

CLEANUP_DONE=0
cleanup() {
    [ "$CLEANUP_DONE" = "1" ] && return
    CLEANUP_DONE=1
    echo "Shutting down Tailscale..."
    tailscale down
    exit 0
}
trap cleanup TERM INT

tailscaled --tun=userspace-networking --statedir=/var/lib/tailscale &
sleep 2

echo "Authenticating with Tailscale..."
tailscale up --authkey="${TAILSCALE_AUTH_KEY}" --hostname="${TAILSCALE_HOSTNAME:-bacnet-bridge}" --accept-routes

echo "Waiting for Tailscale IP..."
IP=""
for i in $(seq 1 30); do
    IP=$(tailscale ip -4 2>/dev/null || true)
    [ -n "$IP" ] && break
    sleep 1
done

if [ -z "$IP" ]; then
    echo "ERROR: Failed to get Tailscale IP after 30 seconds" >&2
    tailscale down
    exit 1
fi

echo "Tailscale IP: $IP"
export BACNET_BRIDGE_ROUTER__TAILSCALE__INTERFACE="${IP}"
export BACNET_BRIDGE_ROUTER__LAN__INTERFACE="${IP}"

echo "Starting: $*"
"$@" &
PID=$!
wait $PID || true
EXIT_CODE=$?

tailscale down
exit $EXIT_CODE
