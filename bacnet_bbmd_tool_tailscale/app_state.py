import json
import os
from network_utils import get_network_interfaces

class AppState:
    def __init__(self):
        self.interfaces = get_network_interfaces()
        self.vpn_ip = ""
        self.lan_ip = ""
        self.port = 20000
        self.log_vpn_only = True
        self.is_running = False
        self.router = None
        self.config_file = "config.json"
        self.load_config()

    def load_config(self):
        if os.path.exists(self.config_file):
            try:
                with open(self.config_file, 'r') as f:
                    data = json.load(f)
                    self.vpn_ip = data.get('vpn_ip', "")
                    self.lan_ip = data.get('lan_ip', "")
                    self.port = data.get('port', 20000)
            except Exception:
                pass

    def save_config(self):
        data = {
            'vpn_ip': self.vpn_ip,
            'lan_ip': self.lan_ip,
            'port': self.port
        }
        try:
            with open(self.config_file, 'w') as f:
                json.dump(data, f)
        except Exception:
            pass
