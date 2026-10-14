"""Smoke test: install + verify + uninstall the new silb installer.
Drives the installer/uninstaller via subprocess with Inno Setup silent flags,
without opening any wizard window.
"""
import os
import subprocess
import sys
import time
import winreg

LOCAL_PROGRAMS = os.path.join(os.environ["LOCALAPPDATA"], "Programs", "Silb")
START_MENU = os.path.join(os.environ["APPDATA"], "Microsoft", "Windows", "Start Menu", "Programs", "Silb")
DESKTOP = os.path.join(os.environ["USERPROFILE"], "Desktop")
UNINSTALL_KEY = r"Software\Microsoft\Windows\CurrentVersion\Uninstall"

installer = sys.argv[1]
print(f"Installer: {installer}")
print(f"Installer size: {os.path.getsize(installer)} B ({os.path.getsize(installer) / 1024 / 1024:.2f} MiB)")

# 1) Uninstall any previous version first (whichever unins exists and is actually executable)
for unins_name in ("unins000.exe", "unins001.exe"):
    unins_path = os.path.join(LOCAL_PROGRAMS, unins_name)
    if os.path.exists(unins_path):
        print(f"=== Desinstalando version previa ({unins_name}) ===")
        try:
            r = subprocess.run([unins_path, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
                              capture_output=True, timeout=60)
            print(f"  exit={r.returncode}")
        except subprocess.TimeoutExpired:
            print(f"  TIMEOUT: el uninstaller {unins_name} no responde (probablemente roto); lo dejo huérfano")
        time.sleep(3)

# 2) Install (the installer creates Silb if it doesn't exist)
print("=== Instalando ===")
r = subprocess.run([installer, "/SP-", "/S", "/SILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/CLOSEAPPLICATIONS"],
                  capture_output=True, timeout=300)
print(f"  install exit={r.returncode}")
if r.returncode != 0:
    print(f"  stdout: {r.stdout.decode(errors='replace')}")
    print(f"  stderr: {r.stderr.decode(errors='replace')}")
    sys.exit(3)

time.sleep(3)
print("=== Instalacion OK ===")
print(f"  Contenido de {LOCAL_PROGRAMS}:")
for f in sorted(os.listdir(LOCAL_PROGRAMS)):
    full = os.path.join(LOCAL_PROGRAMS, f)
    size = os.path.getsize(full) if os.path.isfile(full) else "<dir>"
    print(f"    {f}  {size} B")
print(f"  Menu Inicio ({START_MENU}):")
if os.path.exists(START_MENU):
    for f in sorted(os.listdir(START_MENU)):
        print(f"    {f}")
print(f"  Escritorio (Silb*):")
for f in sorted(os.listdir(DESKTOP)):
    if "silb" in f.lower():
        print(f"    {f}")

# 3) ARP verification
print("=== Verificando ARP ===")
installed_exe = os.path.join(LOCAL_PROGRAMS, "silb.exe")
if not os.path.exists(installed_exe):
    print(f"ERROR: {installed_exe} no existe tras instalar")
    sys.exit(4)
print(f"  Tamano del .exe instalado: {os.path.getsize(installed_exe)} B ({os.path.getsize(installed_exe) / 1024 / 1024:.2f} MiB)")

# Walk the Uninstall registry key looking for our app
def find_arp_entry():
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, UNINSTALL_KEY) as root:
        i = 0
        while True:
            try:
                sub_name = winreg.EnumKey(root, i)
            except OSError:
                return None
            try:
                with winreg.OpenKey(root, sub_name) as sub:
                    dn, _ = winreg.QueryValueEx(sub, "DisplayName")
                    if dn == "Silb":
                        return sub_name
            except (OSError, FileNotFoundError):
                pass
            i += 1
    return None

arp_subkey = find_arp_entry()
if arp_subkey is None:
    print("  ARP: NO encontrado 'Silb' en HKCU Uninstall")
    sys.exit(5)
print(f"  ARP subkey: HKCU\\{UNINSTALL_KEY}\\{arp_subkey}")
fields = ["DisplayName", "DisplayVersion", "Publisher", "InstallLocation", "DisplayIcon", "InstallDate", "EstimatedSize"]
with winreg.OpenKey(winreg.HKEY_CURRENT_USER, f"{UNINSTALL_KEY}\\{arp_subkey}") as sub:
    for field in fields:
        try:
            v, t = winreg.QueryValueEx(sub, field)
            print(f"    {field}: {v} ({t})")
        except FileNotFoundError:
            print(f"    {field}: <no presente>")

# 4) Uninstall
print("=== Desinstalando ===")
unins = None
for name in ("unins001.exe", "unins000.exe"):
    p = os.path.join(LOCAL_PROGRAMS, name)
    if os.path.exists(p):
        unins = p
        break
if unins is None:
    print(f"ERROR: desinstalador no encontrado en {LOCAL_PROGRAMS}")
    sys.exit(6)
print(f"  ejecutando {unins}")
try:
    r = subprocess.run([unins, "/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"],
                      capture_output=True, timeout=120)
    print(f"  uninstall exit={r.returncode}")
except subprocess.TimeoutExpired:
    print(f"  uninstall TIMEOUT")
time.sleep(3)
print(f"  {LOCAL_PROGRAMS} existe tras uninstall? {os.path.exists(LOCAL_PROGRAMS)}")
if os.path.exists(LOCAL_PROGRAMS):
    leftover = os.listdir(LOCAL_PROGRAMS)
    print(f"  contenido residual: {leftover}")

# ARP re-check
arp_subkey2 = find_arp_entry()
print(f"  ARP Silb tras uninstall: {'TODAVIA PRESENTE' if arp_subkey2 else 'eliminado OK'}")

print("=== Smoke test terminado ===")