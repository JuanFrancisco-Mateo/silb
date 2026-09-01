# AGENTS.md — Reglas de entorno para agentes autónomos

Proyecto: **Whispr Open** — dictado por voz local (whisper.cpp WASM + Tauri v2).
Ruta: `E:\AI\FREEBUFF` (desde WSL: `/mnt/e/AI/FREEBUFF`).
Leer este archivo ANTES de tocar nada. Es obligatorio.

## Entorno de ejecución (CRÍTICO — verificado el 01/09/2026)

- El proyecto vive en un disco Windows (E:). `node_modules` está instalado
  **solo con bindings nativos de Windows** (rolldown `binding-win32-x64-msvc`).
- **PROHIBIDO ejecutar `npm` o `npx` directamente desde WSL**: falla con
  `Cannot find native binding` (rolldown/vitest).
  - ✅ Comando correcto desde WSL:
    - `cmd.exe /c "cd /d E:\AI\FREEBUFF && npm test"`
    - `cmd.exe /c "cd /d E:\AI\FREEBUFF && npm run build"`
  - ❌ `npm test` a secas en WSL = error de bindings. No es un bug del código.
- **PROHIBIDO reinstalar node_modules** (`npm install`, borrar package-lock, etc.)
  salvo orden explícita. Si algún agente lo rompe, restaurar desde git y
  reinstalar con `cmd.exe /c "cd /d E:\AI\FREEBUFF && npm install"`.
- Rust/cargo también es el de Windows (`C:\Users\usuario\.cargo\bin\cargo.exe`).
  Todo lo de Tauri se compila/verifica con `cmd.exe /c "cd /d E:\AI\FREEBUFF && cargo ..."`.
  NO compilar Tauri apuntando al cargo de WSL (target Linux, faltan deps de webkit).
- La primera compilación completa de Tauri tarda 5–10 min. Es normal.
- Navegador objetivo: WebView2 (ya presente en Windows 11). Vite ya envía
  cabeceras COOP/COEP (necesarias para SharedArrayBuffer/WASM SIMD).

## Definición de "done" (criterios innegociables)

1. `npm test` pasa vía `cmd.exe /c` (nunca menos tests que el baseline actual).
2. `npm run build` (tsc + vite) pasa sin errores. Warnings de chunk size se toleran.
3. Si se tocó Rust/Tauri: `cmd.exe /c "cd /d E:\AI\FREEBUFF && cargo check"` pasa.
4. Commit en git por fase lógica terminada, mensaje en español describiendo la fase.

## Estilo de código obligatorio

- Módulos de lógica pura en `src/domain/` (funciones puras, sin side effects).
- Tests junto al módulo: `src/domain/<modulo>.test.ts` con Vitest (`environment: node`).
  Los imports de `@remotion/whisper-web` y de navegador se mockean como ya hace
  `src/services/local-whisper.test.ts` y `src/services/recording-controller.test.ts`.
- NO añadir dependencias npm nuevas sin necesidad probada. Primero mirar qué hay.
- TypeScript estricto. No usar `any`.
- No borrar código que ya funciona. Cambios pequeños y verificables por commit.

## Arquitectura actual (mapa de ficheros)

- `src/App.tsx` — UI + cableado de todo el flujo (SUPERFICIE DE CONFLICTOS:
  por defecto SOLO el agente integrador lo modifica).
- `src/domain/correction.ts` — motor de rectificaciones habladas (regex puras) + `CorrectionSession`.
- `src/services/local-whisper.ts` — descarga/cache del modelo WASM + transcripción.
- `src/services/recording-controller.ts` — ciclo grabación/transcripción por turnos (deps inyectables).
- `src/services/desktop-shortcut.ts` — listener del atajo global Tauri (Ctrl/Cmd+Shift+D).
- `src-tauri/src/lib.rs` — shell Tauri v2: plugin global-shortcut, emite `global-dictation-toggle`.
- `src/styles.css` — sistema visual.

## No hacer

- No tocar el hardware ni cron del sistema.
- No lanzar la app con micrófono real ni pedir permiso de audio.
- No asumir que la app "funciona dictando" — la verificación es tests/build,
  el flujo real de micrófono lo prueba el usuario.
- No iniciar servidores en foreground que bloqueen. Si necesitas correr la app,
  background + health check.