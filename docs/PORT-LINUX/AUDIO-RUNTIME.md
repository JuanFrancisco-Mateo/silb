# Runtime de audio/transcripción/tray/LLM en Linux (port t_7ac526a3)

Estado por módulo (PALABRAS HONESTAS: nada de esto se ha compilado ni
medido en esta máquina — regla de la task: sin `cargo build`; la validación
real de build/micrófono/teclado la ejecuta el usuario):

| Módulo      | Estado                        | Qué falta para "listo"                    |
|-------------|-------------------------------|-------------------------------------------|
| audio.rs    | a validar (build del usuario) | build + probar micrófono real             |
| transcribe.rs | a validar (build del usuario) | build + descarga de GGML + transcribir  |
| tray.rs     | pendiente backend de sistema  | decidir host de tray + instalar libs GTK  |
| hotkey.rs   | listo a validar (Hyprland)    | build + bind en hyprland.conf (ver HOTKEY-HYPR.md) |
| llm.rs      | listo a validar (build)       | build + probar arranque de llama-server   |
| main.rs / build.rs | listo (trivial)        | build                                     |

## 1. audio.rs — captura con cpal 0.15

- Host Linux: cpal 0.15 habla ALSA (hosts Linux = ALSA/JACK; crates.io
  cpal 0.15.3). En Omarchy el audio del sistema es PipeWire y la app llega
  a él por el plugin `pipewire-alsa`: app → cpal/ALSA → pipewire-alsa →
  PipeWire. NO se activa la feature opcional `pipewire` de cpal (evitaría
  requerir libpipewire-0.3-dev en build; no aporta nada para capturar el
  micro por los mismos canales).
- Buffer: la app usa el config por defecto del dispositivo
  (`default_input_config()`), es decir `BufferSize::Default`. Para CAPTURA
  de dictado (buffer por turnos, no streaming en vivo) el tamaño del
  buffer de entrada no añade latencia perceptible: se consume todo al
  parar el turno.
- «Bug conocido de cpal 0.15 en Wayland con sample rates altos»: NO se ha
  encontrado ese bug como tal (Wayland no interviene en el audio; los
  hosts son ALSA/PipeWire). Lo que SÍ existe y está documentado:
  - cpal issue #1029 (y familia #1033/#1036): con pipewire-alsa y
    `BufferSize::Default`, PIPEWIRE configura buffers enormes y el
    PLAYBACK (no la captura) sufre retrasos de segundos. No aplica a la
    ruta de captura del dictado; si el usuario viera síntomas análogos en
    captura, el fix sería pedir `BufferSize::Fixed(1024)` al construir el
    stream (una línea en `build_stream`), decisión pendiente de síntomas
    reales, NO aplicada de antemano.
  - La enumeración de dispositivos por ALSA puede dar nombres técnicos
    («hw:0,0», perfiles de pipewire-alsa) en vez de nombres amables de
    Windows. La UI ya lista `input_devices()` tal cual; el selector
    funciona, solo cambia el formato de los nombres.
- Código: sin `#[cfg]` Linux/Windows: cpal es multiplataforma y la lógica
  existente (fallback H6 de dispositivo, normalize_gain, resample a 16k,
  tope de 2 h) es pura y ya testada. Único cambio: log de arranque con
  rate/canales/formato/host para diagnosticar a la primera.
- Qué probar el usuario (checklist):
  1. `cargo build --release` (o run).
  2. Ajustes → selector de micrófono: deben aparecer los perfiles ALSA.
  3. Grabar un turno corto y verificar que el medidor de nivel se mueve y
     la transcripción llega (ver §2).
  4. Si el micro da «dispositivo ocupado»: probar `pw-cli` (¿PipeWire
     vivo?) y cerrar apps que retengan el micro (navegador con Webcam).

## 2. transcribe.rs — whisper-rs 0.16, CUDA opcional

- La feature `cuda` de whisper-rs (docs.rs/lib.rs whisper-rs: "cuda:
  enable CUDA support. Implicitly enables hidden GPU flag at runtime") ya
  NO va metida a fuego en Cargo.toml: ahora es una feature de Cargo del
  propio crate (ver `[features] cuda` en Cargo.toml), OFF por defecto:

      cargo build --release              # CPU-only (default Linux)
      cargo build --release --features cuda   # con CUDA (requiere nvcc)

  En Windows el flujo anterior (cuda por defecto) se sustituye por el
  mismo flag explícito. Sin código cfg en transcribe.rs: los mismos
  `FullParams` aplican con y sin CUDA (whisper.cpp decide backend interno
  al construir el contexto).
- No hay código que asuma paths de GPU ni cuda throwaway en el módulo
  (grep hecho: `cuda|GPU|gpu` solo toca comentarios de documentación).
  No había nada que envolver en cfg(feature="cuda") en el código — la
  feature vive en Cargo.toml.
- Velocidad CPU-only en el portátil (Ryzen 5 PRO 5650U, 6C/12T, 14 GiB):
  ESTIMACIÓN HONESTA, NO MEDIDA (la task lo prohíbe y no hay benchmarks
  inventados). Referencias públicas para calibrar expectativas:
  - whisper.cpp (README del repo ggerganov/whisper.cpp) publica benchmarks
    CPU de small en x86_64 con AVX en el rango de ~2-4x tiempo real
    (varía mucho por CPU, threading y beam size). Con n_threads=4..6 y
    beam=5 (los defaults de la app), lo esperable en el Ryzen es que un
    turno hablado de 30 s tarde del orden de 10-30 s en small; tiny/base
    bajan mucho más.
  - La medición REAL en este portátil la deja el usuario (criterio de la
    task): transcribir un turno de duración conocida con `small` y
    comprobar el factor de escala en el log (whisper.cpp imprime
    «whisper_full: ... time = ...»).
  - Si small CPU no diera la talla, tiny/base están en el selector de la
    UI sin cambios de código.
- Modelos: mismos GGML de HuggingFace (repo ggerganov/whisper.cpp). La
  carpeta de destino pasa de `%APPDATA%\whispr-open\models` (Windows) a
  `~/.local/share/whispr-open/models` (Linux) vía `dirs::data_dir()` —
  mapping XDG estándar, sin cfg en el código. VAD (`ggml-vad-q8.bin`)
  vive en la misma carpeta y sigue siendo opcional.

## 3. tray.rs — BANDEJA PENDIENTE en Linux (decisión honesta)

- tray-icon 0.24 en Linux SOLO soporta el backend AppIndicator
  (docs.rs/tray-icon 0.24.2: «Linux (gtk Only)»; dependencias Linux:
  gtk3 + libappindicator/libayatana-appindicator + libxdo — pacman:
  `gtk3 libappindicator-gtk3 libxdo`).
- Fragilidad doble: (1) el build exige esas librerías GTK3; (2) en
  runtime exige un HOST de StatusNotifierItem (una barra que pinte iconos
  SNI). Hyprland no trae nada de esto de fábrica (una barra tipo
  waybar/eww con soporte appindicator sería el host).
- DECISIÓN del port: en Linux NO se compila la dependencia
  (`[target.'cfg(not(target_os = "linux"))'.dependencies] tray-icon` en
  Cargo.toml) y `src/tray.rs` ofrece un stub bajo
  `#[cfg(target_os = "linux")]` que cumple el MISMO contrato que el tray
  real (constantes MENU_*, `SystemTray::new()` que devuelve Ok con
  log avisando, campos status_item/toggle_item con `set_text()` no-op y
  canal `events` siempre vacío). `app.rs` (fuera del reparto de este
  port) no necesita cfg.
- Efecto en el usuario Linux: sin icono en bandeja; todo el flujo vive en
  la ventana principal (dictado, ajustes) + atajo (§ hotkey) + botón
  salir de la propia UI. La ventana sigue ocultable; para recuperarla,
  relanzar la app NO abre una segunda instancia cuando el guard de
  instancia única de Linux exista (single_instance.rs — módulo de OTRO
  agente, su stub actual devuelve true siempre).
- Reactivar el tray en el futuro (checklist):
  1. `sudo pacman -S gtk3 libappindicator-gtk3 libxdo`
  2. Quitar el cfg-gate de `tray-icon` en Cargo.toml (mover la dep al
     bloque global).
  3. Quitar el `#[cfg]` del bloque real de `src/tray.rs` (el stub y el
     bloque real conviven documentados; el real reutiliza `circle_rgba`).
  4. Confirmar que el host de tray (waybar + SNI) está corriendo.

## 4. hotkey.rs — backend "hyprland" (socket UNIX)

Ver doc específica docs/PORT-LINUX/HOTKEY-HYPR.md (snippet exacto de
hyprland.conf, checklist de prueba, permisos). Resumen:
- Windows: `global-hotkey` igual que siempre (cfg not(linux)).
- Linux/Wayland: la app escucha `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock`
  y un bind de Hyprland le escribe `toggle`:

      bind = $mainMod SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

- El evento viaja por el mismo tipo de canal que en Windows
  (`GlobalHotKeyEvent` con campos públicos id/state — verificado en
  docs.rs/global-hotkey 0.6): app.rs/ui/poll.rs no distinguen backends.
- La dependencia `global-hotkey` NO se cfg-queda a Windows: app.rs y
  ui/poll.rs (ficheros de otros agentes) importan `GlobalHotKeyEvent`
  sin cfg; el backend Linux del crate es dlopen (x11-dl) y compila sin
  libX11 presente. En una sesión X11 (GNOME/XFCE) el grab de X11
  funcionaría sin tocar nada (HotkeyManager::register usa el mismo
  camino del crate).

## 5. llm.rs — server LLM en Linux

- SERVER_EXE_NAME pasa a cfg: `llama-server.exe` (Windows) /
  `llama-server` (Linux). is_server_exe/sanitize_server_exe usan la
  constante (tests duplicados por target).
- launch_server: implementación Linux real (antes devolvía «solo
  Windows»): Command::new(exe) con cwd = dir del exe, stdout/stderr →
  `llama-server.log` junto a la config (misma mecánica que Windows,
  sin CREATE_NO_WINDOW), PID registrado en LAUNCHED_SERVER_PID.
- terminate_server_pid Linux: salva guarda por imagen leyendo
  `/proc/<pid>/exe` (solo se mata si se llama `llama-server`), SIGTERM y
  si sigue vivo 2 s → SIGKILL. Nueva dependencia `libc = "0.2"` SOLO en
  `[target.'cfg(unix)'.dependencies]` (kill(2); en Windows no compila).
- Autodetección de rutas (default_server_roots) por SO:
  - Windows (sin cambios): D:\LlamaCPP, C:\LlamaCPP, junto al exe,
    %LOCALAPPDATA%\LlamaCPP, PATH.
  - Linux: junto al exe de la app → `$HOME/.local/bin/llama-server` →
    PATH → `$XDG_CONFIG_HOME/whispr-open/llama-server` (default
    `~/.config/whispr-open/`).
- Modelos GGUF (default_model_roots): `~/.lmstudio/models` (igual que
  Windows relativo a $HOME), `~/.lmstudio/models/lmstudio-community`,
  NUEVO `~/.config/whispr-open/models` (Linux), y las raíces de unidades
  C:/D: solo Windows (no dañan en Linux: no existen).
- Flags del server (`build_server_args`): IGUALES en ambos SO
  (`-ngl 99 --flash-attn on -ctk q8_0 -ctv q8_0 -c 8192`). En un build
  CPU-only de llama.cpp esos flags de GPU se degradan con warning del
  server; con `--features cuda` son los que encienden la GPU. Test
  duplicado por target.
- Qué probar el usuario:
  1. Instalar/colocar `llama-server` (build de llama.cpp) en uno de los
     paths de autodetección o poner la ruta en Ajustes.
  2. Arrancar la app y verificar el log: «server LLM lanzado (pid N)» +
     «/v1/models respondiendo».
  3. Probar una corrección hablada («digo X») y el cierre limpio de la
     app (el server propio debe morir: comprobar con `pgrep llama-server`).

## 6. main.rs y build.rs

- main.rs: `#![windows_subsystem = "windows"]` pasa a
  `#![cfg_attr(windows, windows_subsystem = "windows")]` (el atributo
  puro rompe la utilidad del binario en Linux). El resto del arranque ya
  era multiplataforma: single_instance tiene stubs no-Windows (módulo de
  otro agente) y el run de iced_winit no toca nada Windows.
- build.rs: ya tenía el bloque `#[cfg(not(windows))]` (no-op + rerun).
  El camino Windows (winresource + regeneración de .ico) solo compila en
  Windows: `winresource` está en `[build-dependencies]` global, PERO su
  uso está dentro de `#[cfg(windows)] fn main()`; en Linux el `main()`
  cfg-izado ni lo referencia, y cargo no enlaza winresource fuera del
  build script (no tira). Se deja tal cual: es el patrón estándar.

## 7. Cambios en Cargo.toml (resumen para review)

- `whisper-rs`: `features = ["cuda"]` → `default-features = false` +
  `[features] cuda = ["whisper-rs/cuda"]` (OFF por defecto, ambos SO).
- `tray-icon` movido a `[target.'cfg(not(target_os = "linux"))'.dependencies]`
  (Linux sin tray: ver §3).
- `winapi` movido a `[target.'cfg(target_os = "windows")'.dependencies]`
  (no compila fuera de Windows).
- `global-hotkey` y `crossbeam-channel`: SIN cambios de sección (ver §4:
  app.rs los usa sin cfg; x11-dl es dlopen).
- NUEVO `[target.'cfg(unix)'.dependencies] libc = "0.2"` (kill del server
  LLM; ver §5).
- Nota de verificación: en whisper-rs 0.16 la feature `cuda` EXISTE y el
  `default` ya va VACÍO (Cargo.toml.orig de 0.16.0: `default = []`), de
  modo que `default-features = false` esencialmente es un no-op que se
  mantiene por explícito (declarar que el port es CPU-only salvo
  `--features cuda`). Corregido en review del run 6.

## 8. Correcciones del run 6 (revisión del run anterior)

- src/tray.rs (stub Linux): `crossbeam_channel::never().1` → `never()`
  (la función devuelve `Receiver<T>` directamente, no una tupla —
  verifico contra docs.rs/crossbeam-channel; el `.1` NO compila).
- Los llaves/paréntesis de los ficheros del reparto verificados con
  parser básico (sin compilar, regla de la task): balance 0 en todos.
- Verifico en fuentes del crate (docs.rs global-hotkey 0.6.4 lib.rs) que
  `GlobalHotKeyEvent { pub id: u32, pub state: HotKeyState }` es
  construible en el listener Linux (sin `#[non_exhaustive]`): ok.

## 9. Pendientes explícitos (honestidad de la task)

- Build real del binario en Linux (prohibido al agente): el usuario debe
  correr `cargo build --release` y reportar errores de compilación que el
  análisis estático no pille (cfgs, APIs de crates que hayan cambiado).
- Prueba de micrófono real (§1), transcripción real (§2), bind de Hyprland
  real (§4 HOTKEY-HYPR.md) y arranque real del server LLM (§5).
- Tray: backend pendiente (§3) — requiere libs GTK y host SNI; decisión
  de producto pendiente (¿vale la pena para Hyprland?).
- La tarea hermana «Parte 1» (inventario) no estaba pusheada a origin al
  hacer este trabajo (fetch origin no mostró feature/port-linux-inventario);
  el merge final lo coordina Port/qa-01.
