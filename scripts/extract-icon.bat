@echo off
REM Extrae el icono asociado al binario y reporta sus dimensiones.
REM Uso: scripts\extract-icon.bat <ruta_al_exe>
powershell.exe -NoProfile -ExecutionPolicy Bypass -Command "Add-Type -AssemblyName System.Drawing; $ico = [System.Drawing.Icon]::ExtractAssociatedIcon('%~1'); if ($null -eq $ico) { Write-Error 'no se extrajo icono'; exit 3 }; Write-Host ('Icono incrustado OK: {0}x{1} px' -f $ico.Width, $ico.Height)"