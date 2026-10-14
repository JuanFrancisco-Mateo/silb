@echo off
REM Ejecuta el instalador de Silb en modo totalmente silencioso
REM (/SP- /S /SILENT /SUPPRESSMSGBOXES), sin mostrar wizard ni UAC.
REM Para evitar reinstalar encima, primero desinstala si existe.

echo === Buscando instalacion previa ===
if exist "%LOCALAPPDATA%\Programs\Silb\unins000.exe" (
    echo === Desinstalando version previa ===
    "%LOCALAPPDATA%\Programs\Silb\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
    timeout /t 3 /nobreak >nul
)

echo === Instalando %~1 ===
"%~1" /SP- /S /SILENT /SUPPRESSMSGBOXES /NORESTART /CLOSEAPPLICATIONS
if errorlevel 1 (
    echo INSTALL FAILED code=%errorlevel%
    exit /b %errorlevel%
)
echo === Instalacion OK ===
dir /b "%LOCALAPPDATA%\Programs\Silb"
echo === Accesos directos ===
dir /b "%USERPROFILE%\Desktop" | findstr /i "Silb"
dir /b "%APPDATA%\Microsoft\Windows\Start Menu\Programs\Silb"