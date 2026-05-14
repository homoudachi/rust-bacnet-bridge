import psutil
import socket

def get_network_interfaces():
    """
    Returns a list of dictionaries containing interface names and their IP addresses.
    Specifically labels Tailscale interfaces.
    """
    interfaces = []
    for interface_name, snics in psutil.net_if_addrs().items():
        for snic in snics:
            if snic.family == socket.AF_INET:
                ip_address = snic.address
                # Skip loopback
                if ip_address == "127.0.0.1":
                    continue
                
                is_tailscale = ip_address.startswith("100.")
                
                interfaces.append({
                    "name": interface_name,
                    "ip": ip_address,
                    "is_tailscale": is_tailscale,
                    "label": f"{interface_name} ({ip_address}){' [Tailscale]' if is_tailscale else ''}"
                })
    
    return interfaces

def find_tailscale_interface():
    """Returns the first detected Tailscale interface or None."""
    interfaces = get_network_interfaces()
    for iface in interfaces:
        if iface["is_tailscale"]:
            return iface
    return None

if __name__ == "__main__":
    # Test output
    print("Detected Interfaces:")
    for iface in get_network_interfaces():
        print(f"- {iface['label']}")
