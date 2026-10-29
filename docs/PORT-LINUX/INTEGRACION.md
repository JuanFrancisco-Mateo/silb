# INTEGRACION — rama feature/port-linux (merge de las 4 partes del port)

Rama: `feature/port-linux` (desde `native-iced` 0499ffb) · Integrador:
dev-02 · Fecha: 03/10/2026

## 1. Qué se mergeó y en qué orden

Fusión de 4 ramas independientes (todas nacen de `native-iced` 0499ffb, ninguna
va encima de otra; las fusiones son entre hermanas):

| # | Rama | Commit | Conflicto | Resolución |
|---|---|---|---|---|
| 1 | `feature/port-linux-inventario` (8fa4905) | — merge limpio (fast-forward por contenido) | — | — |
| 2 | `feature/port-linux-codigo-core` (401c5e0) | 434245c | Cargo.toml | La rama P2 abrió `[target.'cfg(windows)'.dependencies]` y dejó engullidos arboard/tray-icon/cpal dentro; se resuelve a favor del layout de P1 (winapi ya en esa tabla al final, el resto universal) y se conserva la nota «GATEADO» de P2 en el comentario del winapi |
| 3 | `feature/port-linux-audio-runtime` (84d350b) | ed1d75a | Cargo.toml | La rama P3 dejó la tabla `[features]` tragándose arboard/enigo/global-hotkey/crossbeam-channel/ureq y la tabla `[target.'cfg(unix)'.dependencies]` tragándose rfd/serde/serde_json/dirs/fancy-regex/uuid/chrono. Se reordenan a sus secciones correctas |
| 4 | `feature/port-linux-dominio-ui` (678cdf2, solo en clon local upstream) | 8f40f24 | Cargo.toml, src/inject.rs, src/transcribe.rs | Ver abajo (los 3 conflicto real con adjudicacion de sentencias) |

## 2. El bug de las secciones TOML (corregido, no harcodeado)

Las ramas P2/P3/P4 meten, cada una por su lado y sin querer, el mismo patrón de
secciones que se desplazan: al mover una dependencia a una tabla distinta (solo
Windows, no-Linux, unix), la línea de la dependencia NO era la siguiente en el
orden que esperaba el editor y se colaba DENTRO de la tabla anterior — resultado:
dependencias normales quedaron dentro de `[features]` o dentro de una tabla cfg().

Estado final ordenado (decidido leyendo cada uso en src/):

| Seccion | Deps | Por qué |
|---|---|---|
| `[dependencies]` | iced, iced_winit, cpal, whisper-rs (default-features=false), arboard, global-hotkey, crossbeam-channel, ureq, serde, serde_json, dirs, fancy-regex, uuid, chrono | Multiplataforma o compilable en Linux. Ver §3 por cada una |
| `[features]` | SOLO `cuda = ["whisper-rs/cuda"]` (+ `default = []`) | CUDA off por defecto en ambos SO; build Windows con RTX 4070 necesita `--features cuda` |
| `[target.'cfg(windows)'.dependencies]` | winapi, enigo, rfd | No compilan en Linux (winapi API Windows; enigo solo backend X11; rfd requiere GTK runtime; en Linux el flujo rfd se deja PENDIENTE, ver §4.2) |
| `[target.'cfg(not(target_os = "linux"))'.dependencies]` | tray-icon | No compila en Linux sin GTK3+libappindicator+libxdo (Au Indicator/AppIndicator); tray.rs degrada a stub completo tras cfg (ver AUDIO-RUNTIME.md §3) |
| `[target.'cfg(unix)'.dependencies]` | libc | Señales POSIX del server LLM (llm.rs, `terminate_server_pid`: SIGTERM/SIGKILL), no compila en Windows (ver AUDIO-RUNTIME.md §5) |
| `[target.'cfg(windows)'.build-dependencies]` | winresource | Solo se usa en build.rs de Windows |

 Dicho de otro modo: `default-features = false` para whisper-rs no cambia nada en
la práctica (el default de whisper-rs 0.16 ya está vacío), se deja por dejarlo
claro: la compilación de Linux es solo CPU por diseño salvo `--features cuda`.

## 3. Adjudicaciones código de los conflictos P4

### src/inject.rs (conflicto `copy_to_clipboard`)

Quedó así tras la fusión (3 variantes que no se solapan, según configuración):

- `#[cfg(any(target_os = "windows", target_os = "macos"))]` → arboard.
  La rama P4 (dominio-ui) llegó a tener un `#[cfg(any(linux, macos))]`
  que dejaba Linux CON arboard; el integrador decide: Linux usa
  `wl-copy` (programa aparte, el camino del port P2/P3) y macOS usa arboard.
  La rama P3 deja el `#[cfg(unix)] wl-copy` que se tragaba también macOS:
  en macOS no existe wl-copy, por eso la cfg queda limitada a
  `target_os = "linux"`.
- `#[cfg(target_os = "linux")]` → wl-copy (stdin + wait + código de salida,
  comprobación del punto de partida P2).
- `#[cfg(not(any(target_os = "windows", target_os = "unix")))]` → apaño de error.

Decisión sobre arboard (con los 4 lados del conflicto):
- INVENTARIO.md (P1) recomienda arboard en Linux (la parte
  wayland-data-control está en Cargo.lock: smithay-clipboard 0.7.3).
- CODIGO-UI.md (P4) dice que copy_to_clipboard ya está portado a
  arboard en Linux (con data-control).
- AUDIO-RUNTIME/CODIGO-CORE (P2) implementan wl-copy (wl-clipboard,
  que ya está instalado en la máquina: wl-clipboard 2.3.0-1 comprobado en
  DEPENDENCIAS.md §6).
- Resolución final: Linux = wl-copy (P2, Wayland puro por programa aparte, sin
  depender de ningún crate) y macOS se queda con arboard (lo único que hay).
  arboard sigue siendo para todos en [dependencies] porque
  src/inject.rs lo usa en Windows (copia antes del enigo) y en el
  mac. Nota de la contradicción resuelta en la actualización de CODIGO-UI.md.

### src/transcribe.rs (conflicto doc-comment + cfg de `models_dir`)

P4 trajo la variante correcta: `#[cfg(target_os = "windows")]` en la
función `models_dir()` (usando `dirs::data_dir()`) y una segunda
`#[cfg(not(target_os = "windows"))]` con `crate::paths::app_data_dir()`
(XDG data de P4). HEAD (con las fusiones P2+P3) tenía el comentario
incorrecto (decía que la función era igual en todos los sistemas sin cfg y al
cambiarla las dos implementaciones chocaban). Decidido: P4 (las dos variantes) y
el comentario de P4.

## 4. Coherencia Cargo.toml ↔ código (dinámica de compilado)

### 4.1 Declaraciones por cfg en Cargo.toml

- `arboard` DECLARADA para todos — PERMITIDO: los usos reales son en
  inject.rs `copy_to_clipboard` de `#[cfg(any(windows, macos))]`
  (línea 218) y la función `inject_text_into_active_app` solo de Windows
  (línea 285). En Linux el port P2 define copy_to_clipboard con
  wl-copy. En Linux, con arboard declarada como dependencia para todos, el crate
  compila (la parte wayland de smithay está en Cargo.lock) aunque el
  código de Linux no lo llame.
- `global-hotkey` DECLARADA para todos: hay usos SIN cfg en
  app.rs (línea 31 y 456) y en ui (poll.rs — también global_hotkey), y la
  parte Linux (socket de hyprland) construye GlobalHotKeyEvent (línea 332
  de hotkey.rs). Compila en Linux sin libX11 (x11-dl se carga al ejecutar). NOTA:
  en una sesión Wayland pura la parte X11 del crate no registra el atajo; el port
  de verdad del atajo global es la parte hyprland (ver HOTKEY-HYPR.md).
- `crossbeam-channel` para todos: hotkey.rs/app.rs lo usan; el apaño de Linux
  de tray.rs usa la ruta completa `crossbeam_channel::never()` (línea
  156 de tray.rs) — la dependencia para todos no se limita.
- `winapi/enigo/rfd/tray-icon/libc/winresource`: secciones correctas por
  cfg con revisión de uso (ver tabla sección 2).

### 4.2 Deuda declarada (PENDIENTE, cargo check del usuario)

Es ESPERABLE que cargo check en Linux falle o avise en:
- `rfd`: usado SIN cfg en src/ui/settings_handlers.rs (6 usos) y
  apuntado como TODO en CODIGO-UI.md (línea 32-34). Declarada como
  dependencia solo de Windows en el Cargo.toml integrado -> EN LINUX el módulo
  cuenta con el apaño declarado de settings_handlers (sin GTK). Si el
  cargo check de Linux falla aquí, es el PENDIENTE conocido que se ataca en
  CODIGO-UI.md pendiente n. 2, no un fallo de la fusión.
- `enigo`: igual — usado solo tras `#[cfg(windows)]` en inject.rs
  (línea 304), la declaración que pasó de todos a solo Windows en la fusión hace
  que el uso de Windows no cambie y Linux no lo compile (no hay uso
  sin cfg en Linux).
- `arboard` queda declarada para todos (ver §3): compila en Linux.

## 5. Documentos: gobierno de la coherencia

Las versiones distintas de los mismos ficheros NO se han fusionado a ojo:
- Cada documento vive en su rama de origen (INVENTARIO/PLAN de P1; CODIGO-CORE
  de P2; AUDIO-RUNTIME/HOTKEY-HYPR de P3; CODIGO-UI/DEPENDENCIAS/SCRIPTS
  de P4) y llega a la rama integrada con su contenido entero.
- Contradicciones encontradas y resueltas (documento de referencia
  final: este INTEGRACION.md):
  1. copy_to_clipboard en Linux: P2/P3 dicen wl-copy; P4 (CODIGO-UI.md
     línea 32-34) dice arboard. **Decidido: wl-copy + cfg(linux)**
     (es lo que hace el código integrado; CODIGO-UI.md queda
     como descripción de lo que se decidió en la nota).
  2. bandeja en Linux: CODIGO-UI.md pendiente n. 3 dice "camino pendiente
     de probar" con libappindicator; AUDIO-RUNTIME.md §3 no hace nada y sin
     dependencia GTK. **Decidido: AUDIO-RUNTIME.md** (así está el
     código integrado: tray.rs 100% tras cfg(not(linux)) y la dependencia
     tray-icon es `cfg(not(target_os = "linux"))`).
  3. dependencias solo de Windows: P1 FINAL dice tray-icon/winapi/enigo/
     global-hotkey/crossbeam/rfd en `cfg(windows)`; AUDIO-RUNTIME.md
     dice global-hotkey+crossbeam para todos. **Decidido:
     AUDIO-RUNTIME.md** (global-hotkey y crossbeam para todos — hay
     usos sin cfg en app.rs; tray-icon `not(linux)`; enigo/rfd/winapi
     `cfg(windows)`).
  4. atajo: HOTKEY-HYPR.md y AUDIO-RUNTIME.md coinciden (parte
     hyprland + socket UNIX; la parte X11 del crate no funciona en
     Wayland). Sin contradicción.

## 6. Limites de la validacion (HONESTIDAD)

Validación ESTRICTAMENTE ESTÁTICA (mi tarea no tiene herramientas de Rust):
- TOML leído con tomllib (Python): válido, secciones no duplicadas,
  estructura correcta (ver §2).
- Usos de cada crate comprobados con grep en src/ (ver §4).
- Marcas de conflicto de git en los 3 ficheros conflictivos: 0 tras
  resolverlos (comprobado con grep del patrón de marca de conflicto).
- DELIBERADAMENTE NO: `cargo check` / `cargo build` (sin herramientas en
  esta máquina, y la tarea lo prohíbe). El primer `cargo check` en
  Linux lo corre el usuario después de instalar rustup+cmake
  (`sudo pacman -S --needed rustup cmake base-devel; rustup default
  stable`). Si falla, los mensajes de cfg/API que se describen aquí
  son el mapa para el primer diagnóstico.
- Cargo.lock NO se regeneró (hace falta cargo); el primer cargo check
  actualizará el lock al cambiar las opciones por sistema.

## 7. Nota sobre P4 y push

- `feature/port-linux-dominio-ui` (P4) NO estaba subida a GitHub al
  hacer esta integración; se sacó del clon local compartido
  (`~/hermes-team/repos/whispr-open`, añadido como remote `upstream`
  del clon de trabajo). Esta integración SÍ sube `feature/port-linux`
  a GitHub.
- La nota de P4 en AUDIO-RUNTIME.md §9 («la tarea hermana Parte 1 no
  estaba subida a origin») quedó vieja: P1 ya está en GitHub
  (8fa4905); las dos fusiones van con este INTEGRACION.md como prueba.
