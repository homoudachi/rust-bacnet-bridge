from nicegui import ui
import logging
from app_state import AppState
from bacnet_engine import BBMDRouter

class LogHandler(logging.Handler):
    def __init__(self, log_widget, state: AppState):
        super().__init__()
        self.log_widget = log_widget
        self.state = state

    def emit(self, record):
        msg = self.format(record)
        
        # Filter logic: if "Log VPN Only" is on, skip messages containing "[LAN]"
        if self.state.log_vpn_only and "[LAN]" in msg:
            return
            
        self.log_widget.push(msg)

def create_ui(state: AppState):
    # Styling
    ui.colors(primary='#2e7d32', secondary='#1565c0', accent='#fdd835')
    
    with ui.header().classes('items-center justify-between bg-primary text-white p-4'):
        ui.label('BACnet BBMD Tailscale Bridge').classes('text-2xl font-bold')
        ui.icon('lan', size='md')

    with ui.column().classes('w-full max-w-4xl mx-auto p-6 gap-6'):
        
        # Interface Selection
        with ui.card().classes('w-full shadow-lg p-4'):
            ui.label('Network Configuration').classes('text-xl font-semibold mb-4')
            
            with ui.row().classes('w-full gap-4'):
                vpn_opts = {i['ip']: i['label'] for i in state.interfaces if i['is_tailscale']}
                lan_opts = {i['ip']: i['label'] for i in state.interfaces if not i['is_tailscale']}
                
                # Auto-select defaults
                default_vpn = next(iter(vpn_opts.keys()), "")
                default_lan = next(iter(lan_opts.keys()), "")
                
                state.vpn_ip = default_vpn
                state.lan_ip = default_lan

                ui.select(vpn_opts, label='VPN Interface (Tailscale)', value=state.vpn_ip)\
                    .classes('flex-1').bind_value(state, 'vpn_ip').on_value_change(state.save_config)
                
                ui.select(lan_opts, label='Local LAN Interface', value=state.lan_ip)\
                    .classes('flex-1').bind_value(state, 'lan_ip').on_value_change(state.save_config)

            with ui.row().classes('w-full items-center gap-4 mt-4'):
                ui.number('UDP Port', value=state.port, format='%d')\
                    .classes('w-32').bind_value(state, 'port').on_value_change(state.save_config)
                
                ui.space()
                
                def toggle_service():
                    if not state.is_running:
                        try:
                            state.router = BBMDRouter(state.vpn_ip, state.lan_ip, int(state.port))
                            state.router.start()
                            state.is_running = True
                            start_btn.set_text('STOP BBMD')
                            start_btn.classes('bg-red-600', remove='bg-green-600')
                            status_dot.classes('bg-green-500', remove='bg-red-500')
                        except Exception as e:
                            ui.notify(f"Error starting service: {e}", color='negative')
                    else:
                        if state.router:
                            state.router.stop()
                        state.is_running = False
                        start_btn.set_text('START BBMD')
                        start_btn.classes('bg-green-600', remove='bg-red-600')
                        status_dot.classes('bg-red-500', remove='bg-green-500')

                with ui.row().classes('items-center gap-2'):
                    status_dot = ui.element('div').classes('w-3 h-3 rounded-full bg-red-500 shadow-sm')
                    start_btn = ui.button('START BBMD', on_click=toggle_service)\
                        .classes('bg-green-600 text-white font-bold px-8')

        # Foreign Device Table (FDT)
        with ui.card().classes('w-full shadow-lg p-4'):
            ui.label('Registered Foreign Devices (VPN Clients)').classes('text-xl font-semibold mb-2')
            
            columns = [
                {'name': 'address', 'label': 'Device Address', 'field': 'address', 'align': 'left'},
                {'name': 'ttl', 'label': 'TTL (sec)', 'field': 'ttl', 'align': 'center'},
                {'name': 'remaining', 'label': 'Remaining (sec)', 'field': 'remaining', 'align': 'center'},
            ]
            fdt_table = ui.table(columns=columns, rows=[]).classes('w-full')
            
            def update_fdt():
                if state.router and state.is_running:
                    rows = state.router.get_fdt()
                    fdt_table.rows = rows
                else:
                    fdt_table.rows = []
            
            ui.timer(2.0, update_fdt)

        # Logs
        with ui.card().classes('w-full shadow-lg p-4 flex-1'):
            with ui.row().classes('w-full justify-between items-center mb-2'):
                ui.label('System Logs').classes('text-xl font-semibold')
                ui.checkbox('Log VPN Traffic Only', value=True).bind_value(state, 'log_vpn_only')

            log_view = ui.log().classes('w-full h-96 font-mono text-sm bg-gray-900 text-green-400')
            
            # Setup logging redirection
            handler = LogHandler(log_view, state)
            handler.setFormatter(logging.Formatter('%(asctime)s - %(levelname)s - %(message)s', '%H:%M:%S'))
            logging.getLogger().addHandler(handler)
            logging.getLogger().setLevel(logging.INFO)

    # Footer
    with ui.footer().classes('bg-gray-100 text-gray-500 p-2 text-xs'):
        ui.label('Running on Localhost:28821 | BACnet Port: ' + str(state.port)).bind_text_from(state, 'port', backward=lambda x: f"Running on Localhost:28821 | BACnet Port: {x}")

if __name__ in {"__main__", "__mp_main__"}:
    state = AppState()
    create_ui(state)
    ui.run(title='BACnet BBMD Bridge', host='127.0.0.1', port=28821, show=False)
