@echo off
REM Lista entradas ARP que coincidan con "Silb" (versión instalada,
REM editor, icono).
reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall" /s /f "Silb" 2>nul | findstr /i "DisplayName DisplayVersion Publisher DisplayIcon InstallLocation"