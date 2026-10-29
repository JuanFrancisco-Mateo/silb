# AGENTS.md — Reglas de entorno para agentes autónomos

Proyecto: **Silb** — dictado por voz local, app de escritorio nativa
(Rust + Iced + whisper.cpp vía whisper-rs). Nada de webview ni Tauri.
Leer este archivo ANTES de tocar nada.

- Repo público: `github.com/JuanFrancisco-Mateo/silb`. Rama de trabajo: `main`.
- El repo privado `whispr-open` es el archivo histórico: no se publica desde ahí.
- Todos los comandos se lanzan desde la raíz del repo clonado; no se asume
  ninguna carpeta concreta ni un usuario de Windows concreto.
- Sin WSL: todo (compilar, tests, lanzar la app) se hace en Windows nativo.
  Las únicas partes de Linux son el port (`docs/PORT-LINUX/`) y los scripts
  `*-linux.sh`.

## Entorno de ejecución (verificado el 29/10/2026)

- Rust/cargo es el de Windows. `cargo test` y `cargo build --release` desde una
  consola normal o `cmd.exe /c "cargo test"`; no está en el PATH de MSYS.
- La primera compilación completa tarda 5–10 min (iced + whisper.cpp). Es normal.
- Toolchain: cmake 4.x y MSVC (VS2019 BuildTools 14.29) para whisper-rs;
  cargo encuentra MSVC solo vía vswhere, no hace falta abrir su consola.
- whisper-rs debe ser 0.16+ (la 0.12/0.14 no compila contra whisper.cpp actual:
  cambiaron los campos de `whisper_full_params`).
- Datos de la app: `%APPDATA%\whispr-open\` en Windows y
  `~/.local/share/whispr-open/` en Linux (`src/paths.rs`). Dentro:
  `config.json`, `models\` (modelos GGML de whisper), `models-llm\` (GGUF del
  servidor de corrección) y `whispr-open.log`.
- El servidor de corrección (`llama-server`) se lanza SIEMPRE como proceso
  nativo desde la app (`src/llm.rs`), con el binario de `vendor\llama\` o el que
  haya configurado el usuario. Nada de `wsl.exe`.

## Definición de "done" (criterios innegociables)

1. `cargo test` → **352 pasan, 0 fallan, 1 ignorado**.
2. `cargo check` → sin errores (los 2 avisos conocidos de `src/hotkey.rs` se
   toleran).
3. Commit por fase lógica, mensaje en español.
4. Si toca el instalador: recompilar con `ISCC.exe packaging\whispr-open.iss` y
   comprobar el tamaño (el base debe rondar los 15 MB).

## Arquitectura actual (mapa de ficheros)

- `src/main.rs` — arranque de Iced 0.13 (patrón funcional: `application` +
  `run_with`) y comprobación de instancia única.
- `src/app.rs` — la app entera: estado, mensajes y el reparto entre pantallas.
  Es el fichero más grande; para orientarse, ir por las cabeceras de sección.
- `src/ui/` — piezas de interfaz: `shell.rs` (marco y pestañas), `settings.rs`
  (Ajustes), `settings_handlers.rs` (qué hace cada control), `recording.rs`
  (pantalla de dictado), `bubble.rs` e `icons.rs` (indicadores), `poll.rs`
  (latido de 100 ms) y `window_tray.rs` (bandeja del sistema).
- `src/llm.rs` — corrección con IA: arranca y vigila `llama-server`, arma la
  petición, limpia la respuesta y protege contra recortes.
- `src/domain/` — reglas del texto, sin interfaz: `correction.rs`
  (rectificaciones habladas), `corrective.rs` (corrección con IA),
  `cleanup.rs` (muletillas y repeticiones) y `dictionary.rs` (diccionario
  personal).
- `src/audio.rs` — captura del micrófono con cpal (mono, por turnos), con el
  filtro que descarta los plugins de ALSA en Linux.
- `src/transcribe.rs` — modelos de whisper, descarga e motor
  (`WhisperContext` protegido por `Mutex`).
- `src/inject.rs` — escribir el texto en la app que esté en primer plano.
- `src/hotkey.rs` — atajo global (Ctrl+Shift+D en Windows; en Wayland lo da el
  compositor).
- `src/bubble.rs` — la burbuja flotante que se ve mientras dictas.
- `src/config.rs` — ajustes persistentes en JSON.
- `src/paths.rs`, `src/log.rs`, `src/fonts.rs`, `src/theme.rs`,
  `src/icon_header.rs`, `src/custom_image.rs`, `src/tray.rs`,
  `src/single_instance.rs` — datos, registro, tipografías, colores, icono,
  imágenes propias, bandeja y mutex de instancia única.

## Empaquetado y documentación

- `packaging\whispr-open.iss` — instalador Inno. Sin argumentos compila el base
  **solo CPU** (~15 MB); con `/DMyCudaPack` compila el pack CUDA opcional
  (~522 MB) que se instala encima y exige el runtime CUDA 12.
- `scripts\` — utilidades: `linux-check.sh` (verificación del port, escribe un
  informe en `docs/`), `build-linux.sh`, `run-linux.sh`, `make-brand-images.py`
  (logo y banner), `capture-window.ps1` (captura de la ventana con
  `PrintWindow`, no con la pantalla), `audit-doc-comments.py`.
- `docs\` — arquitectura, empaquetado, guía de usuario, roadmap, checklist de
  release y el port a Linux (`docs/PORT-LINUX/`).

## Idioma

- Comentarios, documentación y mensajes de commit en **español llano**: frases
  cortas, sin jerga innecesaria (`fallback`, `handler`, `runtime`… se explican
  o se sustituyen). Los nombres reales de la API de Iced no se traducen.

## No hacer

- No meter frontend web ni Tauri de vuelta: eso era la versión vieja.
- No lanzar el servidor de IA por WSL/`wsl.exe`.
- No tocar hardware ni tareas programadas del sistema.
- El micrófono y la interfaz los prueba el usuario; aquí la verificación es
  tests + build (y el informe de `linux-check.sh` para el port).
