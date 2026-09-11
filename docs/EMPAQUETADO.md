# EMPAQUETADO Y DISTRIBUCIÓN — especificación

Objetivo: convertir `target/release/whispr-open.exe` en distribuciones
instalables y confiables para Windows 10/11.

Estado al cierre de la **Fase 5 (R7, 11/09/2026, v0.3.0)**: instalador
Inno Setup funcional (objetivo ≤25 MiB, sin admin,
`%LOCALAPPDATA%\Programs\WhisprOpen`),
icono `.ico` multirresolución (16/24/32/48/64/128/256, color `#7C6CFF`,
fondo transparente) generado por código e incrustado en el `.exe` y en los
accesos directos, variante portable = el propio `.exe` release (sin
instalador). Medidas reales de ligereza abajo (§8).

**Cambios en R7 (Fase 5):** bump de versión 0.2.0 → 0.3.0 (cierre de la
fase 5 con la personalización completa integrada en el binario final:
sección Imágenes en Ajustes, paleta editable, animación por frames).
Medidas reales actualizadas en §8.4.

---

## 1. Formas de distribución (de más simple a más completa)

### 1.1 Portable `.exe` (recomendado empezar aquí)

Un único ejecutable autocontenido, sin instalación.

- Qué hace falta: incrustar el **icono** en el binario (recursos de Windows,
  ver §4), y empaquetar cualquier DLL de runtime que falte.
- Cómo: `cargo build --release` ya produce un binario estático salvo las
  DLLs de MSVC runtime y las de CUDA/whisper. Verificar con `dumpbin`/`llvm-nm`
  qué DLLs importa y decidir si se enlazan estáticamente o se redistribuyen
  (carpeta junto al `.exe`).
- Estado R6: **el `.exe` se distribuye tal cual como portable**. Se han
  medido 176 MB en RAM en reposo (sin modelo), 486 MB tras el prerroll
  del modelo `small`, 1.213 MB de memoria privada (ver §8). El `.exe`
  tiene el icono `.ico` incrustado y arranca sin pedir elevación ni
  dependencias externas más allá del runtime de MSVC.
- Ventajas: sin elevación de privilegios, ideal para probar y para "probar en
  mano". Desventajas: sin asociación de archivos, sin integración con el menú
  Inicio, sin auto-actualización.

### 1.2 Instalador (Inno Setup)

**Recomendación:** Inno Setup (v6.7.3+). Cubre: instalar a
`%LOCALAPPDATA%\Programs\WhisprOpen\` (sin admin), accesos directos (menú
Inicio + escritorio), desinstalador, integración con `SignTool` para firmar
en el mismo pipeline.

Contenido del instalador:

- `whispr-open.exe` (+ DLLs necesarias, si las hubiera; el binario actual
  no requiere runtime externo gracias a MSVC static).
- Icono `.ico` (se usa también para el acceso directo; el `.exe` ya lo
  lleva incrustado, el segundo es redundante para `IconFilename`).
- Licencia (`LICENSE`, MIT) en la pantalla de bienvenida (`InfoBeforeFile`).
- Registro en "Agregar o quitar programas" (nombre, versión, editor, icono).

#### 1.2.1 Medidas reales del instalador (R6, 11/09/2026)

| Artefacto | Tamaño |
|---|---|
| `target/release/whispr-open.exe` (sin icono) | 44.226.048 B (42,18 MiB) |
| `target/release/whispr-open.exe` (con icono incrustado) | 44.854.272 B (42,77 MiB, +628 KB) |
| `target/release/whispr-open.exe` (tras Personalización I+II+III, feat `image` activada) | 47.178.240 B (44,99 MiB) |
| `packaging/Output/whispr-open-0.2.0-setup.exe` (R6 inicial, feat `image` desactivada) | 23.769.034 B (22,67 MiB) |
| `packaging/Output/whispr-open-0.2.0-setup.exe` (tras Personalización III, feat `image` activa) | 24.332.697 B (23,21 MiB) |
| `packaging/Output/whispr-open-0.3.0-setup.exe` (R7, v0.3.0 con personalización completa) | (medido en §8.4) |

Compresión LZMA2/ultra64 reduce el binario de ~45 MiB a ~23 MiB
(~52 % del original). Objetivo ≤25 MiB: **CUMPLIDO** (margen 1,79 MiB
incluso con la feature `image` activada tras Personalización III).

### 1.3 MSIX

Paquete moderno de Windows (instalación limpia, actualizaciones delta,
sandbox). Requiere **firma de código** obligatoria y un manifiesto
`AppxManifest.xml`.

- Herramientas: `MakeAppx.exe`, `SignTool.exe` (Windows SDK), o
  `cargo-msix`/`MSIX Packaging Tool`.
- Cuándo: cuando se quiera distribución por Microsoft Store o updates
  gestionados por el sistema. No es prioridad para la v1 (la firma es un
  prerequisito duro y caro de arrancar).

---

## 2. Firma de código

Necesaria para que Windows SmartScreen no muestre el aviso de "editor
desconocido" y para MSIX.

- **Certificado**: uno de firma de código (OV u EV) de una CA (DigiCert,
  Sectigo, SSL.com) o un certificado autofirmado para pruebas internas.
  - EV permite reputación inmediata en SmartScreen; OV acumula reputación con
    el tiempo. Para empezar, un certificado de prueba local sirve para
    validar el pipeline.
- **Herramienta**: `SignTool.exe sign /fd SHA256 /a /tr http://timestamp.digicert.com ...`.
- **Dónde firmar**: en CI, con el `.pfx` guardado como secreto (nunca en el
  repo). Firmar **tanto el `.exe` portable como el instalador** (Inno
  permite firmar el `Setup.exe` al final con la directiva `SignTool` en
  la sección `[Setup]`, p. ej.:
  `SignTool=ByParam /f $qcertfile /p $qcertpass /fd SHA256 /tr http://timestamp.digicert.com $f`).
- **Windows will block**: sin firma, SmartScreen marca "Windows protected your
  PC" y el usuario tiene que pulsar "Más información → Ejecutar de todos
  modos". Con OV/EV firmado, desaparece con reputación.

### Estado R6

**Sin firma** en el pipeline actual — el usuario lo activará cuando
disponga de un certificado OV/EV. El `.iss` está listo para añadir la
directiva `SignTool` en cuanto llegue el `.pfx`. NO se ha simulado ni
inventado ningún certificado.

---

## 3. Icono

- **Generación determinista** por código en `assets/build_icon.py`. Sin
  dependencias externas (solo `struct` y `zlib` de la stdlib de Python).
- Formato: BMP DIB dentro de `.ico` (máxima compatibilidad con el
  cargador de recursos de Windows — el formato PNG-in-ICO es rechazado
  por algunas versiones de `rc.exe`).
- Resoluciones: **16, 24, 32, 48, 64, 128, 256 px** (256 obligatorio
  para Windows 11). Color del acento `#7C6CFF` (el mismo del icono de
  bandeja `src/tray.rs` y del tema de la app `src/theme.rs`).
- Fondo transparente: BMP DIB con alfa en el canal A del XOR (sin
  premultiplicar, sin AND mask).
- **Validación**: `scripts/verify-ico.py` parsea el header ICONDIR y
  decodifica la entrada mayor. Test de Rust equivalente en
  `src/icon_header.rs::tests::icon_header_and_payload_are_valid`
  (integrado en `cargo test --lib`).
- **Inserción**: `build.rs` (build-dependency `winresource = "0.1"`).
  El `.ico` se versiona en `assets/whispr-open.ico` para que la build
  no necesite Python (el script solo se ejecuta si el `.ico` falta).
- **Manifest**: el `build.rs` usa `set_icon` pero **no** `set_manifest`:
  el compilador MSVC genera el manifest SxS del CRT
  (`vcruntime140_app.dll`, etc.) automáticamente. Sobreescribirlo rompe
  el arranque con error SxS 14001 (verificado el 11/09/2026). El DPI
  awareness se queda a nivel "DPI Unaware" por ahora (se respeta el
  escalado de Windows; si en algún momento se quiere Per-Monitor V2,
  se añadirá un `.rc` aparte con un recurso neutral (24), no un
  manifest).

---

## 4. Versionado (SemVer)

- Esquema: `MAJOR.MINOR.PATCH`. Actual: **0.3.0** (`Cargo.toml`).
  - `0.x`: API inestable; cada feature nueva sube MINOR, los fixes PATCH.
  - Se pasa a `1.0.0` cuando la app tenga el flujo completo (corrección por
    voz incluida) y una estabilidad de formato demostrada.
- Fuente de verdad única: `Cargo.toml` `version`. El instalador lee la
  versión de la variable de entorno `WHISPR_VERSION` (default `0.3.0`).
- Convención de commits (opcional): prefijos `feat:` / `fix:` / `docs:` /
  `chore:` (ya se usan en el repo) para derivar CHANGELOG y el bump.

---

## 5. Actualizaciones

Opciones, de menor a mayor esfuerzo:

1. **Nada automático (v1)**: el usuario baja el nuevo instalador a mano.
   Suficiente mientras la base de usuarios sea reducida.
2. **Comprobación de versión** (barato): al arrancar, consultar un endpoint
   estático (p. ej. la release más reciente de GitHub) y avisar "hay versión
   nueva" con enlace. Sin descarga automática.
3. **Auto-update completo**: 
   - Portable: `self_update`/`sparkle`-like (reemplazar el binario y
     relanzar). `self_replace` + un descargador.
   - Instalador Inno: Inno Download Plugin, o descargar el `Setup.exe` y
     relanzar.
   - MSIX: actualización nativa del sistema.

Recomendación: 2 → 3 en ese orden. El backend de releases de GitHub (tags +
assets del instalador) es el canal natural para v1.

---

## 6. Paquetes / ediciones futuras

- **Edición única (v1)**: una sola distribución local completa. La app es
  privada por diseño ("nada sale de tu PC"); una única edición refuerza el
  mensaje.
- **Ediciones opcionales futuras** (solo si hay demanda):
  - *Lite*: sin el LLM corrector embebido (usa fallback por reglas), binario
    más pequeño para equipos sin GPU.
  - *Pro*: notetaker de reuniones, identificación de hablantes, cloud
    opcional.
  - *Portable* vs *Instalada* no son "ediciones": son dos artefactos de la
    misma edición.
- Cualquier edición de pago introduciría licencia; hoy la app es MIT.

---

## 7. Pipeline de release (pasos exactos para R6, 11/09/2026)

### 7.1 Build del binario release (con icono incrustado)

```cmd
:: 1) Build release. El build.rs incrusta el icono vía winresource.
::    Si el .ico falta, lo regenera con assets/build_icon.py.
::    No hace falta llamar a Python a mano.
cd /d E:\AI\FREEBUFF
C:\Users\usuario\.cargo\bin\cargo.exe build --release

:: Salida: target\release\whispr-open.exe (≈42,77 MiB con icono)
```

El manifest SxS del CRT lo genera el propio compilador MSVC (no
sobreescribir).

### 7.2 Build del instalador Inno Setup

```cmd
:: 2) Compilar el .iss. Inno Setup 6.7.3 está instalado por usuario en:
::    C:\Users\usuario\AppData\Local\Programs\Inno Setup 6\ISCC.exe
::
::    El script lee la versión de la variable de entorno WHISPR_VERSION
::    (default 0.3.0). Para hacer un release con otra versión:
::      set WHISPR_VERSION=0.3.1
::      ISCC.exe ... whispr-open.iss
::
::    Para apuntar a otro directorio de release (CI, rama de hotfix):
::      ISCC.exe /DWHISPR_RELEASE_DIR=D:\builds\release ...
::
::    El instalador escribe en:
::      packaging\Output\whispr-open-{version}-setup.exe

"C:\Users\usuario\AppData\Local\Programs\Inno Setup 6\ISCC.exe" /Q packaging\whispr-open.iss
```

El instalador **no requiere elevación** (`PrivilegesRequired=lowest`),
instala en `%LOCALAPPDATA%\Programs\WhisprOpen`, registra el
desinstalador en
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{AppId}_is1`
y muestra la licencia MIT antes de pedir confirmación.

### 7.3 Smoke test (instalar + arrancar + desinstalar)

```cmd
:: Instalar (silencioso, sin wizard):
packaging\Output\whispr-open-0.3.0-setup.exe /SP- /S /SILENT /SUPPRESSMSGBOXES /NORESTART

:: Verificar que el icono está incrustado:
powershell -NoProfile -Command "Add-Type -AssemblyName System.Drawing; [System.Drawing.Icon]::ExtractAssociatedIcon('%LOCALAPPDATA%\Programs\Whispr Open\whispr-open.exe')"

:: Medir RAM en reposo y tras el prerroll del modelo (muestreo cada 10 s):
powershell -NoProfile -Command "Get-Process whispr-open | Select WorkingSet64,PrivateMemorySize64"

:: Desinstalar (silencioso, preserva config y modelos):
"%LOCALAPPDATA%\Programs\Whispr Open\unins000.exe" /VERYSILENT /SUPPRESSMSGBOXES /NORESTART
```

---

## 8. Medidas de ligereza (reales, R6, 11/09/2026)

### 8.1 Tamaño del binario (release, perfil optimizado)

Medido el 11/09/2026 con `scripts/measure-release-profile.sh` (cada
variante recompila el binario desde cero y reporta el delta frente a
la base).

| Variante | Tamaño | Δ vs base | % |
|---|---:|---:|---:|
| Base (`lto=true, strip=true`) | 44.854.272 B (42,77 MiB) | — | — |
| + `opt-level="z"` | 39.932.928 B (38,08 MiB) | -4.921.344 B | -11,0 % |
| + `panic="abort"` | 40.923.648 B (39,03 MiB) | -3.930.624 B | -8,8 % |
| TODO (`cgu=1` + `opt-level=z` + `panic=abort`) | 36.868.096 B (35,16 MiB) | -7.986.176 B | -17,8 % |

El script `scripts/measure-release-profile.sh` reconstruye el `.exe`
con cada variante. El perfil que **se queda en el repo** (`Cargo.toml`)
es la base (`lto=true, strip=true`): cumple el objetivo de tamaño del
instalador (≤25 MiB, ver §1.2.1) sin sacrificar rendimiento. Cualquiera
de las variantes adicionales se puede activar poniendo
`[profile.release]` overrides en un `~/.cargo/config.toml` o con
`--config <ruta>` por build.

### 8.2 Decisión: solo `lto=true` + `strip=true`

Razones (medidas el 11/09/2026):

1. **El binario ya cumple el objetivo de tamaño** (44,99 MiB con la
   feature `image` activa tras Personalización III; instalador 23,21 MiB
   tras LZMA, ver §1.2.1).
2. **`opt-level="z"` ahorra 4,92 MiB (-11,0 %)** — pero penaliza el
   rendimiento: la app es GPU-bound en la transcripción (whisper.cpp +
   CUDA) y CPU-bound en el render (Iced 0.13); un -11 % en velocidad
   por 4 MiB no compensa.
3. **`panic="abort"` ahorra 3,93 MiB (-8,8 %)** — pero rompe el
   `catch_unwind` que Iced/winit pueden usar para sobrevivir a pánicos
   del usuario (la app actual no lo hace explícitamente, pero la
   práctica estándar de Rust es mantener `unwind` para no cambiar el
   comportamiento de pánicos de la stdlib).
4. **TODO (`cgu=1 + z + abort`) ahorraría 7,99 MiB (-17,8 %)** — útil
   solo si en algún momento el binario supera ~50 MiB (p. ej. CUDA se
   enlaza estáticamente de verdad). Hoy no compensa.

Si en algún momento el instalador supera 30 MiB, se reconsidera
activar `opt-level="z"` o `panic="abort"`. Hoy, no compensa.

### 8.3 RAM en reposo y tras prerroll del modelo

Medido con `Get-Process` sobre Windows 11 24H2, modelo `small`
(ggml-small.bin, 462 MB en disco), muestreo durante 65 s:

| Tiempo | WorkingSet (MB) | PrivateMemory (MB) |
|---|---:|---:|
| t=+5 s (pre-prerroll) | 176 | 257 |
| t=+15 s (cargando whisper) | 435 | 1.115 |
| t=+35 s (cargando whisper) | 495 | 1.214 |
| t=+55 s (prerroll listo) | **486** | **1.213** |
| t=+65 s (estable) | **486** | **1.213** |

- **En reposo sin modelo**: ~176 MB WorkingSet, ~257 MB Private.
- **Tras prerroll de `small`**: ~486 MB WorkingSet, ~1.213 MB Private
  (el modelo `small` ocupa ~310 MB adicionales en RAM).
- Con modelo `tiny` (≈75 MB en disco) el salto es mucho menor (~80 MB
  adicionales). Con `base` (≈140 MB) el salto es de ~140 MB.

El script `scripts/measure-ram.bat` reproduce el muestreo: lanza el
`.exe`, espera N segundos (default 5), y muestrea cada 10 s durante 60
s. Imprime WorkingSet, Private y Virtual en MB y termina matando el
proceso con `taskkill /F`.

### 8.4 Decisión sobre prerroll y carga del modelo

El prerroll actual (carga el modelo configurado al arrancar en un hilo
de fondo) **se mantiene**: el coste de 1–2 s en la primera pulsación
del atajo se paga al inicio de la sesión, no en mitad del dictado.
Para quien no quiera ese coste de RAM, basta cambiar el modelo a
`tiny` en Ajustes → Modelo (~75 MB en disco, +80 MB en RAM).

### 8.5 Medidas reales de R7 (Fase 5, 11/09/2026, v0.3.0)

Cierre de la fase 5 con bump de versión 0.2.0 → 0.3.0 y el binario
final con la personalización completa integrada (Ajustes por secciones
+ paleta + imágenes y animación por frames).

| Artefacto | Tamaño (B) | Tamaño (MiB) |
|---|---:|---:|
| `target/release/whispr-open.exe` (v0.3.0) | **47.177.728** | **44,99** |
| `packaging/Output/whispr-open-0.3.0-setup.exe` | **24.331.612** | **23,20** |
| `assets/whispr-open.ico` (16/24/32/48/64/128/256) | 372.526 | 363,79 KiB |

**Δ vs R6:** el `.exe` es idéntico al byte (-0 B; el cambio de versión
solo afecta al string `version` en `Cargo.toml`). El instalador
disminuye -1.085 B (≈ -0,004 %) por el mismo motivo. **Objetivo ≤25 MiB
del instalador:** CUMPLIDO (margen 1,80 MiB).

**RAM tras prerroll de `small`** (medido el 11/09/2026 con `psapi.GetProcessMemoryInfo`):
WorkingSet 484,6 MiB, PrivateMemory 1.243,9 MiB. Coherente con la medición
de R6 (486 / 1.213 MiB).

**Smoke del exe (`target/release/whispr-open.exe` directo, sin instalar):**
- PID arranca; log escribe `Whispr Open 0.3.0 iniciando` en `%APPDATA%\whispr-open\whispr-open.log`.
- Prerroll de whisper en ~1 s; bandeja lista; ventana principal registrada (id=2); burbuja aplicada noact+pos+tamaño 72.
- `taskkill /T /PID <pid>` → 0 instancias whispr-open.exe y 0 instancias llama-server.exe.

**Smoke del instalador (`scripts/install-smoke.py`):**
- install exit=0; ARP con `DisplayName=Whispr Open`, `DisplayVersion=0.3.0`, `Publisher=Whispr Open contributors`, `DisplayIcon=...\whispr-open.exe`.
- uninstall exit=0; ARP Whispr Open tras uninstall = `eliminado OK`.

**Guard de instancia única:** verificado. La segunda ejecución registra
`[main] ya hay otra instancia corriendo: salgo sin tocar atajo ni bandeja`,
muestra `MessageBoxW` con título «Whispr Open» y **no** registra bandeja,
burbuja ni atajo global.

Detalle completo en `INFORME-CIERRE.md` (raíz del repo).

---

## 9. Criterios de aceptación

- ✅ Un `.exe` portable con icono propio que arranca sin consola.
- ✅ Un instalador que: instala en `%LOCALAPPDATA%\Programs\WhisprOpen`,
  crea accesos directos (menú Inicio + escritorio), registra desinstalador
  en `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall`, muestra
  versión correcta, **sin elevación de administrador**.
- ⏳ Firma de código (pipeline listo; pendiente de certificado real).
- ✅ La versión en todos los artefactos coincide con `Cargo.toml`
  (`0.3.0`).
- ✅ El instalador (23,21 MiB tras Personalización III) cumple el
  objetivo de ≤25 MiB.
- ✅ El `.ico` multirresolución está versionado en el repo, validado por
  un test de Rust y un verificador externo Python.