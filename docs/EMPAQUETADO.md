# EMPAQUETADO Y DISTRIBUCIÓN - especificación

> Las medidas de las tablas del [apartado 7](EMPAQUETADO.html#7-pipeline-de-release-pasos-exactos-para-r6-11092026) y del [apartado 8](EMPAQUETADO.html#8-medidas-de-ligereza-reales-r6-11092026)
> se tomaron con una versión anterior de la app y no se han vuelto a medir.

Objetivo: convertir `target/release/silb.exe` en algo que se pueda instalar y que
dé confianza en Windows 10/11.

Estado al cerrar la **0.3.0 (11/09/2026)**: instalador Inno Setup
que funciona (objetivo ≤25 MiB, sin admin, `%LOCALAPPDATA%\Programs\Silb`), icono
`.ico` en varios tamaños (16/24/32/48/64/128/256, color `#7C6CFF`, fondo
transparente) generado por código y metido dentro del `.exe` y en los accesos
directos, y la versión portable = el propio `.exe` release (sin instalador). Las
medidas reales de ligereza están abajo ([apartado 8](EMPAQUETADO.html#8-medidas-de-ligereza-reales-r6-11092026)).

**Cambios en la 0.3.0:** subida de versión 0.2.0 → 0.3.0 (cierre de la etapa
con la personalización completa ya dentro del binario final: sección Imágenes en
Ajustes, colores editables, animación por frames). Medidas reales actualizadas en el
[apartado 8.4](EMPAQUETADO.html#84-decisión-sobre-prerroll-y-carga-del-modelo).

---

## 1. Formas de distribución (de más simple a más completa)

### 1.1 Portable `.exe` (recomendado empezar aquí)

Un único ejecutable que lleva todo dentro, sin instalación.

- Qué hace falta: meter el **icono** dentro del binario (recursos de Windows,
  ver [apartado 4](EMPAQUETADO.html#4-versionado-semver)) y empaquetar cualquier DLL de runtime que falte.
- Cómo: `cargo build --release` ya genera un binario estático salvo las DLL del
  runtime de MSVC y las de CUDA/whisper. Comprueba con `dumpbin`/`llvm-nm` qué
  DLL necesita y decide si se enlazan dentro o se reparten aparte (una carpeta
  junto al `.exe`).
- Estado R6: **el `.exe` se reparte tal cual como portable**. Se han medido
  176 MB de RAM en reposo (sin modelo), 486 MB tras dejar cargado el modelo
  `small`, y 1.213 MB de memoria privada (ver [apartado 8](EMPAQUETADO.html#8-medidas-de-ligereza-reales-r6-11092026)). El `.exe` lleva el icono
  `.ico` dentro y arranca sin pedir permisos de administrador ni nada más allá
  del runtime de MSVC.
- Ventajas: sin pedir permisos elevados, ideal para probar y para "probar a
  mano". Desventajas: sin asociación de archivos, sin integración con el menú
  Inicio, sin actualizarse sola.

### 1.2 Instalador (Inno Setup)

**Recomendación:** Inno Setup (v6.7.3+). Cubre: instalar en
`%LOCALAPPDATA%\Programs\Silb\` (sin admin), accesos directos (menú Inicio +
escritorio), desinstalador, y conexión con `SignTool` para firmar en el mismo
proceso.

Qué lleva el instalador:

- `silb.exe` (+ las DLL necesarias, si las hubiera; el binario actual no necesita
  runtime externo porque MSVC va enlazado dentro).
- Icono `.ico` (se usa también para el acceso directo; el `.exe` ya lo lleva
  dentro, así que para `IconFilename` es redundante).
- Licencia (`LICENSE`, MIT) en la pantalla de bienvenida (`InfoBeforeFile`).
- Entrada en "Agregar o quitar programas" (nombre, versión, editor, icono).

#### 1.2.1 Medidas reales del instalador (R6, 11/09/2026)

| Artefacto | Tamaño |
|---|---|
| `target/release/silb.exe` (sin icono) | 44.226.048 B (42,18 MiB) |
| `target/release/silb.exe` (con icono incrustado) | 44.854.272 B (42,77 MiB, +628 KB) |
| `target/release/silb.exe` (tras Personalización I+II+III, feat `image` activada) | 47.178.240 B (44,99 MiB) |
| `packaging/Output/silb-0.2.0-setup.exe` (R6 inicial, feat `image` desactivada) | 23.769.034 B (22,67 MiB) |
| `packaging/Output/silb-0.2.0-setup.exe` (tras Personalización III, feat `image` activa) | 24.332.697 B (23,21 MiB) |
| `packaging/Output/silb-0.3.0-setup.exe` (0.3.0, con personalización completa) | (medido en el [apartado 8.4](EMPAQUETADO.html#84-decisión-sobre-prerroll-y-carga-del-modelo)) |

La compresión LZMA2/ultra64 baja el binario de ~45 MiB a ~23 MiB (~52 % del
original). Objetivo ≤25 MiB: **CUMPLIDO** (1,79 MiB de margen incluso con la
función `image` activada tras la Personalización III).

### 1.3 MSIX

Paquete moderno de Windows (instalación limpia, actualizaciones por diferencias,
caja aislada). Exige **firma de código** obligatoria y un manifiesto
`AppxManifest.xml`.

- Herramientas: `MakeAppx.exe`, `SignTool.exe` (Windows SDK), o
  `cargo-msix`/`MSIX Packaging Tool`.
- Cuándo: cuando se quiera repartir por la Microsoft Store o que el sistema
  gestione las actualizaciones. No es prioridad para la v1 (la firma es un
  requisito duro y caro de arrancar).

---

## 2. Firma de código

Hace falta para que Windows SmartScreen no enseñe el aviso de "editor
desconocido" y para MSIX.

- **Certificado**: uno de firma de código (OV u EV) de una autoridad (DigiCert,
  Sectigo, SSL.com), o uno autofirmado para pruebas internas.
  - EV da buena reputación desde el primer día en SmartScreen; OV la va
    acumulando con el tiempo. Para empezar, un certificado de prueba local sirve
    para validar el proceso.
- **Herramienta**: `SignTool.exe sign /fd SHA256 /a /tr http://timestamp.digicert.com ...`.
- **Dónde firmar**: en el sistema de integración, con el `.pfx` guardado como
  secreto (nunca en el repo). Hay que firmar **tanto el `.exe` portable como el
  instalador** (Inno permite firmar el `Setup.exe` al final con la directiva
  `SignTool` en la sección `[Setup]`, por ejemplo:
  `SignTool=ByParam /f $qcertfile /p $qcertpass /fd SHA256 /tr http://timestamp.digicert.com $f`).
- **Windows te va a frenar**: sin firma, SmartScreen saca "Windows protected your
  PC" y hay que pulsar "Más información → Ejecutar de todos modos". Con OV/EV
  firmado, el aviso desaparece a medida que se gana reputación.

### Estado R6

**Sin firma** en el proceso actual - se activará cuando haya un certificado
OV/EV. El `.iss` está listo para añadir la directiva `SignTool` en cuanto llegue
el `.pfx`. NO se ha simulado ni inventado ningún certificado.

---

## 3. Icono

- **Generado siempre igual** por código en `assets/build_icon.py`. Sin
  dependencias externas (solo `struct` y `zlib` de la biblioteca estándar de
  Python).
- Formato: BMP DIB dentro de `.ico` (la máxima compatibilidad con el cargador de
  recursos de Windows - el formato PNG-in-ICO lo rechazan algunas versiones de
  `rc.exe`).
- Tamaños: **16, 24, 32, 48, 64, 128, 256 px** (el de 256 es obligatorio para
  Windows 11). Color del acento `#7C6CFF` (el mismo del icono de la bandeja
  `src/tray.rs` y del tema de la app `src/theme.rs`).
- Fondo transparente: BMP DIB con transparencia en el canal A del XOR (sin
  premultiplicar, sin máscara AND).
- **Comprobación**: `scripts/verify-ico.py` lee la cabecera ICONDIR y decodifica
  la entrada mayor. Hay un test de Rust equivalente en
  `src/icon_header.rs::tests::icon_header_and_payload_are_valid` (entra en
  `cargo test --lib`).
- **Cómo se mete**: `build.rs` (dependencia de compilación `winresource = "0.1"`).
  El `.ico` está versionado en `assets/silb.ico` para que la compilación no
  necesite Python (el script solo se ejecuta si falta el `.ico`).
- **Manifiesto**: el `build.rs` usa `set_icon` pero **no** `set_manifest`: el
  compilador de MSVC genera solo el manifiesto SxS del CRT
  (`vcruntime140_app.dll`, etc.). Si lo sobreescribes, la app no arranca con el
  error SxS 14001 (comprobado el 11/09/2026). El escalado de pantalla se queda en
  "DPI Unaware" por ahora (se respeta el escalado de Windows; si algún día se
  quiere Per-Monitor V2, se añadirá un `.rc` aparte con un recurso neutral (24),
  no un manifiesto).

---

## 4. Versionado (SemVer)

- Esquema: `MAJOR.MINOR.PATCH`. Actual: **0.8.1** (`Cargo.toml`).
  - `0.x`: la interfaz interna todavía cambia; cada función nueva sube MINOR y
    los arreglos PATCH.
  - Se pasa a `1.0.0` cuando la app tenga el recorrido completo (corrección por
    voz incluida) y una estabilidad de formato demostrada.
- Un único sitio manda: la `version` de `Cargo.toml`. El instalador lee la
  versión de la variable de entorno `WHISPR_VERSION` (o del define
  `/DWHISPR_VERSION=`) y, si no hay ninguna, usa la que trae
  `packaging/silb.iss` en `MyAppVersion` (hoy `0.8.0`). Al subir de
  versión se cambian `Cargo.toml`, la entrada `whispr-open` de `Cargo.lock` y
  ese define.
- Convención de commits (opcional): prefijos `feat:` / `fix:` / `docs:` /
  `chore:` (ya se usan en el repo) para sacar el CHANGELOG y la subida de
  versión.

---

## 5. Actualizaciones

Opciones, de menos a más trabajo:

1. **Nada automático (v1)**: el usuario se baja el instalador nuevo a mano.
   Suficiente mientras haya pocos usuarios.
2. **Comprobación de versión** (barato): al arrancar, preguntar a una dirección
   fija (por ejemplo, la release más reciente de GitHub) y avisar "hay versión
   nueva" con un enlace. Sin descarga automática.
3. **Actualización automática completa**: 
   - Portable: algo tipo `self_update`/`sparkle` (reemplazar el binario y volver
     a lanzarlo). `self_replace` + un descargador.
   - Instalador Inno: Inno Download Plugin, o descargar el `Setup.exe` y
     relanzarlo.
   - MSIX: lo actualiza el propio sistema.

Recomendación: 2 → 3 en ese orden. Las releases de GitHub (tags + ficheros del
instalador) son el canal natural para la v1.

---

## 6. Paquetes / ediciones futuras

- **Edición única (v1)**: una sola distribución local completa. La app es privada
  por diseño ("nada sale de tu PC"); una única edición refuerza el mensaje.
- **Ediciones opcionales futuras** (solo si hay demanda):
  - *Lite*: sin la IA correctora dentro (usa la corrección por reglas), binario
    más pequeño para equipos sin GPU.
  - *Pro*: notetaker de reuniones, identificación de hablantes, nube opcional.
  - *Portable* vs *Instalada* no son "ediciones": son dos formas de repartir la
    misma edición.
- Cualquier edición de pago obligaría a cambiar de licencia; hoy la app es MIT.

---

## 7. Pipeline de release (pasos exactos para R6, 11/09/2026)

### 7.1 Build del binario release (con icono incrustado)

```cmd
:: 1) Build release. El build.rs incrusta el icono vía winresource.
::    Si el .ico falta, lo regenera con assets/build_icon.py.
::    No hace falta llamar a Python a mano.
cd /d silb
cargo build --release

:: Salida: target\release\silb.exe (≈42,77 MiB con icono)
```

El manifiesto SxS del CRT lo genera el propio compilador de MSVC (no
sobreescribir).

### 7.2 Build del instalador Inno Setup

Inno Setup 6.7.3 está instalado por usuario en
`%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe`; el script es
`packaging\silb.iss`. La versión sale de `WHISPR_VERSION` (define
`/DWHISPR_VERSION=` o variable de entorno) y, si no se pasa ninguna, del valor
por defecto del script (`MyAppVersion`, hoy **0.8.0**). Al subir de versión se
cambian `Cargo.toml`, `Cargo.lock` y ese define.

#### Tres artefactos, dos descargas (0.8.0)

El usuario **elige entre dos descargas y las dos instalan la misma app**; lo que
cambia es de dónde sale la parte de CUDA, no lo que se instala. Un tercer
fichero no se elige: lo baja el instalador cuando hace falta.

| Fichero | Quién lo baja | Lleva | Comprimido (lzma2/ultra64) |
|---|---|---|---|
| `silb-<v>-setup.exe` | **el normal**: cualquiera con internet | build CPU + `vendor\llama\` **sin** `cuda\` | **15.425.426 B (14,71 MiB)** |
| `silb-<v>-offline-setup.exe` | equipos sin red | build CPU + build CUDA + runtime CUDA 12 + `vendor\llama\` | **466.136.198 B (444,54 MiB)** |
| `silb-cuda-pack-<v>.exe` | **no lo elige nadie**: asset del release que baja el online | build CUDA + runtime CUDA 12 (3 DLL) | **464.216.383 B (442,71 MiB)** |

Medido el 04/11/2026 con el `ISCC` de Inno 6.7.3 (el pack sale con su
`silb-cuda-pack-<v>.sha256` al lado, `sha256 = 59ce44bd…5e0377`). Los tres
llevan `ProductVersion = 0.8.0` estampado.

Lo decide la **presencia de un define** (ISPP no compara bien texto con número):

```cmd
:: 1) ONLINE (sin defines): el que se publica, ~15 MB. El pack se baja al instalar
::    si hay GPU NVIDIA; URL y hash van incrustados (/DPACKURL, /DPACKSHA256).
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /Q packaging\silb.iss

:: 2) OFFLINE (/DMyOffline): las dos versiones dentro, ~466 MB, sin red.
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /Q /DMyOffline packaging\silb.iss

:: 3) PACK (/DMyPack): solo [Files] (binario CUDA + 3 DLL), se aplica encima de
::    una instalación ya hecha y NO crea desinstalador ni accesos directos.
"%LOCALAPPDATA%\Programs\Inno Setup 6\ISCC.exe" /Q /DMyPack packaging\silb.iss
```

Los tres se encadenan con `bash packaging/build-installers.sh [version]`: pack →
`.sha256` → instalador online con `/DPACKURL` y `/DPACKSHA256` ya puestos. **Si
se reconstruye el pack, hay que recompilar el online**: la URL y el hash van
dentro y un hash viejo da "El hash del archivo es incorrecto" y cae a CPU.

**Qué hace el online con la GPU** (`UseCuda`, en `[Code]`):

1. `nvidia-smi.exe --query-gpu=name --format=csv,noheader` con código de salida
   0 (se invoca con `{sysnative}`: `{sys}` es SysWOW64 y ahí no está). Si no,
   WMI `Win32_VideoController` con "NVIDIA" en el nombre. **No se exige VRAM ni
   runtime CUDA instalado**: el runtime viaja en el pack.
2. Con GPU pide confirmación y baja `silb-cuda-pack-<v>.exe` del release con
   `TDownloadWizardPage`; el SHA-256 es **obligatorio**.
3. Aplica el pack en silencio por encima de `{app}`: `silb.exe` CUDA + las tres
   DLL del runtime al lado, que es de donde Windows las resuelve.
4. **Nunca aborta.** Sin red, 404, hash que no cuadra o el usuario dice que no →
   instala en CPU, escribe `backend.txt` con `backend=cpu` y avisa "Sin
   aceleración por GPU: se puede añadir después." (en instalación silenciosa
   eso es lo único que se ve, junto con `gpu=` y `aviso=`).

El desinstalador limpia las tres DLL del pack con `[UninstallDelete]`: sin eso
quedaban 772 MB en `{app}` (el instalador online no las conoce como suyas).

**Por qué el pack pesa 442 MiB y no ~100.** El binario CUDA importa solo dos
DLL (`cudart64_12.dll` 583.680 B y `cublas64_12.dll` 102.518.272 B), pero
`cublas64_12.dll` importa **estáticamente** `cublasLt64_12.dll` (668.669.952 B):
sin esa tercera el proceso no llega a arrancar en un equipo sin el kit CUDA en
el PATH. La cuenta que daba "~100 MB" contaba dos DLL; con las tres el pack son
465 MB sin comprimir y **442,71 MiB** comprimido. Las tres las copia
`bash packaging/build-cuda-runtime.sh` a `packaging\build\cuda12\` (idempotente,
verifica tamaño y sha256; no va a git).

**Licencia del runtime.** Las tres DLL son redistribuibles: el *CUDA Toolkit
Supplement to Software License Agreement for NVIDIA Software Development Kits*
lo dice en el apartado 2.2 "Distribution" ("The portions of the SDK that are distributable
under the Agreement are listed in Attachment A") y en el apartado 2.6 "Attachment A", que
incluye `cudart.dll` y `cublas.dll` / `cublasLt.dll` "including certain
variations of these files that have version number or architecture specific
information embedded in the file name" - o sea `cudart64_12.dll`,
`cublas64_12.dll` y `cublasLt64_12.dll`. Texto en `docs.nvidia.com/cuda/eula/`
(`index.html`, apartados 2.2 y 2.6); se cita tal cual, sin copiar la URL del PDF.

**Qué compilar antes (el online empaqueta `target/release/silb.exe`).** El
online y el offline toman el build CPU de `packaging\build\silb-cpu.exe` y el
CUDA de `packaging\build\silb-cuda.exe` (o `WHISPR_CUDA_EXE`). `--features cuda`
deja el exe CUDA en `target/release`, así que el último `cargo build --release`
tiene que ser el de CPU antes de copiar el CUDA aparte: si no, el instalador
sale con un binario que necesita el runtime 12 y `cuda\`. El tamaño lo delata
(23,6 MB CPU frente a 49,8 MB CUDA).

**Actualizar encima de una instalación CUDA: `cuda\` se queda a propósito.** Si
alguien que instaló el pack se pasa al instalador online (o al offline en un
equipo que ya no se considera GPU), el exe CUDA se sustituye por el de CPU y
`backend.txt` pasa a `cpu`, pero los 656 MB de `vendor\llama\cuda\` siguen
en disco y el servidor de IA los sigue usando con la GPU (`src/llm.rs`,
`vendored_cuda_dir_in`), que es lo que esa persona quería al instalar el pack.
Borrarlos desde el instalador sería quitarle una aceleración que ya tenía. Las
tres DLL del runtime sí se quedan también (son las que hacen arrancar al exe
CUDA si vuelve a ponerse); el desinstalador limpia todo, DLLs incluidas
(`[UninstallDelete]`), así que no se pierde nada: es espacio que se recupera al
desinstalar.

**Actualizar con Silb abierto.** Inno usa `CloseApplications` (por defecto
`yes`) y el Restart Manager de Windows: en modo interactivo se para en
"Preparando la instalación", lista `silb.exe` y pide cerrarla; en silencioso la
cierra solo (`/CLOSEAPPLICATIONS`). Si te niegas, el instalador no puede
reemplazar `silb.exe` y termina con error. Silb no llama a
`RegisterApplicationRestart`, así que el Restart Manager no la vuelve a abrir: lo
hace la entrada `[Run]` del final (con `skipifsilent`, o sea, no en instalaciones
silenciosas).

El instalador **no pide permisos de administrador** (`PrivilegesRequired=lowest`),
instala en `%LOCALAPPDATA%\Programs\Silb`, apunta el desinstalador en
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{AppId}_is1` y enseña
la licencia MIT antes de pedir confirmación.

### 7.3 Smoke test (instalar + arrancar + desinstalar)

```cmd
:: Instalar (silencioso, sin wizard). Con el .iss de la 0.8.0 el online necesita
:: el hash del pack: /DPACKSHA256=<hash de silb-cuda-pack-<v>.exe>.
packaging\Output\silb-0.8.0-setup.exe /SP- /S /SILENT /SUPPRESSMSGBOXES /NORESTART

:: Verificar que el icono está incrustado:
powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; [System.Drawing.Icon]::ExtractAssociatedIcon('%LOCALAPPDATA%\Programs\Silb\silb.exe')"

:: Medir RAM en reposo y tras el prerroll del modelo (muestreo cada 10 s):
powershell -NoProfile -Command "Get-Process silb | Select WorkingSet64,PrivateMemorySize64"

:: Desinstalar (silencioso, preserva config y modelos):
"%LOCALAPPDATA%\Programs\Silb\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

---

## 8. Medidas de ligereza (reales, R6, 11/09/2026)

### 8.1 Tamaño del binario (release, perfil optimizado)

Medido el 11/09/2026 recompilando el binario con cada variante y comparando el
tamaño con el de la base.

| Variante | Tamaño | Δ vs base | % |
|---|---:|---:|---:|
| Base (`lto=true, strip=true`) | 44.854.272 B (42,77 MiB) | - | - |
| + `opt-level="z"` | 39.932.928 B (38,08 MiB) | -4.921.344 B | -11,0 % |
| + `panic="abort"` | 40.923.648 B (39,03 MiB) | -3.930.624 B | -8,8 % |
| TODO (`cgu=1` + `opt-level=z` + `panic=abort`) | 36.868.096 B (35,16 MiB) | -7.986.176 B | -17,8 % |

El perfil que **se queda en el repo** (`Cargo.toml`) es el base
(`lto=true, strip=true`): cumple el objetivo de tamaño del instalador (≤25 MiB,
ver [apartado 1.2.1](EMPAQUETADO.html#121-medidas-reales-del-instalador-r6-11092026)) sin perder velocidad. Cualquiera de las otras variantes se puede
activar poniendo ajustes de `[profile.release]` en un `~/.cargo/config.toml` o
con `--config <ruta>` en cada compilación.

### 8.2 Decisión: solo `lto=true` + `strip=true`

Motivos (medidos el 11/09/2026):

1. **El binario ya cumple el objetivo de tamaño** (44,99 MiB con la función
   `image` activa tras la Personalización III; instalador 23,21 MiB tras LZMA,
   ver [apartado 1.2.1](EMPAQUETADO.html#121-medidas-reales-del-instalador-r6-11092026)).
2. **`opt-level="z"` ahorra 4,92 MiB (-11,0 %)** - pero penaliza la velocidad: la
   app va sobrada de GPU al transcribir (whisper.cpp + CUDA) y justa de CPU al
   dibujar (Iced 0.13); perder un 11 % de velocidad por 4 MiB no compensa.
3. **`panic="abort"` ahorra 3,93 MiB (-8,8 %)** - pero rompe el
   `catch_unwind` que Iced/winit pueden usar para sobrevivir a un fallo interno
   (la app actual no lo hace a propósito, pero lo normal en Rust es mantener
   `unwind` para no cambiar cómo se comportan los fallos de la biblioteca
   estándar).
4. **TODO (`cgu=1 + z + abort`) ahorraría 7,99 MiB (-17,8 %)** - solo útil si
   algún día el binario pasa de ~50 MiB (por ejemplo, si CUDA se enlaza de
   verdad dentro). Hoy no compensa.

Si en algún momento el instalador pasa de 30 MiB, se vuelve a mirar si activar
`opt-level="z"` o `panic="abort"`. Hoy, no compensa.

### 8.3 RAM en reposo y tras prerroll del modelo

Medido con `Get-Process` sobre Windows 11 24H2, modelo `small`
(ggml-small.bin, 462 MB en disco), muestreando durante 65 s:

| Tiempo | WorkingSet (MB) | PrivateMemory (MB) |
|---|---:|---:|
| t=+5 s (pre-prerroll) | 176 | 257 |
| t=+15 s (cargando whisper) | 435 | 1.115 |
| t=+35 s (cargando whisper) | 495 | 1.214 |
| t=+55 s (prerroll listo) | **486** | **1.213** |
| t=+65 s (estable) | **486** | **1.213** |

- **En reposo sin modelo**: ~176 MB WorkingSet, ~257 MB Private.
- **Tras dejar cargado `small`**: ~486 MB WorkingSet, ~1.213 MB Private (el
  modelo `small` ocupa ~310 MB más en RAM).
- Con el modelo `tiny` (≈75 MB en disco) el salto es mucho menor (~80 MB más).
  Con `base` (≈140 MB) el salto es de ~140 MB.

Para repetir el muestreo: lanza el `.exe`, espera unos segundos y mira
WorkingSet y Private en el Administrador de tareas (o con `tasklist`).

### 8.4 Decisión sobre prerroll y carga del modelo

Dejar el modelo cargado al arrancar (en un hilo de fondo) **se mantiene**: el
coste de 1-2 s en la primera pulsación del atajo se paga al empezar la sesión, no
a mitad del dictado. Si no quieres ese gasto de RAM, basta cambiar el modelo a
`tiny` en Ajustes → Modelo (~75 MB en disco, +80 MB en RAM).

### 8.5 Medidas reales de la 0.3.0 (11/09/2026)

Cierre de la fase 5 con la subida de versión 0.2.0 → 0.3.0 y el binario final con
la personalización completa dentro (Ajustes por secciones + colores + imágenes y
animación por frames).

| Artefacto | Tamaño (B) | Tamaño (MiB) |
|---|---:|---:|
| `target/release/silb.exe` (v0.3.0) | **47.177.728** | **44,99** |
| `packaging/Output/silb-0.3.0-setup.exe` | **24.331.612** | **23,20** |
| `assets/silb.ico` (16/24/32/48/64/128/256) | 372.526 | 363,79 KiB |

**Δ vs R6:** el `.exe` es idéntico byte a byte (-0 B; el cambio de versión solo
afecta al texto `version` de `Cargo.toml`). El instalador baja -1.085 B
(≈ -0,004 %) por el mismo motivo. **Objetivo ≤25 MiB del instalador:**
CUMPLIDO (1,80 MiB de margen).

**RAM tras dejar cargado `small`** (medido el 11/09/2026 con
`psapi.GetProcessMemoryInfo`): WorkingSet 484,6 MiB, PrivateMemory 1.243,9 MiB.
Coherente con la medición de R6 (486 / 1.213 MiB).

**Prueba rápida del exe (`target/release/silb.exe` directo, sin instalar):**
- El proceso arranca; el registro escribe `Silb <versión> iniciando` en `%APPDATA%\whispr-open\whispr-open.log`.
- Whisper queda cargado en ~1 s; la bandeja está lista; la ventana principal registrada (id=2); burbuja aplicada noact+pos+tamaño 72.
- `taskkill /T /PID <pid>` → 0 instancias de silb.exe y 0 de llama-server.exe.

**Prueba rápida del instalador:**
- instalación silenciosa exit=0; ARP con `DisplayName=Silb`, `DisplayVersion=0.3.0`, `Publisher=Whispr Open contributors`, `DisplayIcon=...\silb.exe`.
- uninstall exit=0; ARP Silb tras desinstalar = `eliminado OK`.

**Guard de instancia única:** comprobado. La segunda ejecución apunta
`[main] ya hay otra instancia corriendo: salgo sin tocar atajo ni bandeja`,
muestra un `MessageBoxW` con título "Silb" y **no** registra bandeja, burbuja ni
atajo global.


---

## 9. Criterios de aceptación

- ✅ Un `.exe` portable con su icono que arranca sin ventana de consola.
- ✅ Un instalador que: instala en `%LOCALAPPDATA%\Programs\Silb`, crea accesos
  directos (menú Inicio + escritorio), registra el desinstalador en
  `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall`, muestra la versión
  correcta, y **no pide permisos de administrador**.
- ⏳ Firma de código (el proceso está listo; falta el certificado de verdad).
- ✅ La versión de todos los artefactos coincide con `Cargo.toml` (`0.3.0`).
- ✅ El instalador (23,21 MiB tras la Personalización III) cumple el objetivo de
  ≤25 MiB.
- ✅ El `.ico` en varios tamaños está versionado en el repo, validado por un test
  de Rust y por un comprobador externo en Python.

---

## VISIÓN DE Juanfri 29/10/2026 - el instalador se adapta al equipo - **CERRADA en la 0.8.0**

Guía de diseño para la **release final**, en sus palabras, y en qué quedó cada
punto:

1. **El instalador detecta el equipo antes de copiar nada** → `UseCuda` en
   `[Code]`: `nvidia-smi` y, si no está, WMI. Con `/FORCECPU` y `/FORCECUDA`
   para probar los dos caminos.
2. **Si hay GPU, se instala lo que la usa; si no, la versión de CPU** → hay
   **una sola app**: el instalador deja `silb.exe` CPU o `silb.exe` CUDA + las
   tres DLL del runtime según lo que detecta, y lo deja escrito en
   `{app}\backend.txt`.
3. **Que no pese 500 MB** → el instalador normal son **14,71 MiB**; el pack
   (**442,71 MiB**) es un asset aparte que solo se baja si hay GPU. Quien no
   quiera depender de internet tiene el **offline (444,54 MiB)**, que también
   decide solo. Antes eran 547 MB para todo el mundo porque el base metía el
   runtime de CUDA de todas formas.
4. **Encaja en el empaquetado, no en la app** → todo en `packaging/`
   (`silb.iss`, `build-cuda-runtime.sh`, `build-installers.sh`).

### Cierre de las dos preguntas abiertas

**¿De dónde sale el pack? → (a) del release de GitHub, durante la
instalación.** Descartadas la (b) (que lo baje la app después, desde Ajustes) y
la (c) (meter el runtime dentro del instalador base).

- **La (c) no ahorra nada.** El runtime mínimo que enlaza `silb-cuda.exe` son
  **tres** DLL, no dos: `cudart64_12.dll` 583.680 B + `cublas64_12.dll`
  102.518.272 B + `cublasLt64_12.dll` 668.669.952 B (la tercera porque
  `cublas64_12` la importa estáticamente). Metido en el base, este pasaría de
  15 MB a ~440 MB **para todo el mundo**, con o sin NVIDIA: justo lo que la
  visión quería evitar.
- **La (b) llega tarde.** El usuario tendría que ir a Ajustes a buscar la
  aceleración de algo que ya ha instalado; y como el fallo de una GPU sin
  runtime es "no arranca", el sitio natural para resolverlo es el instalador,
  que es el único que puede verificar el fichero antes de aplicarlo.
- **Coste asumido:** el instalador online lleva la URL y el SHA-256 del pack
  incrustados, así que si el pack se reconstruye hay que recompilar el online
  (`build-installers.sh` lo hace en cadena). Un hash que no cuadra no rompe la
  instalación: cae a CPU y avisa.

**¿Qué es "tiene CUDA"? → basta con el driver NVIDIA** (`nvidia-smi` con código
de salida 0 → WMI `Win32_VideoController` con "NVIDIA"). **No** se comprueba que
el runtime 12 esté instalado (viaja en el pack) ni que haya VRAM:

- **El runtime no hace falta comprobarlo**: las DLL van junto al `.exe` y
  Windows las resuelve de ahí, así que un equipo sin kit CUDA arranca igual
  (comprobado lanzando la app con el PATH sin el toolkit).
- **La VRAM no pinta nada aquí**: la GPU solo acelera whisper.cpp; el servidor
  de IA (`vendor\llama\`) va en CPU. No hay modelo que no quepa.
- **Caso conocido**: una NVIDIA tan vieja que CUDA 12 no la soporte instalará
  el binario CUDA y puede fallar al transcribir. Es el precio de no pedir nada
  al usuario; la salida es volver a ejecutar el instalador con `/FORCECPU` (o el
  offline, que se puede forzar igual).
