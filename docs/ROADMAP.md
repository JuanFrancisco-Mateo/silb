# ROADMAP — Whispr Open (rama `native-iced`)

Estado verificado a 02/09/2026 contra el código y el historial de git. Este
archivo documenta **qué hay hecho** y **qué viene después**, priorizado por
impacto/riesgo. Los documentos hermanos (`CORRECCION-VOZ.md`,
`EMPAQUETADO.md`, `UI-FUTURA.md`) son las especificaciones detalladas de cada
etapa.

> Alcance de esta carpeta `docs/`: solo documentar. No se toca `src/` ni se
> implementa nada desde aquí.

---

## 1. Estado actual (verificado)

| Feature | Dónde | Evidencia |
|---|---|---|
| CUDA activa | `Cargo.toml` → `whisper-rs { features = ["cuda"] }` | commit `f967d13` «activar CUDA (GPU) en whisper-rs + log de system info». whisper.cpp se compila con CUDA; la RTX 4070 corre el modelo en tiempo real. |
| Modelo **Small** por defecto | `src/config.rs` → `selected_model()` | commit `07d542c`. tiny/base producen errores fonéticos en español (`buscar`→`boscar`); Small es el mínimo razonable. Opciones: Tiny (75 MB), Base (142 MB), Small (466 MB). |
| UI 2026, tema **dark** | `src/theme.rs` (tokens) + `src/config.rs` (`is_dark` → dark por defecto) | commit `962342c`. Grafito `#17181C`, acento violeta único `#7C6CFF`, grid de 8 px, radios 8–10 px, dark mode default con toggle persistido. |
| Burbuja flotante | `src/bubble.rs` | commit `be73930`. Segunda ventana Iced (multi-window `iced_winit::Program`), always-on-top, transparente, 56 px, `WS_EX_NOACTIVATE` (no roba el foco). |
| System tray | `src/tray.rs` | commit `e3e0f3b`. `tray-icon` 0.24, menú Mostrar/Ocultar/Salir, icono RGBA 32×32 generado en código (círculo violeta). |
| Atajo global | `src/hotkey.rs` | configurable `shortcut` en `config.json` (default `Ctrl+Shift+D`; global-hotkey 0.6), canal crossbeam drenado en el Poll de 100 ms, eventos filtrados por id del atajo. |
| Inyección en app activa | `src/inject.rs` | `arboard` (portapapeles) + `enigo` (Ctrl+V o tecleo directo). Formato, retardo y filtro por título de ventana configurables (`inject_*` en `config.json`). Solo Windows. |
| Post-procesado | `src/domain/{cleanup,dictionary,correction}.rs` | Muletillas + repeticiones, diccionario personal, rectificaciones habladas por regex multi-turno. |
| Build release | `target/release/whispr-open.exe` | Presente (~43 MB), sin consola (`windows_subsystem`), log a archivo. |

Versión actual del paquete: **0.2.0** (`Cargo.toml`). Rama activa: `native-iced`
(la rama `main` es la versión web legada y no se toca desde aquí).

### Cadena de procesado actual (por turno de dictado)

```
micrófono (cpal, f32 mono, 48k→16k)
  → whisper.cpp (Small, idioma "es", greedy, 4 threads, CUDA)
  → cleanup (muletillas/repeticiones)   [opcional]
  → dictionary (reemplazos personales)  [siempre si hay entradas]
  → correction (rectificaciones habladas por regex, multi-turno)
  → inyección en app activa / transcript
```

---

## 2. Próximas etapas (priorizadas)

| # | Prioridad | Etapa | Doc | Motivo |
|---|---|---|---|---|
| 1 | **P0** | Corrección / formateo por voz con LLM local | [`CORRECCION-VOZ.md`](CORRECCION-VOZ.md) | Es la mayor brecha de UX frente a Wispr Flow: poder decir «no, lo último está mal, es 2 en vez de 4» y que el texto se arregle solo. El motor actual (regex) no cubre numerales ni referencias a "lo último". |
| 2 | **P1** | Empaquetado y distribución | [`EMPAQUETADO.md`](EMPAQUETADO.md) | Hoy solo hay un `.exe` de release sin instalador, sin icono de archivo, sin firma. Necesario para compartir la app. |
| 3 | **P2** | Mejoras de UI | [`UI-FUTURA.md`](UI-FUTURA.md) | Cerrar los huecos del checklist 2026: trust signal "nada sale de tu PC", override visible de la IA, contraste AA del acento, accesibilidad. |
| 4 | P3 (backlog) | Cache del motor Whisper | — | Nota `ponytail` en `src/app.rs:926`: el contexto whisper se recarga en cada turno. Cachear `Mutex<Option<WhisperEngine>>` por modelo ahorra ~1–2 s por turno. |
| 5 | P3 (backlog) | Transcripción casi-tiempo-real | — | Ventanas solapadas + re-decodificación parcial. Riesgo alto, toca el núcleo de grabación; requiere prueba manual de micrófono. |
| 6 | P3 (backlog) | Notetaker / hablantes | — | Reuniones, identificación de hablantes. Fuera de alcance a corto plazo. |

### Orden recomendado de trabajo

1. **P0 (corrección por voz)** primero: se apoya en infraestructura ya existente
   (post-procesado en `apply_pipeline`, hilos de trabajo) y entrega el mayor
   salto de experiencia sin tocar el camino crítico del dictado.
2. **P1 (empaquetado)** en paralelo: es trabajo de tooling/CI y no compite por
   los mismos archivos que P0.
3. **P2 (UI)** al final: varias mejoras son extensiones del sistema de tokens
   ya creado y conviene hacerlas cuando P0 esté estable (el "override visible
   de la IA" depende del panel de confirmación de la corrección por voz).

---

## 3. Definición de "done" (heredada de `AGENTS.md`)

1. `cmd.exe /c "cd /d E:\AI\FREEBUFF && C:\Users\usuario\.cargo\bin\cargo.exe test"` → tests verdes.
2. `... cargo.exe check` → sin errores (warnings tolerados).
3. Commit por etapa lógica, mensaje en español.
