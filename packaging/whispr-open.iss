; whispr-open.iss — instalador de Silb v0.3.0 (Fase 5 / R7).
;
; INSTALACIÓN POR USUARIO (sin admin):
;   Silb se instala en %LOCALAPPDATA%\Programs\Silb, igual que
;   cualquier app de Microsoft Store o winget. Cero UAC, cero prompts.
;
; DESTINATARIOS:
;   Windows 10 1809+ / Windows 11. La app usa Iced 0.13 + whisper.cpp + CUDA
;   estático; el instalador NO incluye los modelos GGML (se descargan en el
;   primer arranque desde Hugging Face; ver src/transcribe.rs::ensure_model).
;   SÍ incluye el server LLM (`{app}\vendor\llama\`, ver vendor/README.md):
;   ~44 MB en CPU o ~700 MB con el pack CUDA de `vendor\llama\cuda\`.
;
; CÓMO COMPILARLO (Windows):
;   cmd /c ""%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /Q packaging\whispr-open.iss"
;   Salida: packaging\Output\silb-0.3.0-setup.exe (~20-25 MB, LZMA).
;
; VARIABLES DE USUARIO (las que se pueden sobreescribir por línea de comandos
; con /D="nombre=valor"):
;   WHISPR_RELEASE_DIR  directorio con silb.exe (por defecto ..\target\release)
;   WHISPR_VERSION      versión del paquete (por defecto 0.3.0)
;
; Ejemplo con override:
;   ISCC.exe /DWHISPR_VERSION=0.3.1 /DWHISPR_RELEASE_DIR=D:\builds\release packaging\whispr-open.iss

#define MyReleaseDir GetEnv("WHISPR_RELEASE_DIR") ? GetEnv("WHISPR_RELEASE_DIR") : "..\target\release"
#define MyAppName "Silb"
#define MyAppPublisher "Whispr Open contributors"
#define MyAppURL "https://github.com/JuanFrancisco-Mateo/whispr-open"
#define MyAppExeName "silb.exe"
#ifndef MyAppVersion
  #define MyAppVersion GetEnv("WHISPR_VERSION") ? GetEnv("WHISPR_VERSION") : "0.3.0"
#endif

[Setup]
; GUID estable del producto (cambia SOLO si cambia el nombre o el editor).
; Se MANTIENE tras el renombrado a Silb: es el mismo producto, así una
; instalación anterior se ACTUALIZA EN SITIO (Inno reutiliza el directorio del
; registro) en vez de aparecer dos veces en «Agregar o quitar programas».
AppId={{6E2A0E9C-1B5B-4F0E-9C8C-5F8F7B6A7D11}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppVerName={#MyAppName} {#MyAppVersion}
AppPublisher={#MyAppPublisher}
AppPublisherURL={#MyAppURL}
AppSupportURL={#MyAppURL}
AppUpdatesURL={#MyAppURL}
AppCopyright=Copyright (c) 2026 Whispr Open contributors
; Instalación POR USUARIO en %LOCALAPPDATA%\Programs\Silb. Cero UAC.
DefaultDirName={localappdata}\Programs\{#MyAppName}
; Sin privilegio de administrador: el instalador corre con el usuario actual.
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=dialog
; Compresión LZMA/ultra64 (mejor ratio para binarios Rust grandes).
Compression=lzma2/ultra64
SolidCompression=yes
LZMAUseSeparateProcess=yes
; Interfaz moderna (Win 10/11): look&feel nativo.
WizardStyle=modern
WizardSizePercent=120
; Icono del instalador (mismo .ico que el binario).
SetupIconFile=..\assets\whispr-open.ico
; Pantalla de bienvenida con la licencia MIT.
InfoBeforeFile=..\LICENSE
; Mensaje final de instalación.
InfoAfterFile=
; Desinstalador visible en "Agregar o quitar programas" (ARP).
Uninstallable=yes
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
UninstallFilesDir={app}
; Nombre del desinstalador.
OutputDir=Output
OutputBaseFilename=silb-{#MyAppVersion}-setup
; Permite instalar sobre una versión anterior sin desinstalar primero.
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
; Acceso directo en el escritorio (marcado por defecto, el usuario lo puede quitar).
Name: "desktopicon"; Description: "Crear un acceso directo en el escritorio"; GroupDescription: "Accesos directos:"; Flags: checkedonce

[Files]
; Binario principal: el .exe release con icono incrustado (ver build.rs).
Source: "{#MyReleaseDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
; Icono que verá el acceso directo (mismo .ico que el binario, redundante
; pero Inno lo necesita para el .lnk; el .exe ya lleva el suyo).
Source: "..\assets\whispr-open.ico"; DestDir: "{app}"; Flags: ignoreversion
; Server LLM EMBARCADO: viaja con la app para que el usuario no teclee
; ninguna ruta (la app lo busca en {app}\vendor\llama\llama-server.exe). Lo
; copia packaging\vendor-llama.sh desde el build local de llama.cpp; NO va a
; git. El subdirectorio cuda\ (runtime CUDA opcional) entra con recursesubdirs.
Source: "..\vendor\llama\*"; DestDir: "{app}\vendor\llama"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
; Menú Inicio (siempre).
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\whispr-open.ico"; Comment: "Dictado local por voz"
; Escritorio (opcional, según selección del usuario).
Name: "{userdesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\whispr-open.ico"; Comment: "Dictado local por voz"; Tasks: desktopicon
; Acceso directo al desinstalador (carpeta del menú Inicio).
Name: "{group}\Desinstalar {#MyAppName}"; Filename: "{uninstallexe}"; IconFilename: "{app}\whispr-open.ico"

[Run]
; Lanzar al terminar la instalación (opcional, marcado por defecto).
Filename: "{app}\{#MyAppExeName}"; Description: "Ejecutar {#MyAppName}"; Flags: nowait postinstall skipifsilent

[UninstallDelete]
; Limpiar el cache de modelos (los GGML pesan GB; mejor preguntar).
; Si quieres limpiarlo al desinstalar, descomenta la siguiente línea:
; Type: filesandordirs; Name: "{userappdata}\whispr-open\models"

[Code]
// Mensaje de confirmación al desinstalar: pregunta si quiere borrar también
// la config y los modelos (lo habitual es "sí" porque se puede regenerar).
// Se respeta el flag /VERYSILENT: si el usuario desinstala sin UI, no se
// muestra ningún MsgBox y se mantiene la config (lo más seguro).
function InitializeUninstall(): Boolean;
var
  ResultCode: Integer;
begin
  Result := True;
  if not UninstallSilent then
  begin
    if MsgBox(
      '¿Quieres borrar también tu configuración personal y los modelos descargados?' + #13#10 +
      '(Si dices NO, podrás seguir usando Silb si lo reinstalas.)',
      mbConfirmation, MB_YESNO
    ) = IDYES then
    begin
      DelTree(ExpandConstant('{userappdata}\whispr-open'), True, True, True);
    end;
  end;
end;