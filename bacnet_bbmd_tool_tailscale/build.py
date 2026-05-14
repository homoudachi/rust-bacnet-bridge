import os
import subprocess
import nicegui
from pathlib import Path

def build():
    # Find NiceGUI directory to include its assets
    nicegui_path = Path(nicegui.__file__).parent
    
    pyinstaller_exe = r'C:\Users\Matt\AppData\Roaming\Python\Python313\Scripts\pyinstaller.exe'
    
    # Define the PyInstaller command
    cmd = [
        pyinstaller_exe,
        '--noconfirm',
        '--onefile',
        '--windowed',
        '--name', 'BACnet-BBMD-Tool',
        f'--add-data={nicegui_path};nicegui',
        '--hidden-import', 'asyncore',
        '--hidden-import', 'asynchat',
        '--hidden-import', 'bacpypes.debugging',
        '--hidden-import', 'bacpypes.comm',
        'main.py'
    ]
    
    print(f"Running build command: {' '.join(cmd)}")
    subprocess.run(cmd, check=True)

if __name__ == "__main__":
    build()
