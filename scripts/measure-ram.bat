@echo off
REM Lanza el .exe portable, espera a que se inicialice, lee su WorkingSet
REM (RAM en bytes) y su PrivateMemorySize, y los imprime. Después lo cierra.
REM Uso: scripts\measure-ram.bat <ruta_al_exe> [segundos_espera]

setlocal
set "EXE=%~1"
set "WAIT=%~2"
if "%WAIT%"=="" set "WAIT=15"
if not exist "%EXE%" (
    echo no existe: %EXE%
    exit /b 2
)
echo === Lanzando %EXE% (espera %WAIT%s) ===
start "" "%EXE%"
timeout /t %WAIT% /nobreak >nul
echo === Get-Process whispr-open (tras %WAIT%s) ===
powershell.exe -NoProfile -Command "Get-Process whispr-open -ErrorAction SilentlyContinue | Select-Object Id,ProcessName,@{n='WorkingSet_MB';e={[math]::Round($_.WorkingSet/1MB,2)}},@{n='PrivateMem_MB';e={[math]::Round($_.PrivateMemorySize/1MB,2)}},@{n='Virtual_MB';e={[math]::Round($_.VirtualMemorySize64/1MB,2)}} | Format-Table -AutoSize"
echo === Muestreos adicionales (cada 10s) ===
for /L %%i in (1,1,6) do (
    timeout /t 10 /nobreak >nul
    powershell.exe -NoProfile -Command "Get-Process whispr-open -ErrorAction SilentlyContinue | Select-Object @{n='t=+%WAIT%+%%i*10s';e={''}},@{n='WorkingSet_MB';e={[math]::Round($_.WorkingSet/1MB,2)}},@{n='PrivateMem_MB';e={[math]::Round($_.PrivateMemorySize/1MB,2)}} | Format-Table -AutoSize"
)
echo === Cerrando whispr-open.exe ===
taskkill /F /IM whispr-open.exe 2>nul
endlocal