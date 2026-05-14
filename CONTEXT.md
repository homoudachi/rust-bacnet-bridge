# BACnet Bridge

A dual-transport BACnet router that bridges a local BACnet/IP LAN to a remote BACnet network, enabling service laptops to discover and interact with on-site BACnet devices.

## Language

**Router (BACnet Router)**:
A device that forwards BACnet NPDUs between networks with different BACnet network numbers. Not an IP router.
_Avoid_: Bridge, gateway

**Hub (BACnet/SC Hub)**:
A BACnet/SC relay that forwards frames between connected spokes. Cloud-hosted, always available.
_Avoid_: Server, broker

**Spoke**:
A BACnet/SC node that maintains a persistent WebSocket connection to a Hub. Each Router acts as a spoke when using BACnet/SC transport.
_Avoid_: Client, node

**Transport**:
The underlying communication mechanism for BACnet messages. Two options — BACnet/SC (WebSocket+TLS to a Hub) or BACnet/IP (UDP over Tailscale VPN).
_Avoid_: Connection, link

**BBMD (Broadcast Management Device)**:
A BACnet/IP device that forwards broadcasts to registered Foreign Devices and peer BBMDs. Used only when the Transport is BACnet/IP (Tailscale fallback).
_Avoid_: Broadcast forwarder

**Foreign Device**:
A BACnet/IP device on a remote IP subnet that registers with a BBMD to receive broadcasts. iComm operates as a Foreign Device when using Tailscale transport.
_Avoid_: Remote device, FD

**Network Number**:
A BACnet network identifier (1-65535). Each BACnet network segment gets a unique number. The LAN is network 1; the SC spoke side is network 2.

**iComm**:
Innotech's proprietary BACnet client software. Connects via BACnet/IP UDP or Foreign Device registration. Does not speak BACnet/SC natively, requiring a Laptop Router to translate.
_Avoid_: Client tool, BACnet browser

**Site Router**:
The bridge instance running at a facility with physical BACnet devices on the local LAN. Acts as the permanent bridge.
_Avoid_: Server, local bridge

**Laptop Router**:
The bridge instance running on a service laptop alongside iComm. Acts as the transient bridge during service calls.
_Avoid_: Client router, remote bridge

## Relationships

- A **Router** bridges exactly one LAN network to exactly one remote **Transport** (SC or Tailscale), one at a time
- A **Hub** connects zero or more **Spokes**
- A **Spoke** connects to exactly one **Hub**
- A **Foreign Device** registers with exactly one **BBMD**
- A **Laptop Router** serves as the BACnet/SC-to-BACnet/IP translator for **iComm**

## Topology

```
                         BACnet/IP            BACnet/SC            BACnet/SC            BACnet/IP
iComm ──────────────────► Laptop Router ────► Hub ────► Site Router ────► LAN devices
(Foreign Device or UDP)  (Spoke)          (Cloud)    (Spoke)           (network 1)
```

Fallback mode (Tailscale):

```
                         BACnet/IP over Tailscale VPN             BACnet/IP
iComm ──────────────────► Site Router (BBMD) ────► LAN devices
(Foreign Device)                                     (same subnet)
```

## Example dialogue

> **Dev:** "When a service tech opens iComm on the laptop, does iComm connect directly to the Hub?"
> **Domain expert:** "No — iComm only speaks BACnet/IP. It connects to the Laptop Router via Foreign Device registration. The Laptop Router translates to BACnet/SC and connects to the Hub as a Spoke."

## Flagged ambiguities

- "bridge" was used to mean both Router (BACnet routing) and the overall product — resolved: the product is "BACnet Bridge", the component is Router.
- "remote device" was used to mean both the service laptop and the BACnet devices on the LAN — resolved: Laptop Router and LAN devices are distinct.
