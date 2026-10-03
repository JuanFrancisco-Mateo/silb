# INTEGRACION — rama feature/port-linux (merge de las 4 partes del port)

Rama: `feature/port-linux` (desde `native-iced` 0499ffb) · Integrador:
dev-02 · Fecha: 03/10/2026

## 1. Qué se mergeó y en qué orden

Merge de 4 ramas independientes (todas nacen de `native-iced` 0499ffb,
ninguna apila sobre otra; los merges son entre hermanas):

| # | Rama | Commit | Conflicto | Resolución |
|---|---|---|---|---|
| 1 | `feature/port-linux-inventario` (8fa4905) | — merge limpio (fast-forward por contenido) | — | — |
| 2 | `feature/port-linux-codigo-core` (401c5e0) | 434245c | Cargo.toml | La rama P2 abrió `[target.'cfg(windows)'.dependencies]` y dejó engullidos aturos/tray/audio dentro; se resuelve a favor del layout de P1 (winapi ya en esa tabla al final, el resto universal) y se conserva la nota «GATEADO» de P2 en el comentario del winapi |
| 3 | `feature/port-linux-audio-runtime` (84d350b) | ed1d75a | Cargo.toml | La rama P3 dejó la tabla `[features]` tragándose arboard/enigo/global-hotkey/crossbeam-channel/ureq y la tabla `[target.'cfg(unix)'.dependencies]` tragándose rfd/serde/serde_json/dirs/fancy-regex/uuid/chrono. Se reordenan a sus secciones correctas |
| 4 | `feature/port-linux-dominio-ui` (678cdf2, solo en clon local upstream) | 8f40f24 | Cargo.toml, src/inject.rs, src/transcribe.rs | Ver abajo (los 3 conflicto real con adjudicacion de sentencias) |

## 2. El bug de las secciones TOML (corregido, no harcodeado)

Las ramas P2/P3/P4 introducen, de forma independiente y sin querer, el mismo
patrón de desplazamiento de secciones: al posible mover una dep a una tabla
intro (win-only, win-not-linux, unix), la línea de dependencia NO era la
siguiente en el orden que esperaba el editor y se filtró INTO la tabla
anterior — resultado: deps normales quedaron dentro de [features] o dentro
de una tabla cfg().

Estado final normalizado (adjudicado leyendo cada uso en src/):

| Seccion | Deps | Por qué |
|---|---|---|
| `[dependencies]` | iced, iced_winit, cpal, whisper-rs (default-features=false), arboard, global-hotkey, crossbeam-channel, ureq, serde, serde_json, dirs, fancy-regex, uuid, chrono | Multiplataforma o compilable en Linux. Ver §3 por cada una |
| `[features]` | SOLO `cuda = ["whisper-rs/cuda"]` (+ `default = []`) | CUDA off por defecto en ambos SO; build Windows con RTX 4070 necesita `--features cuda` |
| `[target.'cfg(windows)'.dependencies]` | winapi, enigo, rfd | No compilan en Linux (winapi API Windows; enigo solo backend X11; rfd requiere GTK runtime; en Linux el flujo rfd se deja PENDIENTE, ver §4.2) |
| `[target.'cfg(not(target_os = "linux"))'.dependencies]` | tray-icon | No compila en Linux sin GTK3+libappindicator+libxdo (Au Indicator/AppIndicator); tray.rs degrada a stub completo tras cfg (ver AUDIO-RUNTIME.md §3) |
| `[target.'cfg(unix)'.dependencies]` | libc | Señales POSIX del server LLM (llm.rs, `terminate_server_pid`: SIGTERM/SIGKILL), no compila en Windows (ver AUDIO-RUNTIME.md §5) |
| `[target.'cfg(windows)'.build-dependencies]` | winresource | Solo se usa en build.rs de Windows |

 Bingo: `default-features = false` para whisper-rs es un no-op efectivo
 (default de whisper-rs 0.16 ya es vacío), se deja por explícito: el build
 de Linux es CPU-only por diseño salvo `--features cuda`.

## 3. Adjudicaciones código de los conflictos P4

### src/inject.rs (conflicto `copy_to_clipboard`)

Quedó así tras el merge (3 variantes config-disjuntas, sin solape):

- `#[cfg(any(target_os = "windows", target_os = "macos"))]` → arboard.
  La rama P4 (dominio-ui) llegó a tener un `#[cfg(any(linux, macos))]`
  que dejaba Linux CON arboard; el integrador decide: Linux usa
  `wl-copy` (subproceso, flujo del port P2/P3) y macOS usa arboard.
  La rama P3 deja el `#[cfg(unix)] wl-copy` que engullía también a macOS:
  en macOS no existe wl-copy, por eso queda la cfg restringida a
  `target_os = "linux"`.
- `#[cfg(target_os = "linux")]` → wl-copy (stdin + wait + exit code,
  verificación del punto de partida P2).
- `#[cfg(not(any(target_os = "windows", target_os = "unix")))]` → stub Err.

Adjudicación arboard (decisión con los 4 lados del conflicto):
- INVENTARIO.md (P1) recomienda arboard en Linux (backend
  wayland-data-control presente en Cargo.lock: smithay-clipboard 0.7.3).
- CODIGO-UI.md (P4) afirma que copy_to_clipboard ya está portado a
  arboard en Linux (PWM con data-control).
- AUDIO-RUNTIME/CODIGO-CORE (P2) implementan wl-copy (wl-clipboard,
  ya instalado en el host: wl-clipboard 2.3.0-1 verificado en
  DEPENDENCIAS.md §6).
- Resolución final: Linux = wl-copy (P2, portable Wayland puro por
  subproceso, sin dependencias de crate) y macOS le deja arboard (único
  backend disponible). arboard sigue UNIVERSAL en [dependencies] porque
  src/inject.rs lo usa en Windows (copia antes del enigo) y en el
  mac stub. Nota de contradicción adjudicada en CODIGO-UI.md update.

### src/transcribe.rs (conflicto doc-comment + cfg de `models_dir`)

P4 trajo la variante correcta: `#[cfg(target_os = "windows")]` en la
función `models_dir()` (usando `dirs::data_dir()`) y una segunda
`#[cfg(not(target_os = "windows"))]` con `crate::paths::app_data_dir()`
(XDG data de P4). HEAD (dado la merge P2+P3) tenía el doc-comment
incorrecto (decía que la función era universal sin cfg y onChange ambas
implementaciones quedaban en conflicto). Adjudicado: P4 (ambas variantes),
doc-comment de P4.

## 4. Coherencia Cargo.toml ↔ código (dinámica de compilado)

### 4.1 Declaraciones por cfg en Cargo.toml

- `arboard` DECLARADA universal — PERMITIDO: los usos reales son en
  inject.rs `copy_to_clipboard` de `#[cfg(any(windows, macos))]`
  (linea 218) y la función `inject_text_into_active_app` Windows-only
  (linea 285). En Linux el port P2 define copy_to_clipboard con
  wl-copy. En Linux con arboard declarada como dep universal, el crate
  compila (backend wayland smithay incluido en Cargo.lock) aunque el
  código Linux no lo llame.
- `global-hotkey` DECLARADA universal: hay usos SIN cfg en
  app.rs (linea 31 y 456) y ui (poll.rs — también global_hotkey), y el
  backend Linux (hyprland socket) fabrica GlobalHotKeyEvent (linea 332
  de hotkey.rs). Compila en Linux sin libX11 (x11-dl es dlopen en
  runtime). NOTA: en una sesion Wayland pura el backend X11 del crate
  no registra el atajo; port real del atajo global = backend hyprland
  (ver HOTKEY-HYPR.md).
- `crossbeam-channel` universal: hotkey.rs/app.rs lo usan; el stub Linux
  de tray.rs usa la ruta completa `crossbeam_channel::never()` (linea
  156 de tray.rs) — la dependencia universal no se gateguea.
- `winapi/enigo/rfd/tray-icon/libc/winresource`: secciones correctas por
  cfg con audit de uso (ver tabla sección 2).

### 4.2 Deuda declarada (PENDIENTE, cargo check del usuario)

El cargo check en Linux está ESPERADO que falle/avise en:
- `rfd`: usado SIN cfg en src/ui/settings_handlers.rs (6 usos) y
  registrado como TODO en CODIGO-UI.md (línea 32-34). Declarada como
  dep windows-only en el Cargo.toml integrado -> EN LINUX el modulo
  cuenta con el stub declarado de settings_handlers (sin GTK). Si el
  cargo check de Linux falla aqui, es el PENDIENTE conocido atacado en
  CODIGO-UI.md pendiente n. 2, no una regression del merge.
- `enigo`: igual — usado solo tras `#[cfg(windows)]` en inject.rs
  (linea 304), la declaracion universal→windows-only del merge hace
  que el uso Windows no cambie y Linux no lo compile (no hay uso
  sin cfg en Linux).
- `arboard` queda declarada universal (ver §3): compila en Linux.

## 5. Documentos: gobierno de la coherencia

Versiones distintas de los mismos ficheros NO se han fusionado a ojo:
- Cada doc vive en su rama de origen (INVENTARIO/PLAN de P1; CODIGO-CORE
  de P2; AUDIO-RUNTIME/HOTKEY-HYPR de P3; CODIGO-UI/DEPENDENCIAS/SCRIPTS
  de P4) y llega a la rama integrada con su contenido íntegro.
- Contradicciones detectadas y adjudicadas (documento de referencia
  final: este INTEGRACION.md):
  1. copy_to_clipboard en Linux: P2/P3 dicen wl-copy; P4 (CODIGO-UI.md
     linea 32-34) dice arboard. **Adjudicado: wl-copy + cfg(linux)**
     (implementación real del código integrado; CODIGO-UI.md queda
     como describes-decindir en la nota).
  2. tray en Linux: CODIGO-UI.md pendiente n. 3 dice "camino pendiente
     de probe" con libappindicator; AUDIO-RUNTIME.md §3 dice stub sin
     dependencia GTK. **Adjudicado: AUDIO-RUNTIME.md** (así está el
     código integrado: tray.rs 100% tras cfg(not(linux)) y la dep
     tray-icon es `cfg(not(target_os = "linux"))`).
  3. dependencias Windows-only: P1 FINAL dice tray-icon/winapi/enigo/
     global-hotkey/crossbeam/rfd en `cfg(windows)`; AUDIO-RUNTIME.md
     dice global-hotkey+crossbeam universal. **Adjudicado:
     AUDIO-RUNTIME.md** (global-hotkey y crossbeam universales — hay
     usos sin cfg en app.rs; tray-icon `not(linux)`; enigo/rfd/winapi
     `cfg(windows)`).
  4. hotkey: HOTKEY-HYPR.md y AUDIO-RUNTIME.md coinciden (backend
     hyprland + socket UNIX; el backend X11 del crate no funciona en
     Wayland). Sin contradicción.

## 6. Limites de la validacion (HONESTIDAD)

Validación ESTRICTAMENTE ESTATICA (mi task no tiene toolchain Rust):
- TOML parseado con tomllib (Python): válido, secciones no duplicadas,
  estructura correcta (ver §2).
- Usos de cada crate verificados por grep en src/ (ver §4).
- Marcadores de conflicto de git en los 3 ficheros conflictivos: 0 tras
  la resolución (verificado por grep con patrón de conflict-marker).
- DELIBERADAMENTE NO: `cargo check` / `cargo build` (sin toolchain en
  esta máquina, prohibido por la task). El primer `cargo check` en
  Linux lo corre el usuario tras instalar rustup+cmake
  (`sudo pacman -S --needed rustup cmake base-devel; rustup default
  stable`). Si falla, los mensajes de cfg/API/detección aquí descritos
  son el mapa de primer diagnóstico.
- Cargo.lock NO se regeneró (requiere cargo); el primer cargo check
  actualizará el lock al cambiar features por target.

## 7. Nota sobre P4 y push

- `feature/port-linux-dominio-ui` (P4) NO estaba pusheada a GitHub al
  hacer esta integración; se obtuvo del clon local compartido
  (`~/hermes-team/repos/whispr-open`, añadido como remote `upstream`
  del clon de trabajo). Esta integración SÍ empuja `feature/port-linux`
  a GitHub.
- La nota de P4 en AUDIO-RUNTIME.md §9 («la tarea hermana Parte 1 no
  estaba pusheada a origin») quedó desfasada: P1 ya está en GitHub
  (8fa4905); ambos merges van con este INTEGRACION.md como prueba.
