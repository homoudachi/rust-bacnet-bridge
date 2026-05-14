# Windows Firewall Note:
# To allow inbound UDP traffic for BACnet (default port 20000), run this in an Admin PowerShell:
# New-NetFirewallRule -DisplayName "BACnet BBMD Port" -Direction Inbound -Action Allow -Protocol UDP -LocalPort 20000

import sys
import os
import pkgutil
import importlib.util

# Python 3.14 compatibility: restore removed pkgutil.find_loader
if not hasattr(pkgutil, 'find_loader'):
    def find_loader(name):
        spec = importlib.util.find_spec(name)
        return spec.loader if spec else None
    pkgutil.find_loader = find_loader

# BACpypes compatibility: robust fix for the 'pduExpectingReply' AttributeError
try:
    from bacpypes.pdu import PCI
    _original_pci_update = PCI.update
    def _patched_pci_update(self, pci):
        if not hasattr(pci, 'pduExpectingReply'):
            pci.pduExpectingReply = False
        if not hasattr(pci, 'pduNetworkPriority'):
            pci.pduNetworkPriority = 0
        return _original_pci_update(self, pci)
    PCI.update = _patched_pci_update
except Exception:
    pass

# Fix for PyInstaller --windowed mode: redirect stdout/stderr to devnull to prevent uvicorn crash
if sys.stdout is None:
    sys.stdout = open(os.devnull, "w")
if sys.stderr is None:
    sys.stderr = open(os.devnull, "w")

import logging
from nicegui import ui
import threading
import sys
from app_state import AppState
from ui_dashboard import create_ui
from tray_icon import TrayApp, start_tray_thread
from bacnet_engine import BBMDRouter

# We'll move AppState to its own file to avoid circular imports if needed, 
# but for now let's just define it here or import it if I created it in ui_dashboard.
# Actually I'll create app_state.py for cleanliness.

def main():
    state = AppState()
    
    # Define callbacks for the tray icon
    def on_start():
        if not state.is_running:
            try:
                state.router = BBMDRouter(state.vpn_ip, state.lan_ip, int(state.port))
                state.router.start()
                state.is_running = True
                # Notify UI if needed (NiceGUI handles binding well)
                logging.info("Service started via Tray")
            except Exception as e:
                logging.error(f"Failed to start service from tray: {e}")

    def on_stop():
        if state.is_running:
            if state.router:
                state.router.stop()
            state.is_running = False
            logging.info("Service stopped via Tray")

    def on_exit():
        on_stop()
        logging.info("Application exiting")
        # Use os._exit to ensure all threads (NiceGUI, BACnet, Tray) are killed immediately
        import os
        os._exit(0)

    # 1. Initialize Tray
    tray = TrayApp(on_start=on_start, on_stop=on_stop, on_exit=on_exit)
    start_tray_thread(tray)

    # 2. Setup UI
    create_ui(state)

    # 3. Periodically sync tray state (optional but good for consistency)
    def sync_state():
        tray.update_state(state.is_running)
    
    ui.timer(1.0, sync_state)

    # 4. Run NiceGUI
    # host='0.0.0.0' allows access from other machines (e.g. via Tailscale)
    ui.run(title='BACnet BBMD Bridge', host='0.0.0.0', port=28821, show=True, reload=False)

if __name__ in {"__main__", "__mp_main__"}:
    main()
