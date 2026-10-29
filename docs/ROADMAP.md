# ROADMAP — Silb (rama `native-iced`)

Estado comprobado a 11/09/2026 contra el código y el historial de git. Este
archivo cuenta **qué hay hecho** y **qué viene después**, ordenado por impacto y
riesgo. Los documentos hermanos (`CORRECCION-VOZ.md`, `EMPAQUETADO.md`,
`UI-FUTURA.md`) explican cada etapa con detalle.

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
| Post-procesado | `src/domain/{cleanup,dictionary,correction}.rs` | Limpieza de repeticiones/expresiones con muletillas configurables (Ajustes → Muletillas, toggle «Limpiar muletillas»), diccionario personal y rectificaciones habladas. |
| Corrección IA configurable | `src/config.rs`, `src/llm.rs`, `src/app.rs` | Proveedor local/cloud/custom OpenAI-compatible, timeout, umbral, confirmación opcional y API key fuera de `config.json`. |
| Audio robusto | `src/audio.rs` | El stream se crea y reproduce antes de entrar en `Recording`; todo audio no-16 kHz se remuestrea a 16 kHz. |
| Build release | `target/release/silb.exe` | Presente (~43 MB), sin consola (`windows_subsystem`), log a archivo. |
| Licencia y documentación | `LICENSE`, `README.md`, `docs/` | MIT (10/09/2026); `docs/ARQUITECTURA.md` (recorrido del código para quien no ha tocado Rust), `docs/GUIA-USUARIO.md` y doc-comments públicos en español (11/09/2026). |

Versión actual del paquete: **0.7.0** (`Cargo.toml`). Rama de la release:
`release/0.7.0` (la rama `main` es la versión web antigua y desde aquí no se
toca).

> **Actualización 26/10/2026 (0.5.0)**: el roadmap de abajo se escribió en
> septiembre y sus referencias de versión (0.2.0) están viejas. Lo que ha
> entrado desde entonces: instalador Inno Setup con .ico y CUDA opcional,
> rediseño de la interfaz salido de Figma, servidor de IA metido en el paquete, y
> el **port a Linux integrado** (ver `docs/PORT-LINUX/`). Lo único que sigue abierto de este
> documento es la lista de pendientes P3 (transcripción casi en tiempo real,
> notetaker) y los puntos de `UI-FUTURA.md`.

Comprobación de hoy (11/09/2026): `cargo.exe test` → **271 passed, 0 failed,
1 ignored** (el ignorado necesita un servidor llama.cpp de verdad encendido);
`cargo.exe check` sin errores.

### Cadena de procesado actual (por turno de dictado)

```
micrófono (cpal, f32 mono, cualquier rate → 16 kHz)
  → whisper.cpp (modelo elegido, idioma configurable, beam search, CUDA)
  → limpieza local configurable (repeticiones/muletillas en Ajustes)
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
| 6 | P3 (backlog) | Notetaker / hablantes | — | Reuniones, identificación de hablantes. Fuera de alcance a corto plazo. |
| 7 | **Hecho** | Elegir el modelo de IA desde Ajustes, descargándolo de Hugging Face | [`UI-FUTURA.md`](UI-FUTURA.md) §8 | Implementado en la 0.7: desplegable con lista curada + «Otro» (repo o URL), botón «Descargar» con progreso y descarga a `models-llm/` (commits `1c0e7c9`, `7981f3c`, `f7ec5f0`). |

### Orden recomendado de trabajo

1. **P0 completado:** probar a mano el recorrido con micrófono, el cambio de
   modelo, un servidor en la nube o propio con autenticación y la confirmación
   de las correcciones.
2. **P2 completado** (R3/R4, commit `7f2109e`): nota editable, contraste AA
   medido, transparencia del corrector con IA y foco visible.
3. **R5 completado** (11/09/2026): licencia MIT, `docs/ARQUITECTURA.md`, guía de
   usuario ampliada y comentarios de documentación en español en lo público.
4. **P1 siguiente:** empaquetado y distribución: instalador, icono `.ico`,
   versión portable y firma (fase R6 de `NOTA-SESION.md`).
5. **P3 pendiente:** transcripción casi en tiempo real y notetaker de reuniones.


---

## 3. Definición de "done" (heredada de `AGENTS.md`)

1. `cmd.exe /c "cd /d whispr-open && cargo test"` → tests verdes.
2. `... cargo.exe check` → sin errores (avisos tolerados).
3. Un commit por etapa lógica, con el mensaje en español.
