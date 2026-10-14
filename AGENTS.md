# AGENTS.md — Reglas de entorno para agentes autónomos

Proyecto: **Silb** — dictado por voz local NATIVO de escritorio
(Iced + whisper.cpp vía whisper-rs). Cero frontend web (sin webview, sin
React/TS/Vite — eso vive en la rama `main` como versión legada).
Rama activa: `native-iced`. Ruta: `E:\AI\FREEBUFF` (desde WSL: `/mnt/e/AI/FREEBUFF`).
Leer este archivo ANTES de tocar nada.

## Entorno de ejecución (CRÍTICO — verificado el 02/09/2026)

- El proyecto vive en un disco Windows (E:). Rust/cargo es el de Windows:
  `C:\Users\usuario\.cargo\bin\cargo.exe` (`cmd.exe` NO tiene cargo en PATH).
- NO compilar apuntando al cargo de WSL (target Linux, faltan deps).
- La primera compilación completa tarda 5–10 min (iced + whisper.cpp). Es normal.
- TOOLCHAIN: cmake 4.x y MSVC (VS2019 BuildTools, 14.29) presentes — necesarios
  para compilar whisper-rs (whisper.cpp en C). `where cl` desde WSL falla pero
  cargo encuentra MSVC vía vswhere, así que compila igual.
- whisper-rs: DEBE ser 0.16+ (0.12/0.14 fallan contra whisper.cpp actual:
  campos de `whisper_full_params` cambiados). API 0.16:
  `SamplingStrategy`, `state.get_segment(i) -> Option<WhisperSegment>`, `seg.to_str()`.
- Modelos GGML (whisper.cpp, de HF ggerganov) en `%APPDATA%/whispr-open/models/`.
- REGLA DE PRODUCTO: el server LLM NUNCA se lanza desde WSL (wsl.exe). TODO
  arranque lo hace la app con el binario NATIVO Windows
  `D:\\LlamaCPP\\llama-server.exe` (build Clang 20/CUDA 13, cublas64_13.dll en
  el mismo dir).

## Definición de "done" (criterios innegociables)

1. `cmd.exe /c "cd /d E:\AI\FREEBUFF && C:\Users\usuario\.cargo\bin\cargo.exe test"` → 295 tests verdes.
2. `... cargo.exe check` → sin errores (warnings tolerados).
3. Commit en git por fase lógica, mensaje en español.

## Arquitectura actual (mapa de ficheros)

- `src/main.rs` — entrypoint Iced 0.13 (patrón funcional: application + run_with).
- `src/app.rs` — UI completa (WhisprApp): status, transcript, controles,
  selector de modelo, diccionario, checkboxes. Hilos std para audio/transcripción,
  eventos por canales mpsc + subscription de poll cada 100 ms.
- `src/domain/correction.rs` — rectificaciones habladas (port fiel de TS).
- `src/domain/cleanup.rs` — muletillas + repeticiones (port fiel de TS).
- `src/domain/dictionary.rs` — diccionario personal (port fiel de TS).
- `src/audio.rs` — captura micrófono cpal (f32 mono, por turnos).
- `src/transcribe.rs` — Model (tiny/base/small), descarga GGML con progreso
  (ureq), WhisperEngine (Mutex<WhisperContext>).
- `src/inject.rs` — inyección de texto en app activa (arboard + enigo, Ctrl+V).
- `src/hotkey.rs` — atajo global Ctrl+Shift+D (global-hotkey + crossbeam).
- `src/config.rs` — config persistente JSON en %APPDATA%/whispr-open/config.json.

## No hacer

- No lanzar el server LLM vía WSL/wsl.exe desde el código (ver Entorno).
- No volver a introduce web frontend ni Tauri (es el objetivo de la rama native-iced).
- No tocar la rama `main` desde aquí (versión web legada, se mantiene como referencia).
- No tocar hardware ni cron del sistema.
- El flujo real de micrófono lo prueba el usuario; verificación = tests/build.