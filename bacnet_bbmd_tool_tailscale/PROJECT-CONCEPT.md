Act as an expert Python developer specializing in Industrial IoT, BACnet protocols, and Windows desktop applications. 

Your task is to write a complete, runnable Python application for Windows 11 that acts as a BACnet BBMD (Broadcast Management Device) bridging a Tailscale VPN adapter and a physical LAN adapter.

### Technical Stack:
- **Core Protocol:** `BAC0` (and its underlying `bacpypes` library).
- **System Tray:** `pystray` and `Pillow` (for generating a dynamic tray icon).
- **User Interface:** `NiceGUI` (for a lightweight, local web-based configuration dashboard).
- **Concurrency:** `threading` and `asyncio` to ensure the BACnet stack, the NiceGUI server, and the pystray event loop run concurrently without blocking each other.

### Functional Requirements:

1. **Network Interface Detection:**
   - Write a helper function to automatically detect and list available IP addresses.
   - Specifically identify the Tailscale IP (typically `100.*.*.*`) and the primary local LAN IP.

2. **The BACnet BBMD Service:**
   - Provide a class or function to initialize a `BAC0` application that listens on a configurable UDP port (default is `20000`).
   - The stack must be configured to bind to the Tailscale IP address so it can receive Foreign Device Registrations (FDR).
   - Implement the necessary `bacpypes` routing/BBMD logic so that Who-Is requests received from the Foreign Device over the VPN are broadcasted out to the local LAN adapter, and I-Am responses are routed back over the VPN.
   - Include methods to gracefully start, stop, and restart the BAC0 network instance.

3. **System Tray Integration:**
   - The application should run minimized in the Windows system tray.
   - Generate a simple programmatic icon using `Pillow` that changes color based on state (e.g., Red = Stopped, Green = Running).
   - The right-click menu must include: "Open Dashboard", "Start BBMD", "Stop BBMD", and "Exit".

4. **NiceGUI Dashboard:**
   - Create a local NiceGUI dashboard running on a local port (e.g., `localhost:8080`).
   - The UI must contain:
     - Dropdown menus to select the "VPN Interface (Tailscale)" and "Local LAN Interface".
     - A numeric input for the "UDP Port" (Default: 20000).
     - A clear, styled toggle or button to Start/Stop the BACnet bridging service.
     - A scrollable log window (`ui.log`) that captures and displays system events, connection statuses, and BAC0 debug outputs in real-time.

### Important Implementation Details:
- **Port Conflicts:** Ensure the socket binds cleanly and releases the port when the service is stopped. 
- **Windows Firewall:** Include a commented-out `subprocess` command or instructions in the code for adding the necessary Windows Firewall rule to allow inbound UDP traffic on the specified port.
- **Routing Nuance:** BAC0's default `lite` initialization binds to one IP. To act as a BBMD router bridging *two* interfaces, you may need to utilize `bacpypes.vlan` or configure a `BIPBBMD` node and a standard `BIPSimple` node attached to a `NetworkRouter`. Please provide the most robust method for this dual-NIC bridging.