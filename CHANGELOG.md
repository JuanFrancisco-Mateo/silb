# Historial de versiones

Formato inspirado en [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/).
Las fechas son de la sesión de cierre correspondiente (rama `native-iced` salvo
indicación).

## [0.6.0] — 2026-10-29

Versión de **peso y port**: el instalador pasa de 549 MB a 15 MB y el port a
Linux se puede probar de verdad. Sin cambios en el formato de datos ni en la
config: actualizar por encima de una 0.5.0 no pide reconfigurar nada.

### Cambiado

- **El instalador ya no pesa medio giga.** El base es **CPU-only** y pesa
  ~15,4 MB: lleva el exe de CPU y deja fuera `vendor\llama\cuda\` (656 MB de
  DLLs de NVIDIA). La GPU se sirve en un **pack CUDA aparte** (~522 MB) que se
  instala encima del base y añade `silb-cuda.exe`.
- **El pack CUDA comprueba el runtime antes de copiar.** `silb-cuda.exe` importa
  `cudart64_12.dll` y `cublas64_12.dll`: si el equipo no los tiene, el
  instalador del pack se para, no toca nada y remite al instalador CPU. Antes
  se podía dejar instalado un exe que Windows no llegaba a arrancar.
- **Actualizar del pack al base conserva `cuda\`** (el servidor LLM sigue
  usando la GPU aunque el dictado pase a CPU); el desinstalador sí borra esos
  ficheros, así que no queda basura permanente.

### Añadido

- `scripts/linux-check.sh`: comprobación completa en Linux (`cargo check`,
  tests, build release, tamaño y dependencias del binario) que deja informe en
  `docs/VERIFICACION-LINUX-<fecha>.txt`. Si no hay binario, ahora cuenta como
  fallo en vez de cerrar en verde.
- Guía de prueba del port en `docs/PORT-LINUX/`: qué hay que comprobar en cada
  apartado y qué se considera correcto.

### Corregido

- **Selector de micrófono en Linux**: el host de audio lista todo el zoo de
  plugins de ALSA (`null`, `lavrate`, `speexrate`, `jack`, `oss`, `upmix`,
  `vdownmix`, `usbstream`…) como si fueran micrófonos. Ahora solo aparecen la
  entrada por defecto (`default`, `pipewire`, `pulse`) y los PCM de tarjeta
  (`hw:`, `plughw:`, `sysdefault:`, `front:`).
- **El log dice qué micrófono se abre** (`dispositivo de entrada: «…»`): antes
  no había forma de saber si dictabas con el micro interno o con el USB.
- **Los scripts de Linux apuntaban a un binario inexistente**
  (`target/release/whispr-open`); el binario real es `silb`.
- **Rutas de modelos de llama.cpp en Linux**: ya no se listan `C:/` y `D:/`
  como si fueran carpetas locales.
- **Burbuja de dictado en Wayland**: se queda en el workspace donde arranca. No
  se puede fijar desde la app; queda documentada la regla del compositor
  (Hyprland: `float` + `pin`) en `docs/PORT-LINUX/CODIGO-UI.md`.

### Verificado

- `cargo test`: 341 correctos en Windows, 0 fallos (1 ignorado); suite completa
  también en verde en Linux (Ryzen 5 PRO 5650U, Arch).
- Rendimiento con el modelo `small` y `beam=5`: en CPU (6 hilos) 0,21–0,63 de
  tiempo real según la duración del audio; en GPU (RTX 4070) 0,06–0,21, es
  decir **3–4× más rápido**.

## [0.5.0] — 2026-10-26

Primera versión que **compila y arranca en Linux** además de Windows. Sin
cambios de formato de datos: la config y los modelos siguen en
`%APPDATA%\whispr-open` (Windows) / `~/.local/share/whispr-open` (Linux).

### Añadido

- **Port a Linux integrado** (venía de las ramas `feature/port-linux-*` del
  3-4 de octubre, nunca mergeadas). Lo que se comporta distinto por plataforma:
  - **Atajo global**: en Windows sigue `global-hotkey`; en Wayland/Hyprland se
    usa el socket de `hyprctl` (`docs/PORT-LINUX/HOTKEY-HYPR.md`).
  - **Inyección en la app activa**: Windows con `enigo` + portapapeles; en
    Linux con `wl-copy`/`wtype`/`ydotool` según `inject_backend`.
  - **Instancia única**: mutex con nombre en Windows, `flock` en
    `$XDG_RUNTIME_DIR/whispr-open/instance.lock` en Linux.
  - **Server LLM local**: `llama-server.exe` en Windows, `llama-server` en
    Linux; en ambos casos el proceso se termina al salir (SIGTERM + SIGKILL de
    rescate en Linux, `TerminateProcess` verificado por imagen en Windows).
  - **Bandeja**: el tray es de Windows; en Linux la app funciona sin bandeja
    (ventana + atajo + Salir en la UI) porque Hyprland no trae host
    StatusNotifier.
  - **Burbuja flotante**: sin `WS_EX_NOACTIVATE` en Linux; el resto del
    comportamiento es el mismo.

### Notas

- En Linux no hay instalador: se compila con `scripts/build-linux.sh` y se
  lanza con `scripts/run-linux.sh`.
- El runtime de Linux está verificado a nivel de compilación (`cargo check` y
  `cargo test` en Ubuntu), **no** todavía en una sesión Wayland real: la
  primera prueba de micrófono + inyección en Hyprland queda pendiente.

## [0.4.0] — 2026-10-19

Primera versión con el producto ya renombrado a **Silb** y el repo listo para
publicarse. Sin cambios de formato de datos: la config y los modelos siguen en
`%APPDATA%\whispr-open`.

### Añadido

- **Renombrado a Silb**: binario `silb.exe`, instalador `silb-<ver>-setup.exe`,
  icono y textos de la app. El paquete y el directorio de datos conservan
  `whispr-open` para no dejar sin ajustes a quien ya la tiene instalada.
- **Rediseño de UI desde Figma** (fase 1: tokens y frame Dictado; fase 2:
  frame Ajustes) y retícula única en las cinco pestañas.
- **GIF animado en la burbuja flotante** (codec gif, sin arrastrar
  rav1e/avif al binario).
- **Catálogo local de modelos LLM** y rename de proveedor en Ajustes.
- **Server LLM integrado en el paquete** (`vendor/llama`): la app lo lanza y
  lo cierra sola, sin rutas manuales.
- **Secciones en la ventana principal**, pestañas compartidas y avisos.
- **Build CUDA opcional**: `cargo build --release --features cuda`. Por
  defecto compila en CPU (MSVC + CMake), sin toolkit CUDA ni GPU.

### Cambiado

- `Cargo.lock` trackeado: builds reproducibles.
- Conteo de tests corregido con medición real sobre el tip: **336 passed,
  0 failed, 3 ignored**.
- Documentación sin rutas ni informes internos de sesión.

### Corregido

- El guard numérico caza el numeral viejo en frases naturales, y la
  reconstrucción es el texto que se inyecta.
- Los assets de personalización se copian al directorio de datos.
- El log rota por tamaño y deja de registrar el ruido del arrastre.
- Fallback al micrófono por defecto si el configurado desaparece.
- Se conserva el modelo local cuando falla su carga (bucle DNS).
- Eliminado el `expect` frágil de la imagen de fondo.

### Notas de empaquetado

- El instalador incluye el server LLM (`vendor/llama`, ~44 MB en CPU; el pack
  CUDA opcional añade ~656 MB). Los binarios no viajan en el repo: se copian
  con `packaging/vendor-llama.sh`.

## [0.3.0] — 2026-09-11

Primera release pensada para gente de fuera: instalador, icono de producto y
personalización completa dentro del binario.

### Añadido

- **Instalador Inno Setup** (`packaging/whispr-open.iss`): instala en
  `%LOCALAPPDATA%\Programs\WhisprOpen` **sin administrador**, con acceso
  directo (menú Inicio + escritorio opcional), entrada correcta en «Agregar o
  quitar programas» y desinstalador que pregunta si borrar config y modelos.
  Salida: `whispr-open-0.3.0-setup.exe` (~23 MiB con LZMA2/ultra64).
- **Icono multirresolución** (16/24/32/48/64/128/256, `#7C6CFF`) generado por
  código (`assets/build_icon.py`, invocado desde `build.rs`) e incrustado en
  el `.exe` y en los accesos directos.
- **Personalización completa** (Ajustes): imagen de fondo propia, sprite o
  animación por frames en la burbuja, paleta editable con presets y por campo,
  y Ajustes reorganizados por secciones con pestañas.
- **Variante portable**: el propio `target\release\whispr-open.exe`
  autocontenido, sin instalación.

### Cambiado

- Versión 0.2.0 → 0.3.0 (cierre de las fases R6+R7 del roadmap).
- La documentación de cierre queda en `INFORME-CIERRE.md` y
  `docs/EMPAQUETADO.md` (medidas reales de tamaño y RAM).

## [0.2.0] — 2026-09-11

Reescritura a **escritorio nativo** (Rust + Iced + whisper.cpp). Adiós web.

### Añadido

- Corrección IA completa: server LLM nativo autogestionado (la app lo lanza y
  lo cierra), guard anti-truncación, proveedores **Local** (llama.cpp) /
  **Custom** (endpoint OpenAI-compatible) / **Cloud** (OpenAI), API key en el
  Credential Manager de Windows, chip de estado y opciones explicadas.
- Nota editable antes de inyección, con confirmación de copiado y borrado
  deshace-able.
- Robustez a prueba de fallos: descarga de modelos con reintentos y reanudación,
  micrófono que muere a mitad de grabación, inyección sin ventana con foco
  (queda en portapapeles, nunca se pierde), guard de instancia única.
- Contraste AA medido (`src/theme.rs`), pie de confianza y descarga con MB y
  tiempo restante.
- Licencia MIT, `docs/ARQUITECTURA.md` para quien llega nuevo, guía de
  usuario ampliada y doc-comments en español en funciones y tipos públicos.

## [0.1.0] — 2026-09-02 (rama `main`)

Prototipo web (React + Tauri v2 + Whisper WASM): 40 tests. Queda archivado en
el repo privado `whispr-open`, fuera de este.

[0.4.0]: https://github.com/JuanFrancisco-Mateo/silb/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/JuanFrancisco-Mateo/silb/releases/tag/v0.3.0
[0.2.0]: https://github.com/JuanFrancisco-Mateo/silb/compare/v0.1.0...v0.3.0
[0.1.0]: https://github.com/JuanFrancisco-Mateo/silb/releases/tag/v0.1.0
