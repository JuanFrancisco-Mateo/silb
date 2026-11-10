; silb.iss — instalador de Silb.
;
; CUATRO MODOS, un solo script (ISPP elige el modo por la presencia de un define):
;
;   1. ONLINE (sin defines, el que se publica, ~21 MB):
;      ISCC.exe packaging\silb.iss
;        -> packaging\Output\silb-<v>-setup.exe
;      Lleva el build CPU y el build VULKAN (el mismo binario vale para AMD e
;      Intel). Si al instalar se detecta una GPU NVIDIA, ofrece descargar el
;      runtime CUDA (silb-cuda-runtime-12.exe, ~421 MB) y el pack con el exe
;      (silb-cuda-pack-<v>.exe, ~30 MB), los verifica con SHA-256 y los aplica en
;      silencio. El runtime solo se baja la PRIMERA vez: si {app} ya tiene las
;      DLL, no se pide. Sin GPU, sin red, hash que no cuadra o el usuario dice que
;      no -> instala en CPU y sigue: nunca aborta.
;
;   2. OFFLINE (las tres versiones dentro, ~475 MB):
;      ISCC.exe /DMyOffline /DWHISPR_VERSION=0.9.0 packaging\silb.iss
;        -> packaging\Output\silb-<v>-offline-setup.exe
;
;   3. RUNTIME CUDA (/DMyRuntime, ~421 MB): solo las tres DLL del runtime, en un
;      asset FIJO colgado del release cuda-runtime-12. Se sube UNA vez por versión
;      mayor de CUDA: 772 MB de DLL que no cambian entre releases de Silb.
;      ISCC.exe /DMyRuntime packaging\silb.iss
;        -> packaging\Output\silb-cuda-runtime-12.exe
;
;   4. PACK CUDA (/DMyPack, ~30 MB): solo [Files], se ejecuta por encima de una
;      instalación ya hecha y reemplaza el binario. Las DLL las pone el runtime.
;      No crea desinstalador, ni iconos, ni accesos directos.
;      ISCC.exe /DMyPack /DWHISPR_VERSION=0.9.0 packaging\silb.iss
;        -> packaging\Output\silb-cuda-pack-<v>.exe
;
; Insumos (en packaging\build\, ninguno va a git):
;   bash packaging/build-cuda-runtime.sh   -> build\cuda12\ (cudart, cublas, cublasLt)
;   build\silb-cpu.exe                     -> cargo build --release
;   build\silb-cuda.exe                    -> cargo build --release --features cuda
;   build\silb-vulkan.exe                  -> cargo build --release --features vulkan
;                                             (VULKAN_SDK=... CARGO_TARGET_DIR=E:\vk,
;                                              target corto: cmake de ggml-vulkan
;                                              revienta con FTK1011 si la ruta crece)
; packaging\build-installers.sh encadena todo: pack -> .sha256 -> instalador
; online con /DPACKURL y /DPACKSHA256 ya incrustados.
;
; QUÉ BINARIO QUEDA INSTALADO (lo decide UseCuda/UseVulkan, [Code]):
;   GPU NVIDIA      -> silb-cuda.exe + runtime CUDA 12 (30 MB + 421 MB la 1ª vez)
;   GPU AMD o Intel -> silb-vulkan.exe (dentro del instalador, 0 descargas)
;   sin GPU / /FORCECPU -> silb-cpu.exe
; El exe Vulkan es autónomo (solo necesita vulkan-1.dll del driver) y uno mismo
; sirve para AMD e Intel; por eso NO hay pack Vulkan.
;
; DETECCIÓN DE GPU (function ProbeGpu, [Code]; una sola consulta cacheada):
;   1. WMI Win32_VideoController -> nombre del adaptador de vídeo. Se clasifica:
;      NVIDIA -> CUDA; AMD / RADEON / ADVANCED MICRO DEVICES / INTEL / ARC /
;      IRIS / UHD GRAPHICS -> Vulkan; nada -> CPU.
;   2. Si WMI no dice nada, nvidia-smi.exe --query-gpu=name (código 0) -> CUDA.
;   3. Portátiles híbridos: con Intel + NVIDIA gana NVIDIA (CUDA); con
;      Intel + Radeon gana la Radeon (Vulkan).
;
; PARÁMETROS DE PRUEBA:
;   /FORCECPU    instala el build CPU aunque haya GPU (no ofrece el pack)
;   /FORCECUDA   fuerza el camino CUDA aunque no se detecte GPU
;   /FORCEVULKAN fuerza el camino Vulkan (mismo efecto que una GPU AMD/Intel)
;   /PACKURL=... /PACKSHA256=...  sobreescriben la URL y el hash incrustados
;   WHISPR_GPU_OVERRIDE="<nombre>"  variable de entorno: sustituye el nombre que
;       devuelve WMI, para probar la rama AMD en un equipo sin AMD. Se clasifica
;       igual que un nombre real, así que «AMD Radeon RX 6600» -> Vulkan y
;       «cualquier cosa» -> CPU. Sin /FORCE*: la decisión sale de la detección.
;   Ej. (equipo NVIDIA sin AMD): silb-0.9.0-setup.exe /VERYSILENT ^
;        /DIR=C:\Temp\silb WHISPR_GPU_OVERRIDE="AMD Radeon RX 6600"
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
;   WHISPR_VERSION      versión del paquete (por defecto 0.9.0)
;   WHISPR_PACK_URL / WHISPR_PACK_SHA256  pack remoto (o /DPACKURL /DPACKSHA256)

#define MyReleaseDir GetEnv("WHISPR_RELEASE_DIR") ? GetEnv("WHISPR_RELEASE_DIR") : "..\target\release"
#define MyCudaExe GetEnv("WHISPR_CUDA_EXE") ? GetEnv("WHISPR_CUDA_EXE") : "build\silb-cuda.exe"
#define MyCudaRuntime "build\cuda12"
; Un solo binario Vulkan sirve para AMD e Intel: viaja DENTRO del instalador
; (no hay pack Vulkan) y solo necesita la vulkan-1.dll que pone el driver.
#define MyVulkanExe "build\silb-vulkan.exe"
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
    #define MyAppVersion GetEnv("WHISPR_VERSION") ? GetEnv("WHISPR_VERSION") : "0.9.0"
  #endif
#endif
; Nombre del pack en el release (el mismo que produce /DMyPack).
#define MyPackBaseName "silb-cuda-pack-" + MyAppVersion + ".exe"
; Runtime CUDA 12: asset FIJO, colgado de un release propio (cuda-runtime-12) y no de
; la versión de Silb. Las tres DLL pesan 772 MB y NO cambian entre versiones, así que
; se suben una vez y cada release lleva solo el exe CUDA (~30 MB en vez de ~465).
#define MyRuntimeBaseName "silb-cuda-runtime-12.exe"
#define MyRuntimeUrl "https://github.com/JuanFrancisco-Mateo/silb/releases/download/cuda-runtime-12/" + MyRuntimeBaseName

; --- modo: /DRuntime (DLLs CUDA) · /DMyPack (exe CUDA) · /DMyOffline · nada (online)
; La presencia del define es el flag (ISPP compara mal string vs número).
#ifdef MyRuntime
  #define MyOutputBase "silb-cuda-runtime-12"
  ; Runtime y pack son «solo ficheros»: se aplican por encima de la app y no
  ; tocan el registro ni el desinstalador.
  #define MyFilesOnly
  #if (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cudart64_12.dll")) || (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cublas64_12.dll")) || (!FileExists(AddBackslash(SourcePath) + MyCudaRuntime + "\cublasLt64_12.dll"))
    #error Falta el runtime CUDA 12: bash packaging/build-cuda-runtime.sh
  #endif
#else
#ifdef MyPack
  #ifdef MyOffline
    #error /DMyPack y /DMyOffline son excluyentes
  #endif
  #define MyOutputBase "silb-cuda-pack-" + MyAppVersion
  #define MyFilesOnly
  #if !FileExists(AddBackslash(SourcePath) + MyCudaExe)
    #error El pack CUDA necesita el exe CUDA (packaging\build\silb-cuda.exe)
  #endif
#else
  #ifdef MyOffline
    #define MyOutputBase "silb-" + MyAppVersion + "-offline-setup"
    #if !FileExists(AddBackslash(SourcePath) + "build\silb-cpu.exe")
      #error El instalador offline necesita packaging\build\silb-cpu.exe
    #endif
    #if !FileExists(AddBackslash(SourcePath) + MyVulkanExe)
      #error El instalador offline necesita el exe Vulkan (packaging\build\silb-vulkan.exe)
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
    ; El exe Vulkan viaja dentro también en el online: es lo que hace que AMD
    ; funcione sin red y sin elegir nada.
    #if !FileExists(AddBackslash(SourcePath) + MyVulkanExe)
      #error El instalador online necesita el exe Vulkan (packaging\build\silb-vulkan.exe)
    #endif
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
    ; El runtime se descarga solo la primera vez (si {app} ya tiene las DLL, no se
    ; pide nada). /DRUNTIMEURL permite probarlo contra un servidor local.
    #ifdef RUNTIMEURL
      #undef MyRuntimeUrl
      #define MyRuntimeUrl RUNTIMEURL
    #endif
    #ifdef RUNTIMESHA256
      #define MyRuntimeSha256 RUNTIMESHA256
    #endif
    #ifndef MyRuntimeSha256
      #define MyRuntimeSha256 GetEnv("WHISPR_RUNTIME_SHA256") ? GetEnv("WHISPR_RUNTIME_SHA256") : ""
    #endif
    #if MyRuntimeSha256 == ""
      #error Falta el SHA-256 del runtime CUDA: /DRUNTIMESHA256=<hash> (o WHISPR_RUNTIME_SHA256)
    #endif
  #endif
#endif
#endif

[Setup]
; GUID estable del producto (cambia SOLO si cambia el nombre o el editor).
; Se MANTIENE tras el renombrado a Silb: es el mismo producto, así una
; instalación anterior se ACTUALIZA EN SITIO (Inno reutiliza el directorio del
; registro) en vez de aparecer dos veces en «Agregar o quitar programas».
#ifdef MyRuntime
; El runtime tampoco es el producto: GUID propio, sin desinstalador ni icono.
AppId={{4A7C1E20-9B3D-4F62-8A15-D2C9F0B7E314}
#else
#ifdef MyPack
; El pack NO es el producto: GUID propio y sin desinstalador, para que correrlo
; por encima de la app no le toque el registro ni el unins000.exe.
AppId={{9D3B7A54-2F6E-4C71-9A0E-1C6B5C2D8E43}
#else
AppId={{6E2A0E9C-1B5B-4F0E-9C8C-5F8F7B6A7D11}
#endif
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
#ifdef MyFilesOnly
; El pack solo reemplaza ficheros en el directorio que le pasa el instalador
; base (/DIR={app}): nada de páginas ni de desinstalador.
Uninstallable=no
DisableDirPage=yes
DisableProgramGroupPage=yes
#else
; Icono del instalador (mismo .ico que el binario).
SetupIconFile=..\assets\silb.ico
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
spanish.PackDesc=Se van a descargar los ficheros de aceleración por GPU.
spanish.PackOffer=GPU NVIDIA detectada: %1. ¿Descargar la aceleración CUDA (unos 30 MB)? La primera vez se añaden unos 421 MB de runtime CUDA, que las siguientes versiones ya aprovechan sin volver a bajarlo. Si dices que no, Silb se instala en modo CPU y funciona igual, más lento. (Las GPU AMD e Intel ya llevan su aceleración Vulkan dentro del instalador: no hay nada que descargar.)
spanish.NoGpuAviso=Sin aceleración por GPU: se puede añadir después.
english.PackCaption=Preparing GPU acceleration
english.PackDesc=Downloading the GPU acceleration files.
english.PackOffer=NVIDIA GPU detected: %1. Download the CUDA acceleration (about 30 MB)? The first time this adds about 421 MB of CUDA runtime, which later versions reuse instead of downloading it again. If you say no, Silb installs in CPU mode and works the same, just slower. (AMD and Intel GPUs already carry their Vulkan acceleration inside the installer: there is nothing to download.)
english.NoGpuAviso=Without GPU acceleration: it can be added later.

; Nada que marcar en el pack (no hay iconos ni accesos directos).
#ifndef MyFilesOnly
[Tasks]
; Acceso directo en el escritorio (marcado por defecto, el usuario lo puede quitar).
Name: "desktopicon"; Description: "Crear un acceso directo en el escritorio"; GroupDescription: "Accesos directos:"; Flags: checkedonce
#endif

[Files]
#ifdef MyRuntime
; Las tres DLL del runtime, al lado del ejecutable (Windows las resuelve del
; directorio del exe, así que el equipo NO necesita el toolkit CUDA instalado).
; Asset FIJO: se sube una vez y lo reutilizan todas las versiones de Silb.
Source: "{#MyCudaRuntime}\*.dll"; DestDir: "{app}"; Flags: ignoreversion
#else
#ifdef MyPack
; El exe CUDA encima de {app}\silb.exe. Las DLL ya las dejó el runtime.
Source: "{#MyCudaExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion
#else
#ifdef MyOffline
; Las tres versiones dentro y un solo {app}\silb.exe, elegido al instalar: CUDA
; (+ su runtime) si hay GPU NVIDIA, Vulkan si hay AMD/Intel, CPU si no hay nada.
; Los Check son excluyentes: UseCuda y UseVulkan no pueden ser ciertos a la vez.
Source: "build\silb-cpu.exe"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion; Check: not UseCuda and not UseVulkan
Source: "{#MyCudaExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion; Check: UseCuda
Source: "{#MyCudaRuntime}\*.dll"; DestDir: "{app}"; Flags: ignoreversion; Check: UseCuda
Source: "{#MyVulkanExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion; Check: UseVulkan
#else
; Online: el build CPU se copia SIEMPRE menos cuando toca Vulkan. Hace falta
; como base aunque haya GPU NVIDIA porque el pack CUDA se aplica después (en
; ssPostInstall) y, si la descarga falla, el equipo tiene que quedarse con un
; silb.exe que funcione: el CPU. Con UseVulkan el exe Vulkan lo sustituye.
Source: "{#MyReleaseDir}\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion; Check: not UseVulkan
Source: "{#MyVulkanExe}"; DestDir: "{app}"; DestName: "{#MyAppExeName}"; Flags: ignoreversion; Check: UseVulkan
#endif
#endif
#endif
; Icono que verá el acceso directo (mismo .ico que el binario, redundante
; pero Inno lo necesita para el .lnk; el .exe ya lleva el suyo).
Source: "..\assets\silb.ico"; DestDir: "{app}"; Flags: ignoreversion
; Server LLM EMBARCADO: viaja con la app para que el usuario no teclee
; ninguna ruta (la app lo busca en {app}\vendor\llama\llama-server.exe). Lo
; copia packaging\vendor-llama.sh desde el build local de llama.cpp; NO va a
; git. `cuda\` se EXCLUYE siempre: son ~656 MB de DLLs de NVIDIA (CUDA 13) que
; solo usa llama-server, y multiplicarían por cinco el instalador.
Source: "..\vendor\llama\*"; DestDir: "{app}\vendor\llama"; Excludes: "cuda"; Flags: ignoreversion recursesubdirs createallsubdirs

#ifndef MyFilesOnly
[Icons]
; Menú Inicio (siempre).
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\silb.ico"; Comment: "Dictado local por voz"
; Escritorio (opcional, según selección del usuario).
Name: "{userdesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\silb.ico"; Comment: "Dictado local por voz"; Tasks: desktopicon
; Acceso directo al desinstalador (carpeta del menú Inicio).
Name: "{group}\Desinstalar {#MyAppName}"; Filename: "{uninstallexe}"; IconFilename: "{app}\silb.ico"

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
#ifndef MyFilesOnly
[Code]
var
  GpuProbeDone: Boolean; // la detección se hace una sola vez y se cachea
  GpuName: String;       // adaptador elegido, para el mensaje final
  GpuMode: String;       // 'cuda' | 'vulkan' | 'cpu': el binario que toca
  GpuForcedCpu: Boolean; // /FORCECPU: hay GPU pero el usuario pide el build CPU
  CudaApplied: Boolean;  // quedó el binario CUDA en {app} (offline: UseCuda)
  VulkanApplied: Boolean; // quedó el binario Vulkan en {app} (siempre: UseVulkan)
#ifndef MyOffline
  PackReady: Boolean;    // el pack se descargó y su SHA-256 cuadró
  PackTried: Boolean;    // la oferta se hace una sola vez
  NeedRuntime: Boolean;  // {app} no tiene todavía las DLL del runtime CUDA
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

// Prioridad del binario según el nombre del adaptador: 3 = CUDA (NVIDIA),
// 2 = Vulkan con GPU dedicada AMD/Radeon, 1 = Vulkan con iGPU Intel,
// 0 = nada reconocible (CPU). La prioridad resuelve los portátiles híbridos:
// Intel + NVIDIA -> NVIDIA, Intel + Radeon -> Radeon (se nombra la dedicada,
// que es la que de verdad acelera). Mismo binario Vulkan para AMD e Intel.
function GpuRank(const Name: String): Integer;
var
  U: String;
begin
  Result := 0;
  U := Uppercase(Name);
  if Pos('NVIDIA', U) > 0 then
    Result := 3
  else if (Pos('AMD', U) > 0) or (Pos('RADEON', U) > 0)
    or (Pos('ADVANCED MICRO DEVICES', U) > 0) then
    Result := 2
  else if (Pos('INTEL', U) > 0) or (Pos('ARC', U) > 0) or (Pos('IRIS', U) > 0)
    or (Pos('UHD GRAPHICS', U) > 0) then
    Result := 1;
end;

// UNA pasada de WMI sobre Win32_VideoController (un portátil puede traer dos o
// tres adaptadores) y se queda con el de más prioridad. '' si WMI no está o no
// hay ninguno reconocible.
function WmiGpuName: String;
var
  Locator, Service, Items, Obj: Variant;
  I, Rank, Best: Integer;
  N: String;
begin
  Result := '';
  Best := 0;
  try
    Locator := CreateOleObject('WbemScripting.SWbemLocator');
    Service := Locator.ConnectServer('.', 'root\cimv2');
    Items := Service.ExecQuery('SELECT Name FROM Win32_VideoController');
    for I := 0 to Items.Count - 1 do
    begin
      // SWbemObjectSet.Item() espera la ruta del objeto, no un índice:
      // para recorrer por número está ItemIndex().
      Obj := Items.ItemIndex(I);
      // Propiedad Variant -> String antes de Trim(): Pascal Script no acepta
      // Variant como argumento de Trim.
      N := Obj.Name;
      N := Trim(N);
      Rank := GpuRank(N);
      if Rank > Best then
      begin
        Best := Rank;
        Result := N;
      end;
    end;
  except
    Log('Silb: WMI no disponible para detectar la GPU: ' + GetExceptionMessage);
  end;
end;

// Decisión única: qué binario toca en este equipo. La consultan [Files] (una
// vez por fichero), la oferta del pack (online) y CurStepChanged; se cachea
// porque WMI y nvidia-smi tardan lo suyo.
procedure ProbeGpu;
var
  N, Ovr: String;
  Rank: Integer;
begin
  if GpuProbeDone then
    exit;
  GpuProbeDone := True;
  GpuMode := 'cpu';

  if ParamOn('FORCECPU') then
  begin
    GpuForcedCpu := True;
    Log('Silb: /FORCECPU, se instala el build CPU');
    exit;
  end;
  if ParamOn('FORCECUDA') then
  begin
    GpuMode := 'cuda';
    GpuName := WmiGpuName;
    if GpuName = '' then
      GpuName := 'GPU forzada con /FORCECUDA';
    Log('Silb: /FORCECUDA, se instala el build CUDA');
    exit;
  end;
  if ParamOn('FORCEVULKAN') then
  begin
    GpuMode := 'vulkan';
    GpuName := WmiGpuName;
    if GpuName = '' then
      GpuName := 'GPU forzada con /FORCEVULKAN';
    Log('Silb: /FORCEVULKAN, se instala el build Vulkan');
    exit;
  end;

  // Gancho de prueba: sustituye el nombre de WMI para poder ejercitar la rama
  // AMD en un equipo que no la tiene. Se clasifica igual que un nombre real,
  // así que un texto cualquiera acaba en CPU (que también es probarlo).
  Ovr := GetEnv('WHISPR_GPU_OVERRIDE');
  if Ovr <> '' then
  begin
    N := Ovr;
    Log('Silb: WHISPR_GPU_OVERRIDE=' + N + ' (no se consulta WMI)');
  end
  else
    N := WmiGpuName;

  Rank := GpuRank(N);
  if (Rank = 0) and (Ovr = '') and NvidiaSmiOk then
  begin
    // WMI puede estar roto o no exponer el adaptador y el driver sí saberlo.
    N := 'GPU NVIDIA';
    Rank := 3;
  end;

  if Rank = 3 then
  begin
    GpuMode := 'cuda';
    GpuName := N;
    Log('Silb: GPU NVIDIA detectada (' + GpuName + ') -> build CUDA');
  end
  else if Rank > 0 then
  begin
    GpuMode := 'vulkan';
    GpuName := N;
    Log('Silb: GPU AMD/Intel detectada (' + GpuName + ') -> build Vulkan');
  end
  else
    Log('Silb: no se detecta GPU compatible; se instala el build CPU');
end;

// Los dos predicados que usan [Files], PrepareToInstall y CurStepChanged.
// Son excluyentes por construcción (GpuMode es un solo valor).
function UseCuda: Boolean;
begin
  ProbeGpu;
  Result := GpuMode = 'cuda';
end;

function UseVulkan: Boolean;
begin
  ProbeGpu;
  Result := GpuMode = 'vulkan';
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

// El runtime es un asset FIJO: sus 772 MB de DLL no cambian entre versiones, así que
// se descarga una sola vez (release cuda-runtime-12) y las actualizaciones de Silb no
// lo vuelven a pedir. Se aplica antes que el pack porque el exe CUDA lo necesita.
function ApplyRuntime: Boolean;
var
  Rt: String;
  Code: Integer;
begin
  Code := -1;
  Rt := ExpandConstant('{tmp}\') + '{#MyRuntimeBaseName}';
  Result := Exec(Rt, '/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR="'
      + ExpandConstant('{app}') + '"', '', SW_HIDE, ewWaitUntilTerminated, Code)
    and (Code = 0)
    and FileExists(ExpandConstant('{app}\cublasLt64_12.dll'));
  Log(Format('Silb: runtime CUDA salida=%d, aplicado=%s', [Code, IntToStr(Ord(Result))]));
end;

// Sin runtime no se aplica el exe CUDA: quedaría un binario CUDA sin sus DLL y la
// app no arrancaría. En ese caso la instalación se queda en CPU, que funciona.
function ApplyCuda: Boolean;
begin
  Result := False;
  if not PackReady then
    exit;
  if NeedRuntime then
    if not ApplyRuntime then
      exit;
  Result := ApplyPack;
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
  Url, Sha, RuntimeUrl, RuntimeSha: String;
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
  // El runtime solo se pide si {app} no lo tiene ya: al actualizar desde una versión
  // anterior las DLL están puestas y no se bajan los ~414 MB otra vez.
  NeedRuntime := not FileExists(ExpandConstant('{app}\cublasLt64_12.dll'));
  DownloadPage.Clear;
  if NeedRuntime then
  begin
    RuntimeUrl := ParamValue('RUNTIMEURL', '{#MyRuntimeUrl}');
    RuntimeSha := ParamValue('RUNTIMESHA256', '{#MyRuntimeSha256}');
    Log('Silb: descargando el runtime CUDA de ' + RuntimeUrl + ' (sha256 ' + RuntimeSha + ')');
    DownloadPage.Add(RuntimeUrl, '{#MyRuntimeBaseName}', RuntimeSha);
  end
  else
    Log('Silb: el runtime CUDA ya está en {app}; no se descarga');
  Log('Silb: descargando el pack CUDA de ' + Url + ' (sha256 ' + Sha + ')');
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
  Lineas: TArrayOfString;
begin
  if CurStep <> ssPostInstall then
    exit;
#ifdef MyOffline
  CudaApplied := UseCuda;
  VulkanApplied := UseVulkan;
#else
  // El pack se aplica aquí: ya están los ficheros de [Files] en {app}. El exe
  // Vulkan, si tocaba, lo dejó [Files] al copiar el instalador.
  VulkanApplied := UseVulkan;
  CudaApplied := ApplyCuda;
#endif
  if CudaApplied then
  begin
    Backend := 'cuda';
    Aviso := 'Aceleración por GPU: activada (' + GpuName + ')';
  end
  else if VulkanApplied then
  begin
    Backend := 'vulkan';
    Aviso := 'Aceleración por GPU: activada (Vulkan, ' + GpuName + ')';
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
    Aviso := 'No se detecta GPU NVIDIA ni AMD/Intel: modo CPU';
#else
    // Fallo del pack o renuncia del usuario: el mensaje dice explícitamente que
    // la app funciona y que la GPU se puede añadir luego con el mismo pack.
    Aviso := CustomMessage('NoGpuAviso');
#endif
  end;
  // backend.txt: qué binario quedó instalado. En silencioso es la única
  // constancia que ve quien automatiza la instalación.
  // SaveStringToFile es ANSI y rompía los acentos («Aceleraci?n»); se escribe
  // en UTF-8 sin BOM, que es lo que abre cualquier editor de hoy.
  SetArrayLength(Lineas, 3);
  Lineas[0] := 'backend=' + Backend;
  Lineas[1] := 'gpu=' + GpuName;
  Lineas[2] := 'aviso=' + Aviso;
  SaveStringsToUTF8FileWithoutBOM(ExpandConstant('{app}\backend.txt'), Lineas, False);
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
