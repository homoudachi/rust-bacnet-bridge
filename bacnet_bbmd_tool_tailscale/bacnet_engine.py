import logging
import threading
import asyncio
from bacpypes.debugging import bacpypes_debugging, ModuleLogger
from bacpypes.consolelogging import ConfigArgumentParser
from bacpypes.core import run, stop, deferred
from bacpypes.comm import bind
from bacpypes.pdu import Address, LocalBroadcast, PDU
from bacpypes.bvll import ForwardedNPDU, OriginalBroadcastNPDU, DistributeBroadcastToNetwork, OriginalUnicastNPDU
from bacpypes.netservice import NetworkServiceElement, NetworkServiceAccessPoint, NetworkAdapter
from bacpypes.bvllservice import BIPSimple, BIPBBMD, AnnexJCodec, UDPDirector
from bacpypes.local.device import LocalDeviceObject

_debug = 0
_log = ModuleLogger(globals())

import socket

class SmartUDPDirector(UDPDirector):
    """A version of UDPDirector that ensures pduSource is always an Address object."""
    def __init__(self, *args, label="Unknown", **kwargs):
        self.label = label
        super().__init__(*args, **kwargs)

    def create_socket(self, family, type):
        super().create_socket(family, type)
        # Enable reuse address to avoid WinError 10048 on rapid restarts
        self.socket.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)

    def _response(self, pdu):
        # Ensure pduSource is a proper Address with IP info.
        # UDP recvfrom gives a tuple (ip, port); Address.decode_address()
        # does NOT handle tuples. Build a string "ip:port" instead.
        if isinstance(pdu.pduSource, tuple):
            try:
                pdu.pduSource = Address(f"{pdu.pduSource[0]}:{pdu.pduSource[1]}")
            except Exception as e:
                _log.error(f"Address conversion failed from tuple: {e}")
        elif isinstance(pdu.pduSource, (bytes, bytearray)):
            try:
                pdu.pduSource = Address(pdu.pduSource)
            except Exception as e:
                _log.error(f"Address conversion failed from bytes: {e}")
        
        # Debug logging for incoming traffic
        _log.info(f"[{self.label}] Packet from {pdu.pduSource} ({len(pdu.pduData)} bytes)")
        
        super()._response(pdu)

    def indication(self, pdu):
        # Ensure destination is a raw (ip, port) tuple for socket.sendto().
        # BIPBBMD stores fdAddress as an Address object; sendto() needs a tuple.
        if pdu.pduDestination:
            if isinstance(pdu.pduDestination, Address):
                if hasattr(pdu.pduDestination, 'addrTuple') and pdu.pduDestination.addrTuple:
                    pdu.pduDestination = pdu.pduDestination.addrTuple
            elif not isinstance(pdu.pduDestination, tuple):
                try:
                    addr = Address(pdu.pduDestination)
                    if hasattr(addr, 'addrTuple') and addr.addrTuple:
                        pdu.pduDestination = addr.addrTuple
                    else:
                        pdu.pduDestination = addr
                except Exception:
                    pass
        
        # Debug logging for outgoing traffic
        dest = pdu.pduDestination
        _log.info(f"[{self.label}] Sending packet to {dest} ({len(pdu.pduData)} bytes)")
        super().indication(pdu)

class BBMDRouter:
    def __init__(self, vpn_ip, lan_ip, port=20000, device_id=999):
        self.vpn_ip = vpn_ip
        self.lan_ip = lan_ip
        self.port = port
        self.device_id = device_id
        self.running = False
        self._thread = None
        
        # Logging setup
        self.logger = logging.getLogger("bacnet_engine")
        
    def start(self):
        if self.running:
            return
        self.running = True
        self._thread = threading.Thread(target=self._run_stack, daemon=True)
        self._thread.start()
        self.logger.info(f"BACnet Engine started: VPN={self.vpn_ip}, LAN={self.lan_ip}, Port={self.port}")

    def stop(self):
        if not self.running:
            return
        self.running = False
        deferred(stop)
        if self._thread:
            self._thread.join(timeout=2)
        self.logger.info("BACnet Engine stopped")

    def _run_stack(self):
        try:
            # 1. Define Local Device (Optional but good for identity)
            local_device = LocalDeviceObject(
                objectName="BBMD-Bridge",
                objectIdentifier=("device", self.device_id),
                maxApduLengthAccepted=1024,
                segmentationSupported="segmentedBoth",
                vendorIdentifier=15,
            )

            # 2. Setup the NSAP (Network Service Access Point)
            nsap = NetworkServiceAccessPoint()
            nse = NetworkServiceElement()
            bind(nse, nsap)

            # 3. Create the VPN Stack (BBMD enabled)
            vpn_address = Address(f"{self.vpn_ip}:{self.port}")
            vpn_director = SmartUDPDirector(vpn_address.addrTuple, label="VPN")
            vpn_codec = AnnexJCodec()
            vpn_bip = BIPBBMD(vpn_address)
            
            # Monkey-patch to add registration logging
            _orig_reg = vpn_bip.register_foreign_device
            def _logged_reg(addr, ttl):
                _log.info(f"!!! REGISTERING FOREIGN DEVICE: {addr} (TTL: {ttl})")
                return _orig_reg(addr, ttl)
            vpn_bip.register_foreign_device = _logged_reg
            
            self.vpn_bip = vpn_bip  # Store for FDT access
            
            # Link BIP to Codec to UDP
            bind(vpn_bip, vpn_codec, vpn_director)
            
            # Create Adapter 1 (VPN)
            vpn_adapter = NetworkAdapter(nsap, 1, vpn_address)
            nsap.adapters[1] = vpn_adapter
            bind(vpn_adapter, vpn_bip)

            # 4. Create the LAN Stack (Simple)
            lan_address = Address(f"{self.lan_ip}:47808") # Default LAN port
            lan_director = SmartUDPDirector(lan_address.addrTuple, label="LAN")
            lan_codec = AnnexJCodec()
            lan_bip = BIPSimple(lan_address)
            
            # Link BIP to Codec to UDP
            bind(lan_bip, lan_codec, lan_director)
            
            # Create Adapter 2 (LAN)
            lan_adapter = NetworkAdapter(nsap, 2, lan_address)
            nsap.adapters[2] = lan_adapter
            bind(lan_adapter, lan_bip)
            
            # IMPORTANT: For a pure router, we don't bind an Application() 
            # unless we want the router itself to respond to ReadProperty.
            # If you were getting 'pduExpectingReply' errors, it's likely 
            # because a service element was expecting an APDU but got a raw PDU.

            # --- Cross-adapter broadcast forwarding ---
            # The NSAP.process_npdu() only forwards when npduDADR points to
            # another network.  Local broadcasts (Who-Is / I-Am) have no
            # DADR, so forwardMessage stays False and they are silently
            # dropped.  Bypass the NSAP routing entirely by hooking each
            # BIP's confirmation() to also inject the broadcast into the
            # opposite adapter's indication() path.
            _orig_lan_confirm = lan_bip.confirmation
            def _lan_forward(pdu):
                _log.info(f"[CONFIRM-LAN] type={type(pdu).__name__} src={pdu.pduSource} ({len(pdu.pduData)} bytes)")
                if isinstance(pdu, (OriginalBroadcastNPDU, ForwardedNPDU, OriginalUnicastNPDU, DistributeBroadcastToNetwork)):
                    _log.info(f"[FWD-LAN→VPN] type={type(pdu).__name__} src={pdu.pduSource} ({len(pdu.pduData)} bytes)")
                    # Build a raw PDU carrying the NPDU+APDU data
                    raw = PDU(pdu.pduData, source=pdu.pduSource,
                               destination=LocalBroadcast(),
                               user_data=pdu.pduUserData)
                    # Forward to each foreign device with the LAN device
                    # as the BVLC originating address (NOT the BBMD).
                    for fdte in vpn_bip.bbmdFDT:
                        fxpdu = ForwardedNPDU(pdu.pduSource, raw,
                                              user_data=pdu.pduUserData)
                        fxpdu.pduDestination = fdte.fdAddress
                        vpn_bip.request(fxpdu)
                return _orig_lan_confirm(pdu)
            lan_bip.confirmation = _lan_forward

            _orig_vpn_confirm = vpn_bip.confirmation
            def _vpn_forward(pdu):
                _log.info(f"[CONFIRM-VPN] type={type(pdu).__name__} src={pdu.pduSource} ({len(pdu.pduData)} bytes)")
                if isinstance(pdu, (OriginalBroadcastNPDU, DistributeBroadcastToNetwork)):
                    _log.info(f"[FWD-VPN→LAN] type={type(pdu).__name__} src={pdu.pduSource} ({len(pdu.pduData)} bytes)")
                    xpdu = PDU(pdu.pduData, source=pdu.pduSource,
                               destination=LocalBroadcast(),
                               user_data=pdu.pduUserData)
                    lan_bip.indication(xpdu)
                return _orig_vpn_confirm(pdu)
            vpn_bip.confirmation = _vpn_forward

            # 5. Start the bacpypes core
            run()
            
        except Exception as e:
            self.logger.error(f"Error in BACnet stack: {e}", exc_info=True)
            self.running = False

    def get_fdt(self):
        """Returns the current Foreign Device Table if available."""
        try:
            if hasattr(self, 'vpn_bip'):
                fdt = []
                # In this version of bacpypes, FDT is a list called bbmdFDT
                for entry in self.vpn_bip.bbmdFDT:
                    fdt.append({
                        'address': str(entry.fdAddress),
                        'ttl': entry.fdTTL,
                        'remaining': entry.fdRemain
                    })
                return fdt
        except Exception as e:
            _log.error(f"Error reading FDT: {e}")
        return []

if __name__ == "__main__":
    # Test stub
    logging.basicConfig(level=logging.DEBUG)
