# Dual Transport: BACnet/SC Primary, Tailscale Fallback

The BACnet Bridge supports two transports for connecting a remote service laptop to a site's local BACnet LAN: BACnet/SC (WebSocket+TLS via a cloud-hosted Hub) and BACnet/IP over Tailscale VPN (BBMD + Foreign Device). Only one transport is active at a time; switching is manual.

**Why BACnet/SC as primary**: Standardized (ASHRAE 135-2020), NAT-friendly via hub-and-spoke WebSocket, no third-party VPN dependency. Eliminates Tailscale's Windows client feature-gap issues.

**Why Tailscale as fallback**: The BACnet/SC Hub is a single point of failure. Tailscale VPN provides a peer-to-peer fallback that continues working even if the cloud Hub is unreachable. iComm's Foreign Device registration model maps cleanly to this transport.

**Why manual switch, not auto-failover**: Automatic transition between transports would cause the remote router to flap during intermittent SC connectivity. A manual switch ensures the operator is aware of the transport change and can diagnose the root cause.

**Consequence**: The routing core must be transport-agnostic, accepting a pluggable transport backend (SC Spoke or Tailscale BBMD) that is activated based on configuration.
