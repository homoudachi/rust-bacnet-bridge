import pystray
from PIL import Image, ImageDraw
import threading
import sys
import webbrowser

def create_status_image(color="red"):
    # Create a 64x64 image for the tray icon
    width = 64
    height = 64
    image = Image.new('RGBA', (width, height), (0, 0, 0, 0))
    dc = ImageDraw.Draw(image)
    
    # Draw a circle with the specified color
    padding = 8
    dc.ellipse([padding, padding, width - padding, height - padding], fill=color, outline="white", width=2)
    
    return image

class TrayApp:
    def __init__(self, on_start=None, on_stop=None, on_exit=None):
        self.on_start = on_start
        self.on_stop = on_stop
        self.on_exit = on_exit
        self.icon = None
        self._is_running_service = False

    def _create_menu(self):
        return pystray.Menu(
            pystray.MenuItem("Open Dashboard", self._open_dashboard),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Start BBMD", self._start_service, enabled=lambda item: not self._is_running_service),
            pystray.MenuItem("Stop BBMD", self._stop_service, enabled=lambda item: self._is_running_service),
            pystray.Menu.SEPARATOR,
            pystray.MenuItem("Exit", self._exit_app)
        )

    def _open_dashboard(self, icon, item):
        webbrowser.open("http://localhost:28821")

    def _start_service(self, icon, item):
        if self.on_start:
            self.on_start()
        self.update_state(True)

    def _stop_service(self, icon, item):
        if self.on_stop:
            self.on_stop()
        self.update_state(False)

    def _exit_app(self, icon, item):
        if self.on_exit:
            self.on_exit()
        icon.stop()
        sys.exit(0)

    def update_state(self, is_running):
        self._is_running_service = is_running
        color = "green" if is_running else "red"
        if self.icon:
            self.icon.icon = create_status_image(color)

    def run(self):
        self.icon = pystray.Icon(
            "bacnet_bbmd_bridge",
            create_status_image("red"),
            "BACnet BBMD Bridge",
            self._create_menu()
        )
        self.icon.run()

def start_tray_thread(app):
    thread = threading.Thread(target=app.run, daemon=True)
    thread.start()
    return thread

if __name__ == "__main__":
    # Test stub
    app = TrayApp()
    app.run()
