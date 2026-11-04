; whispr-open.iss — instalador de Silb.
;
; TRES MODOS, un solo script (ISPP elige el modo por la presencia de un define):
;
;   1. ONLINE (sin defines, el que se publica, ~16 MB):
;      ISCC.exe packaging\whispr-open.iss
;        -> packaging\Output\silb-<v>-setup.exe
;      Lleva SOLO el build CPU. Si al instalar se detecta una GPU NVIDIA, ofrece
;      descargar el pack CUDA (silb-cuda-pack-<v>.exe) del release, lo verifica
;      con SHA-256 y lo aplica en silencio. Sin GPU, sin red, hash que no cuadra
;      o el usuario dice que no -> instala en CPU y sigue: nunca aborta.
;
;   2. OFFLINE (las dos versiones dentro, ~466 MB):
;      ISCC.exe /DMyOffline /DWHISPR_VERSION=0.8.0 packaging\whispr-open.iss
;        -> packaging\Output\silb-<v>-offline-setup.exe
;
;   3. PACK CUDA (/DMyPack, ~465 MB): solo [Files], se ejecuta por encima de una
;      instalación ya hecha y reemplaza binario + DLLs del runtime. No crea
;      desinstalador, ni iconos, ni accesos directos.
;      ISCC.exe /DMyPack /DWHISPR_VERSION=0.8.0 packaging\whispr-open.iss
;        -> packaging\Output\silb-cuda-pack-<v>.exe
;
; Insumos (en packaging\build\, ninguno va a git):
;   bash packaging/build-cuda-runtime.sh   -> build\cuda12\ (cudart, cublas, cublasLt)
;   build\silb-cpu.exe                     -> cargo build --release
;   build\silb-cuda.exe                    -> cargo build --release --features cuda
; packaging\build-installers.sh encadena todo: pack -> .sha256 -> instalador
; online con /DPACKURL y /DPACKSHA256 ya incrustados.
;
; DETECCIÓN DE GPU (function UseCuda, [Code]):
;   1. nvidia-smi.exe --query-gpu=name --format=csv,noheader con código de salida 0
;   2. si no está, WMI Win32_VideoController con «NVIDIA» en el nombre
;
; PARÁMETROS DE PRUEBA:
;   /FORCECPU    instala el build CPU aunque haya GPU NVIDIA (no ofrece el pack)
;   /FORCECUDA   fuerza el camino CUDA aunque no se detecte GPU
;   /PACKURL=... /PACKSHA256=...  sobreescriben la URL y el hash incrustados
;   Ej.: silb-0.8.0-setup.exe /VERYSILENT /DIR=C:\Temp\silb /PACKURL=http://127.0.0.1:8000/silb-cuda-pack-0.8.0.exe /PACKSHA256=<hash>
;
; DESTINATARIOS:
;   Windows 10 1809+ / Windows 11 x64. La app usa Iced 0.13 + whisper.cpp; el
;   instalador NO incluye los modelos GGML (se descargan en el primer arranque
;   desde Hugging Face; ver src/transcribe.rs::ensure_model). SÍ incluye el
;   server LLM en CPU ({app}\vendor\llama\, ver vendor/README.md).
;
; VARIABLES DE USUARIO (sobreescribibles con /D="nombre=valor"):
;   WHISPR_RELEASE_DIR  directorio con silb.exe (solo en modo online)
;   WHISPR_CUDA_EXE     exe CUDA (por defecto build\silb-cuda.exe)
;   WHISPR_VERSION      versión del paquete (por defecto 0.8.0)
;   WHISPR_PACK_URL / WHISPR_PACK_SHA256  pack remoto (o /DPACKURL /DPACKSHA256)

#define MyReleaseDir GetEnv("WHISPR_RELEASE_DIR") ? GetEnv("WHISPR_RELEASE_DIR") : "..\target\release"
#define MyCudaExe GetEnv("WHISPR_CUDA_EXE") ? GetEnv("WHISPR_CUDA_EXE") : "build\silb-cuda.exe"
#define MyCudaRuntime "build\cuda12"
#define MyAppName "Silb"
#define MyAppPublisher "Whispr Open contributors"
#define MyAppURL "https://github.com/JuanFrancisco-Mateo/silb"
#define MyAppExeName "silb.exe"
; La versión se puede dar como /DWHISPR_VERSION=x (define de ISPP) o como
; variable de entorno WHISPR_VERSION; el define manda.
#ifndef MyAppVersion
  #ifdef WHISPR_VERSION
    #define MyAppVersion WHISPR_VERSION
  #else
    #define MyAppVersion GetEnv("WHISPR_VERSION") ? GetEnv("WHISPR_VERSION") : "0.8.0"
  #endif
#endif
; Nombre del pack en el release (el mismo que produce /DMyPack).
#define MyPackBaseName "silb-cuda-pack-" + MyAppVersion + ".exe"

; --- modo: /DMyPack (pack) · /DMyOffline (todo dentro) · nada (online) -------
; La presencia del define es el flag (ISPP compara mal string vs número).
#ifdef MyPack
  #ifdef MyOffline
    #error /DMyPack y /DMyOffline son excluyentes
  #endif
  #define MyOutputBase "silb-cuda-pack-" + MyAppVersion
  #if !FileExists(AddBackslash(SourcePath) + MyCudaExe)
    #error El pack CUDA necesita el exe CUDA (packaging\build\silb-cuda.exe)
  #endif
  #if (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cudart64_12.dll")) || (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cublas64_12.dll")) || (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cublasLt64_12.dll"))
    #error Falta el runtime CUDA 12: bash packaging/build-cuda-runtime.sh
  #endif
#else
  #ifdef MyOffline
    #define MyOutputBase "silb-" + MyAppVersion + "-offline-setup"
    #if !FileExists(AddBackslash(SourcePath) + "build\silb-cpu.exe")
      #error El instalador offline necesita packaging\build\silb-cpu.exe
    #endif
    #if !FileExists(AddBackslash(SourcePath) + MyCudaExe)
      #error El instalador offline necesita el exe CUDA (packaging\build\silb-cuda.exe)
    #endif
    #if (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cudart64_12.dll")) || (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cublas64_12.dll")) || (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cublasLt64_12.dll"))
      #error Falta el runtime CUDA 12: bash packaging/build-cuda-runtime.sh
    #endif
  #else
    ; Modo online: el pack vive en el release. El hash es obligatorio (sin él la
    ; descarga no se puede verificar, y un instalador sin verificación no se
    ; publica); lo escribe packaging/build-installers.sh a partir del pack.
    #define MyOutputBase "silb-" + MyAppVersion + "-setup"
    ; Los nombres de /D no llevan el prefijo My: /DPACKURL, /DPACKSHA256.
    #ifdef PACKURL
      #define MyPackUrl PACKURL
    #endif
    #ifndef MyPackUrl
      #define MyPackUrl GetEnv("WHISPR_PACK_URL") ? GetEnv("WHISPR_PACK_URL") : "https://github.com/JuanFrancisco-Mateo/silb/releases/download/v" + MyAppVersion + "/" + MyPackBaseName
    #endif
    #ifdef PACKSHA256
      #define MyPackSha256 PACKSHA256
    #endif
    #ifndef MyPackSha256
      #define MyPackSha256 GetEnv("WHISPR_PACK_SHA256") ? GetEnv("WHISPR_PACK_SHA256") : ""
    #endif
    #if MyPackSha256 == ""
      #error Falta el SHA-256 del pack CUDA: /DPACKSHA256=<hash> (o WHISPR_PACK_SHA256)
    #endif
  #endif
#endif

[Setup]
; GUID estable del producto (cambia SOLO si cambia el nombre o el editor).
; Se MANTIENE tras el renombrado a Silb: es el mismo producto, así una
; instalación anterior se ACTUALIZA EN SITIO (Inno reutiliza el directorio del
; registro) en vez de aparecer dos veces en «Agregar o quitar programas».
#ifdef MyPack
; El pack NO es el producto: GUID propio y sin desinstalador, para que correrlo
; por encima de la app no le toque el registro ni el unins000.exe.
AppId={{9D3B7A54-2F6E-4C71-9A0E-1C6B5C2D8E43}
#else
AppId={{6E2A0E9C-1B5B-4F0E-9C8C-5F8F7B6A7D11}
#endif
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
#ifdef MyPack
; El pack solo reemplaza ficheros en el directorio que le pasa el instalador
; base (/DIR={app}): nada de páginas ni de desinstalador.
Uninstallable=no
DisableDirPage=yes
DisableProgramGroupPage=yes
#else
; Icono del instalador (mismo .ico que el binario).
SetupIconFile=..\assets\whispr-open.ico
; Pantalla de bienvenida con la licencia MIT.
InfoBeforeFile=..\LICENSE
; Desinstalador visible en "Agregar o quitar programas" (ARP).
Uninstallable=yes
UninstallDisplayIcon={app}\{#MyAppExeName}
UninstallDisplayName={#MyAppName}
UninstallFilesDir={app}
#endif
; Nombre del desinstalador.
OutputDir=Output
OutputBaseFilename={#MyOutputBase}
; Permite instalar sobre una versión anterior sin desinstalar primero.
MinVersion=10.0
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "spanish"; MessagesFile: "compiler:Languages\Spanish.isl"
Name: "english"; MessagesFile: "compiler:Default.isl"

[CustomMessages]
spanish.PackCaption=Preparando la aceleración por GPU
spanish.PackDesc=Se va a descargar el paquete de aceleración por GPU.
spanish.PackOffer=GPU NVIDIA detectada: %1. ¿Descargar el paquete de aceleración por GPU (unos 500 MB)? Si dices que no, Silb se instala en modo CPU y funciona igual, más lento.
spanish.NoGpuAviso=Sin aceleración por GPU: se puede añadir después.
english.PackCaption=Preparing GPU acceleration
english.PackDesc=Downloading the GPU acceleration pack.
english.PackOffer=NVIDIA GPU detected: %1. Download the GPU acceleration pack (about 500 MB)? If you say no, Silb installs in CPU mode and works the same, just slower.
english.NoGpuAviso=Without GPU acceleration: it can be added later.

; Nada que marcar en el pack (no hay iconos ni accesos directos).
#ifndef MyPack
[Tasks]
; Acceso directo en el escritorio (marcado por defecto, el usuario lo puede quitar).
Name: "desktopicon"; Description: "Crear un acceso directo en el escritorio"; GroupDescription: "Accesos directos:"; Flags: checkedonce
#endif

[Files]
#ifdef MyPack
; Solo dos cosas: el binario CUDA encima de {app}\silb.exe y las tres DLL del
; runtime a su lado (Windows las resuelve del directorio del ejecutable, así que
; el equipo NO necesita el toolkit CUDA instalado).
Source: "{#MyCudaExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion
Source: "{#MyCudaRuntime}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
#else
#ifdef MyOffline
; Un solo {app}\silb.exe, elegido al instalar: build CUDA + su runtime si hay
; GPU NVIDIA, build CPU si no.
Source: "build\silb-cpu.exe"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion; Check: not UseCuda
Source: "{#MyCudaExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion; Check: UseCuda
Source: "{#MyCudaRuntime}\*.dll"; DestDir: "{app}"; Flags: ignoreversion; Check: UseCuda
#else
; Online: solo el build CPU. El CUDA llega después, si hay GPU, como pack
; descargado (silb-cuda-pack-<v>.exe).
Source: "{#MyReleaseDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
#endif
#endif
; Icono que verá el acceso directo (mismo .ico que el binario, redundante
; pero Inno lo necesita para el .lnk; el .exe ya lleva el suyo).
Source: "..\assets\whispr-open.ico"; DestDir: "{app}"; Flags: ignoreversion
; Server LLM EMBARCADO: viaja con la app para que el usuario no teclee
; ninguna ruta (la app lo busca en {app}\vendor\llama\llama-server.exe). Lo
; copia packaging\vendor-llama.sh desde el build local de llama.cpp; NO va a
; git. `cuda\` se EXCLUYE siempre: son ~656 MB de DLLs de NVIDIA (CUDA 13) que
; solo usa llama-server, y multiplicarían por cinco el instalador.
Source: "..\vendor\llama\*"; DestDir: "{app}\vendor\llama"; Excludes: "cuda"; Flags: ignoreversion recursesubdirs createallsubdirs

#ifndef MyPack
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
; backend.txt lo escribe [Code] (no es una entrada de [Files]), así que Inno no
; lo borra solo: se limpia aquí para no dejar el directorio con basura.
Type: files; Name: "{app}\backend.txt"
; Las DLL del pack CUDA las pone el pack, no este instalador: su desinstalador
; no las conoce y sin esto quedan 772 MB en {app} tras desinstalar.
Type: files; Name: "{app}\cudart64_12.dll"
Type: files; Name: "{app}\cublas64_12.dll"
Type: files; Name: "{app}\cublasLt64_12.dll"
#endif

; El pack es solo [Files]: no necesita una gota de Pascal.
#ifndef MyPack
[Code]
var
  GpuName: String;     // nombre comercial de la GPU, para el mensaje final
  GpuChecked: Boolean; // la detección se hace una sola vez y se cachea
  GpuPresent: Boolean;
  GpuForcedCpu: Boolean; // /FORCECPU: hay GPU pero el usuario pide el build CPU
  CudaApplied: Boolean;  // quedó el binario CUDA en {app} (offline: UseCuda)
#ifndef MyOffline
  PackReady: Boolean;    // el pack se descargó y su SHA-256 cuadró
  PackTried: Boolean;    // la oferta se hace una sola vez
  DownloadPage: TDownloadWizardPage;
#endif

// ¿Está el parámetro en la línea de comandos? /FORCECPU, /FORCECPU=1, /FORCECPU=yes.
// Ojo: el constante {param:...} no se puede usar dentro de [Code], y CmdLine no
// existe como identificador: la cola de la línea de comandos es GetCmdTail.
function ParamOn(const Name: String): Boolean;
begin
  Result := Pos('/' + Uppercase(Name), Uppercase(GetCmdTail)) > 0;
end;

// Valor de un /CLAVE=valor de la línea de comandos ('' si no viene). Sirve para
// /PACKURL y /PACKSHA256: probar el instalador sin publicar nada.
function ParamValue(const Name, Def: String): String;
var
  Tail, Key: String;
  P, E: Integer;
begin
  Result := Def;
  Tail := GetCmdTail;
  Key := '/' + Uppercase(Name) + '=';
  P := Pos(Key, Uppercase(Tail));
  if P = 0 then
    exit;
  Tail := Copy(Tail, P + Length(Key), Length(Tail));
  if (Length(Tail) > 0) and (Tail[1] = '"') then
  begin
    Delete(Tail, 1, 1);
    E := Pos('"', Tail);
  end
  else
    E := Pos(' ', Tail);
  if E = 0 then
    E := Length(Tail) + 1;
  Result := Copy(Tail, 1, E - 1);
end;

// nvidia-smi lo instala el driver de NVIDIA; código de salida 0 = hay GPU viva.
// {sys} en un Setup de 32 bits es SysWOW64, y nvidia-smi.exe solo existe en
// System32: por eso se prefiere {sysnative}.
function NvidiaSmiOk: Boolean;
var
  Smi: String;
  Code: Integer;
begin
  Smi := ExpandConstant('{sysnative}\nvidia-smi.exe');
  if not FileExists(Smi) then
    Smi := ExpandConstant('{sys}\nvidia-smi.exe');
  Result := Exec(Smi, '--query-gpu=name --format=csv,noheader', '', SW_HIDE,
    ewWaitUntilTerminated, Code) and (Code = 0);
  if not Result then
    Log('Silb: nvidia-smi sin respuesta válida; se pregunta a WMI');
end;

// Sin nvidia-smi (driver viejo, o equipo sin él) se mira el nombre del adaptador
// de vídeo en WMI. Devuelve '' si no hay ninguna NVIDIA.
function WmiNvidiaName: String;
var
  Locator, Service, Items, Obj: Variant;
  I: Integer;
  N: String;
begin
  Result := '';
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\cimv2');
    Items := Service.ExecQuery('SELECT Name FROM Win32_VideoController');
    for I := 0 to Items.Count - 1 do
    begin
      // SWbemObjectSet.Item() espera la ruta del objeto, no un índice:
      // para recorrer por número está ItemIndex().
      Obj := Items.ItemIndex(I);
      N := Obj.Name;
      if Pos('NVIDIA', Uppercase(N)) > 0 then
      begin
        Result := Trim(N);
        exit;
      end;
    end;
  except
    Log('Silb: WMI no disponible para detectar la GPU: ' + GetExceptionMessage);
  end;
end;

// Decisión única: ¿este equipo va por GPU? Lo consultan [Files] (offline), la
// oferta del pack (online) y CurStepChanged; se cachea porque [Files] la llama
// por cada fichero y nvidia-smi tarda lo suyo.
function UseCuda: Boolean;
begin
  if not GpuChecked then
  begin
    GpuChecked := True;
    GpuPresent := False;
    if ParamOn('FORCECPU') then
    begin
      GpuForcedCpu := True;
      Log('Silb: /FORCECPU, se instala el build CPU');
    end
    else if ParamOn('FORCECUDA') then
    begin
      GpuPresent := True;
      GpuName := WmiNvidiaName;
      if GpuName = '' then
        GpuName := 'GPU forzada con /FORCECUDA';
      Log('Silb: /FORCECUDA, se instala el build CUDA');
    end
    else if NvidiaSmiOk then
    begin
      GpuPresent := True;
      GpuName := WmiNvidiaName;
      if GpuName = '' then
        GpuName := 'GPU NVIDIA';
    end
    else
    begin
      GpuName := WmiNvidiaName;
      GpuPresent := GpuName <> '';
      if GpuPresent then
        Log('Silb: GPU NVIDIA por WMI: ' + GpuName)
      else
        Log('Silb: no se detecta GPU NVIDIA; se instala el build CPU');
    end;
  end;
  Result := GpuPresent;
end;

#ifndef MyOffline
// Aplica el pack ya descargado y comprueba que de verdad reemplazó el binario:
// silb.exe tiene que existir y pesar MÁS que el build CPU que acaba de copiar
// [Files] (49,9 MB frente a 23,7 MB). Es la comprobación barata y sin números
// mágicos de «quedó el build CUDA».
function ApplyPack: Boolean;
var
  Pack, Exe: String;
  Code: Integer;
  Before, After: Int64;
begin
  Result := False;
  Pack := ExpandConstant('{tmp}\') + '{#MyPackBaseName}';
  Exe := ExpandConstant('{app}\{#MyAppExeName}');
  FileSize64(Exe, Before);
  if not Exec(Pack, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR="'
    + ExpandConstant('{app}') + '"', '', SW_HIDE, ewWaitUntilTerminated, Code) then
  begin
    Log('Silb: no se pudo ejecutar el pack CUDA (' + Pack + ')');
    exit;
  end;
  FileSize64(Exe, After);
  Result := (Code = 0) and FileExists(Exe) and (After > Before);
  // Ojo: una línea que empiece por «[» la toma Inno por un tag de sección, así
  // que el array de Format va en la misma línea que la llamada.
  Log(Format('Silb: pack CUDA salida=%d, silb.exe %d -> %d bytes, aplicado=%s', [Code,
    Before, After, IntToStr(Ord(Result))]));
end;

procedure InitializeWizard;
begin
  DownloadPage := CreateDownloadPage(CustomMessage('PackCaption'),
    CustomMessage('PackDesc'), nil);
  DownloadPage.ShowBaseNameInsteadOfUrl := True;
end;

// La descarga va en wpReady: el usuario ya eligió carpeta y tareas, y todavía no
// se ha copiado nada. El pack se APLICA luego, en ssPostInstall, porque antes no
// existiría {app}\silb.exe al que reemplazar.
// Cualquier problema (sin red, hash, cancelación) deja PackReady en False: la
// instalación sigue en CPU y el mensaje final lo dice.
function NextButtonClick(CurPageID: Integer): Boolean;
var
  Url, Sha: String;
begin
  Result := True;
  if (CurPageID <> wpReady) or PackTried then
    exit;
  PackTried := True;
  if not UseCuda then
    exit;
  if not WizardSilent then
    if MsgBox(FmtMessage(CustomMessage('PackOffer'), [GpuName]),
      mbConfirmation, MB_YESNO) <> IDYES then
    begin
      Log('Silb: el usuario renuncia al pack CUDA; se instala el build CPU');
      exit;
    end;
  Url := ParamValue('PACKURL', '{#MyPackUrl}');
  Sha := ParamValue('PACKSHA256', '{#MyPackSha256}');
  Log('Silb: descargando el pack CUDA de ' + Url + ' (sha256 ' + Sha + ')');
  DownloadPage.Clear;
  // Add(Url, BaseName, RequiredSHA256OfFile): la tercera es la verificación de
  // integridad y es obligatoria; si el fichero no cuadra, Download lanza
  // excepción. (En Inno 6.7 DownloadTemporaryFile lleva un callback extra como
  // cuarto parámetro; la página es el camino documentado en CodeDownloadFiles.iss.)
  DownloadPage.Add(Url, '{#MyPackBaseName}', Sha);
  DownloadPage.Show;
  try
    try
      DownloadPage.Download; // a {tmp}, ya verificado
      PackReady := True;
    except
      PackReady := False;
      if DownloadPage.AbortedByUser then
        Log('Silb: descarga del pack cancelada por el usuario')
      else
        Log('Silb: descarga del pack fallida (' + DownloadPage.LastBaseNameOrUrl
          + '): ' + GetExceptionMessage);
    end;
  finally
    DownloadPage.Hide;
  end;
end;
#endif

// Espacio en disco: el modo offline copia ~850 MB, así que se comprueba antes de
// empezar. En modo online el pack es opcional y el fallback ya cubre el caso, así
// que no hay nada que exigir aquí.
function PrepareToInstall(var NeedsRestart: Boolean): String;
var
  Drive: String;
  FreeMB, TotalMB: Cardinal;
begin
  Result := '';
#ifdef MyOffline
  if not UseCuda then
    exit;
  Drive := ExtractFileDrive(ExpandConstant('{app}')) + '\';
  if not GetSpaceOnDisk(Drive, True, FreeMB, TotalMB) then
  begin
    Drive := ExpandConstant('{sd}\');
    if not GetSpaceOnDisk(Drive, True, FreeMB, TotalMB) then
      exit;
  end;
  // ~50 MB de exe + ~772 MB de runtime, y hace falta el doble mientras se
  // descomprime. Por debajo de esto el instalador dejaría una copia a medias.
  if FreeMB < 1800 then
    Result := 'La versión con aceleración por GPU ocupa unos 850 MB y no queda '
      + 'espacio libre suficiente en ' + Drive + ' (' + IntToStr(FreeMB) + ' MB).'
      + #13#10 + #13#10
      + 'Libera espacio y vuelve a ejecutar el instalador, o instala la versión '
      + 'CPU con /FORCECPU (funciona igual, transcribe más lento).';
#endif
end;

procedure CurStepChanged(CurStep: TSetupStep);
var
  Backend, Aviso: String;
begin
  if CurStep <> ssPostInstall then
    exit;
#ifdef MyOffline
  CudaApplied := UseCuda;
#else
  // El pack se aplica aquí: ya están los ficheros de [Files] en {app}.
  CudaApplied := PackReady and ApplyPack;
#endif
  if CudaApplied then
  begin
    Backend := 'cuda';
    Aviso := 'Aceleración por GPU: activada (' + GpuName + ')';
  end
  else if GpuForcedCpu then
  begin
    Backend := 'cpu';
    Aviso := 'Aceleración por GPU: desactivada (/FORCECPU)';
  end
  else
  begin
    Backend := 'cpu';
#ifdef MyOffline
    Aviso := 'No se detecta GPU NVIDIA: modo CPU';
#else
    // Fallo del pack o renuncia del usuario: el mensaje dice explícitamente que
    // la app funciona y que la GPU se puede añadir luego con el mismo pack.
    Aviso := CustomMessage('NoGpuAviso');
#endif
  end;
  // backend.txt: qué binario quedó instalado. En silencioso es la única
  // constancia que ve quien automatiza la instalación.
  SaveStringToFile(ExpandConstant('{app}\backend.txt'),
    'backend=' + Backend + #13#10
    + 'gpu=' + GpuName + #13#10
    + 'aviso=' + Aviso + #13#10, False);
  Log('Silb: ' + Aviso);
  if not WizardSilent then
    MsgBox(Aviso + #13#10 + #13#10
      + 'Ya está copiado el binario que corresponde a este equipo; no hay '
      + 'nada más que configurar.', mbInformation, MB_OK);
end;

// Mensaje de confirmación al desinstalar: pregunta si quiere borrar también
// la config y los modelos (lo habitual es "sí" porque se puede regenerar).
// Se respeta el flag /VERYSILENT: si el usuario desinstala sin UI, no se
// muestra ningún MsgBox y se mantiene la config (lo más seguro).
function InitializeUninstall(): Boolean;
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
#endif
