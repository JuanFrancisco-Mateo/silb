# ROADMAP — Whispr Open (rama `native-iced`)

Estado verificado a 04/09/2026 contra el código y el historial de git. Este
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
| Post-procesado | `src/domain/{cleanup,dictionary,correction}.rs` | Limpieza fija de repeticiones/expresiones, diccionario personal y rectificaciones habladas; la UI ya no expone configuración de muletillas. |
| Corrección IA configurable | `src/config.rs`, `src/llm.rs`, `src/app.rs` | Proveedor local/cloud/custom OpenAI-compatible, timeout, umbral, confirmación opcional y API key fuera de `config.json`. |
| Audio robusto | `src/audio.rs` | El stream se crea y reproduce antes de entrar en `Recording`; todo audio no-16 kHz se remuestrea a 16 kHz. |
| Build release | `target/release/whispr-open.exe` | Presente (~43 MB), sin consola (`windows_subsystem`), log a archivo. |

Versión actual del paquete: **0.2.0** (`Cargo.toml`). Rama activa: `native-iced`
(la rama `main` es la versión web legada y no se toca desde aquí).

### Cadena de procesado actual (por turno de dictado)

```
micrófono (cpal, f32 mono, cualquier rate → 16 kHz)
  → whisper.cpp (modelo elegido, idioma configurable, beam search, CUDA)
  → limpieza local fija (repeticiones/muletillas)
  → dictionary (reemplazos personales)
  → correction (reglas + LLM opcional en hilo)
  → inyección en app activa / transcript
```

---

## 2. Próximas etapas (priorizadas)

| # | Prioridad | Etapa | Doc | Motivo |
|---|---|---|---|---|
| 1 | **Hecho** | Corrección / formateo por voz con LLM | [`CORRECCION-VOZ.md`](CORRECCION-VOZ.md) | Implementado con proveedor local/cloud/custom, clasificación barata, diff validado, confirmación opcional y undo. |
| 2 | **P1** | Empaquetado y distribución | [`EMPAQUETADO.md`](EMPAQUETADO.md) | Hoy solo hay un `.exe` de release sin instalador, sin icono de archivo, sin firma. Necesario para compartir la app. |
| 3 | **P2** | Mejoras de UI | [`UI-FUTURA.md`](UI-FUTURA.md) | Cerrar los huecos del checklist 2026: trust signal "nada sale de tu PC", override visible de la IA, contraste AA del acento, accesibilidad. |
| 4 | **Hecho** | Cache del motor Whisper | `src/transcribe.rs` | Se conserva un contexto por modelo activo y se reemplaza solo al cambiar de modelo, evitando recargarlo en cada turno. |
| 5 | P3 (backlog) | Transcripción casi-tiempo-real | — | Ventanas solapadas + re-decodificación parcial. Riesgo alto, toca el núcleo de grabación; requiere prueba manual de micrófono. |
| 6 | P3 (backlog) | Notetaker / hablantes | — | Reuniones, identificación de hablantes. Fuera de alcance a corto plazo. |### Orden recomendado de trabajo

1. **P0 completado:** validar manualmente el flujo con micrófono, cambio de
   modelo, endpoint cloud/custom autenticado y confirmación de correcciones.
2. **P1 siguiente:** empaquetado y distribución: instalador, icono y firma.
3. **P2 siguiente:** revisión visual final, contraste AA y accesibilidad.
4. **P3 backlog:** transcripción casi en tiempo real y notetaker de reuniones.


---

## 3. Definición de "done" (heredada de `AGENTS.md`)

1. `cmd.exe /c "cd /d E:\AI\FREEBUFF && C:\Users\usuario\.cargo\bin\cargo.exe test"` → tests verdes.
2. `... cargo.exe check` → sin errores (warnings tolerados).
3. Commit por etapa lógica, mensaje en español.
