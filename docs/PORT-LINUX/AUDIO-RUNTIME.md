# Runtime de audio/transcripción/tray/LLM en Linux (port t_7ac526a3)

Estado de cada módulo (CON HONESTIDAD: nada de esto se ha compilado ni medido en
esta máquina — la tarea prohíbe `cargo build`; la prueba real de compilación,
micrófono y teclado la hace el usuario):

| Módulo      | Estado                        | Qué falta para "listo"                    |
|-------------|-------------------------------|-------------------------------------------|
| audio.rs    | a validar (build del usuario) | build + probar micrófono real             |
| transcribe.rs | a validar (build del usuario) | build + descarga de GGML + transcribir  |
| tray.rs     | pendiente backend de sistema  | decidir host de tray + instalar libs GTK  |
| hotkey.rs   | listo a validar (Hyprland)    | build + bind en hyprland.conf (ver HOTKEY-HYPR.md) |
| llm.rs      | listo a validar (build)       | build + probar arranque de llama-server   |
| main.rs / build.rs | listo (trivial)        | build                                     |

## 1. audio.rs — captura con cpal 0.15

- En Linux cpal 0.15 habla ALSA (los sistemas que soporta son ALSA/JACK; cpal
  0.15.3). En Omarchy el audio del sistema lo lleva PipeWire y la app llega a él
  por el conector `pipewire-alsa`: app → cpal/ALSA → pipewire-alsa → PipeWire. NO
  se activa la opción `pipewire` de cpal (obligaría a tener libpipewire-0.3-dev
  para compilar y no aporta nada para capturar el micro por los mismos canales).
- Buffer: la app usa la configuración por defecto del dispositivo
  (`default_input_config()`), es decir `BufferSize::Default`. Para GRABAR dictado
  (por turnos, no en directo) el tamaño del buffer de entrada no añade retraso que
  se note: se consume todo al parar el turno.
- «Fallo conocido de cpal 0.15 en Wayland con frecuencias de muestreo altas»: NO
  se ha encontrado tal fallo (Wayland no tiene nada que ver con el audio; los
  sistemas que usa son ALSA/PipeWire). Lo que SÍ existe y está documentado:
  - cpal issue #1029 (y de la familia #1033/#1036): con pipewire-alsa y
    `BufferSize::Default`, PIPEWIRE monta buffers enormes y la REPRODUCCIÓN (no
    la captura) sufre retrasos de segundos. No afecta a la captura del dictado;
    si alguien viera síntomas parecidos al capturar, el arreglo sería pedir
    `BufferSize::Fixed(1024)` al crear el stream (una línea en `build_stream`),
    decisión que depende de que aparezcan síntomas reales, NO aplicada por
    adelantado.
  - La lista de dispositivos por ALSA puede dar nombres técnicos («hw:0,0»,
    perfiles de pipewire-alsa) en vez de nombres bonitos como en Windows. La
    interfaz ya lista `input_devices()` tal cual; el selector funciona, solo
    cambia el formato de los nombres.
- Código: sin `#[cfg]` Linux/Windows: cpal funciona en todos los sistemas y la
  lógica que ya hay (alternativa H6 de dispositivo, normalize_gain, remuestreo a
  16k, tope de 2 h) es pura y ya tiene tests. Único cambio: apuntar al arrancar
  la frecuencia, canales, formato y sistema de audio para poder diagnosticar a la
  primera.
- Qué tiene que probar el usuario (lista):
  1. `cargo build --release` (o run).
  2. Ajustes → selector de micrófono: deben aparecer los perfiles ALSA.
  3. Grabar un turno corto y comprobar que el medidor de nivel se mueve y que
     llega la transcripción (ver §2).
  4. Si el micro dice «dispositivo ocupado»: probar `pw-cli` (¿está PipeWire
     vivo?) y cerrar las apps que retengan el micro (el navegador con la webcam).

## 2. transcribe.rs — whisper-rs 0.16, CUDA opcional

- La opción `cuda` de whisper-rs (docs.rs/lib.rs whisper-rs: "cuda: enable CUDA
  support. Implicitly enables hidden GPU flag at runtime") ya NO va fijada en el
  Cargo.toml: ahora es una opción del propio crate (ver `[features] cuda` en
  Cargo.toml), APAGADA por defecto:

      cargo build --release              # CPU-only (default Linux)
      cargo build --release --features cuda   # con CUDA (requiere nvcc)

  En Windows, lo de antes (cuda por defecto) se sustituye por la misma opción
  explícita. Sin código cfg en transcribe.rs: los mismos `FullParams` valen con y
  sin CUDA (whisper.cpp decide por dentro al crear el contexto).
- No hay código que dé por hechas rutas de GPU ni nada de cuda de usar y tirar en
  el módulo (búsqueda hecha: `cuda|GPU|gpu` solo aparece en comentarios de
  documentación). No había nada que envolver en cfg(feature="cuda") en el
  código — la opción vive en el Cargo.toml.
- Velocidad solo con CPU en el portátil (Ryzen 5 PRO 5650U, 6C/12T, 14 GiB):
  ESTIMACIÓN HONESTA, NO MEDIDA (la tarea lo prohíbe y no hay cifras inventadas).
  Referencias públicas para hacerse una idea:
  - whisper.cpp (README del repo ggerganov/whisper.cpp) publica benchmarks de CPU
    con small en x86_64 con AVX en el rango de ~2-4x el tiempo real (varía mucho
    según la CPU, los hilos y el beam size). Con n_threads=4..6 y beam=5 (los
    valores por defecto de la app), lo esperable en el Ryzen es que un turno
    hablado de 30 s tarde del orden de 10-30 s con small; tiny/base bajan mucho
    más.
  - La medición REAL en este portátil la hace el usuario (lo pide la tarea):
    transcribir un turno de duración conocida con `small` y mirar el factor de
    escala en el registro (whisper.cpp imprime «whisper_full: ... time = ...»).
  - Si small con CPU no diera la talla, tiny/base están en el selector de la
    interfaz sin cambiar código.
- Modelos: los mismos GGML de HuggingFace (repo ggerganov/whisper.cpp). La carpeta
  de destino pasa de `%APPDATA%\whispr-open\models` (Windows) a
  `~/.local/share/whispr-open/models` (Linux) vía `dirs::data_dir()` — la
  traducción XDG estándar, sin cfg en el código. El VAD (`ggml-vad-q8.bin`) vive
  en la misma carpeta y sigue siendo opcional.

## 3. tray.rs — BANDEJA PENDIENTE en Linux (decisión honesta)

- tray-icon 0.24 en Linux SOLO tiene la parte AppIndicator (docs.rs/tray-icon
  0.24.2: «Linux (gtk Only)»; dependencias en Linux: gtk3 +
  libappindicator/libayatana-appindicator + libxdo — en pacman:
  `gtk3 libappindicator-gtk3 libxdo`).
- Doble fragilidad: (1) compilar exige esas librerías GTK3; (2) al ejecutar hace
  falta un HOST de StatusNotifierItem (una barra que pinte iconos SNI). Hyprland
  no trae nada de eso de fábrica (haría falta una barra tipo waybar/eww con
  soporte appindicator).
- DECISIÓN del port: en Linux NO se compila la dependencia
  (`[target.'cfg(not(target_os = "linux"))'.dependencies] tray-icon` en
  Cargo.toml) y `src/tray.rs` ofrece un apaño bajo `#[cfg(target_os = "linux")]`
  que cumple el MISMO contrato que la bandeja real (constantes MENU_*,
  `SystemTray::new()` que devuelve Ok con un aviso en el registro, campos
  status_item/toggle_item con `set_text()` que no hacen nada y canal `events`
  siempre vacío). `app.rs` (fuera del reparto de este port) no necesita cfg.
- Efecto para quien use Linux: sin icono junto al reloj; todo el flujo vive en la
  ventana principal (dictado, ajustes) + atajo (§ hotkey) + botón de salir de la
  propia interfaz. La ventana se puede ocultar; para recuperarla, relanzar la app
  NO abre una segunda instancia cuando exista el guard de instancia única de Linux
  (single_instance.rs — módulo de OTRO agente, su apaño actual devuelve true
  siempre).
- Volver a activar la bandeja en el futuro (lista):
  1. `sudo pacman -S gtk3 libappindicator-gtk3 libxdo`
  2. Quitar el cfg de `tray-icon` en Cargo.toml (mover la dependencia al bloque
     general).
  3. Quitar el `#[cfg]` del bloque real de `src/tray.rs` (el apaño y el bloque
     real conviven documentados; el real reutiliza `circle_rgba`).
  4. Confirmar que el host de bandeja (waybar + SNI) está corriendo.

## 4. hotkey.rs — backend "hyprland" (socket UNIX)

Está explicado en su documento: docs/PORT-LINUX/HOTKEY-HYPR.md (fragmento exacto
de hyprland.conf, lista de pruebas, permisos). Resumen:
- Windows: `global-hotkey` igual que siempre (cfg not(linux)).
- Linux/Wayland: la app escucha en `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock` y un
  bind de Hyprland le escribe `toggle`:

      bind = $mainMod SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

- El evento viaja por el mismo tipo de canal que en Windows
  (`GlobalHotKeyEvent` con campos públicos id/state — comprobado en
  docs.rs/global-hotkey 0.6): app.rs/ui/poll.rs no distinguen un sistema de otro.
- La dependencia `global-hotkey` NO se limita a Windows: app.rs y
  ui/poll.rs (ficheros de otros agentes) importan `GlobalHotKeyEvent`
  sin cfg; la parte Linux del crate se carga en caliente (x11-dl) y compila sin
  tener libX11. En una sesión X11 (GNOME/XFCE) el registro de X11
  funcionaría sin tocar nada (HotkeyManager::register usa el mismo
  camino del crate).

## 5. llm.rs — server LLM en Linux

- SERVER_EXE_NAME pasa a cfg: `llama-server.exe` (Windows) /
  `llama-server` (Linux). is_server_exe/sanitize_server_exe usan la
  constante (tests duplicados por sistema).
- launch_server: implementación real para Linux (antes devolvía «solo
  Windows»): Command::new(exe) con cwd = carpeta del exe, stdout/stderr →
  `llama-server.log` junto a la configuración (la misma mecánica que en Windows,
  sin CREATE_NO_WINDOW), proceso apuntado en LAUNCHED_SERVER_PID.
- terminate_server_pid en Linux: comprueba de qué programa es el proceso leyendo
  `/proc/<pid>/exe` (solo se mata si de verdad es `llama-server`), manda SIGTERM y
  si sigue vivo 2 s → SIGKILL. Dependencia nueva `libc = "0.2"` SOLO en
  `[target.'cfg(unix)'.dependencies]` (kill(2); en Windows no compila).
- Búsqueda automática de rutas (default_server_roots) según el sistema:
  - Windows (sin cambios): D:\LlamaCPP, C:\LlamaCPP, junto al exe,
    %LOCALAPPDATA%\LlamaCPP, PATH.
  - Linux: junto al exe de la app → `$HOME/.local/bin/llama-server` →
    PATH → `$XDG_CONFIG_HOME/whispr-open/llama-server` (por defecto
    `~/.config/whispr-open/`).
- Modelos GGUF (default_model_roots): `~/.lmstudio/models` (igual que en
  Windows, relativo a $HOME), `~/.lmstudio/models/lmstudio-community`,
  NUEVO `~/.config/whispr-open/models` (Linux), y las raíces de unidad
  C:/D: solo Windows (en Linux no molestan: no existen).
- Opciones del servidor (`build_server_args`): IGUALES en los dos sistemas
  (`-ngl 99 --flash-attn on -ctk q8_0 -ctv q8_0 -c 8192`). En una compilación de
  llama.cpp solo para CPU esas opciones de GPU se ignoran con un aviso del
  servidor; con `--features cuda` son las que encienden la GPU. Test
  duplicado por sistema.
- Qué tiene que probar el usuario:
  1. Instalar o colocar `llama-server` (compilación de llama.cpp) en una de las
     rutas que se buscan sola o poner la ruta en Ajustes.
  2. Arrancar la app y mirar el registro: «server LLM lanzado (pid N)» +
     «/v1/models respondiendo».
  3. Probar una corrección hablada («digo X») y el cierre limpio de la
     app (el servidor propio debe morir: comprobar con `pgrep llama-server`).

## 6. main.rs y build.rs

- main.rs: `#![windows_subsystem = "windows"]` pasa a
  `#![cfg_attr(windows, windows_subsystem = "windows")]` (el atributo a secas
  estropea la utilidad del binario en Linux). El resto del arranque ya
  funcionaba en cualquier sistema: single_instance tiene apaños para no-Windows
  (módulo de otro agente) y el run de iced_winit no toca nada de Windows.
- build.rs: ya tenía el bloque `#[cfg(not(windows))]` (no hace nada + rerun).
  El camino de Windows (winresource + regenerar el .ico) solo compila en Windows:
  `winresource` está en `[build-dependencies]` global, PERO su uso está dentro de
  `#[cfg(windows)] fn main()`; en Linux el `main()` con cfg ni lo menciona, y
  cargo no enlaza winresource fuera del script de compilación (no se rompe). Se
  deja tal cual: es lo normal.

## 7. Cambios en Cargo.toml (resumen para review)

- `whisper-rs`: `features = ["cuda"]` → `default-features = false` +
  `[features] cuda = ["whisper-rs/cuda"]` (APAGADA por defecto, en los dos
  sistemas).
- `tray-icon` movido a `[target.'cfg(not(target_os = "linux"))'.dependencies]`
  (Linux sin bandeja: ver §3).
- `winapi` movido a `[target.'cfg(target_os = "windows")'.dependencies]`
  (no compila fuera de Windows).
- `global-hotkey` y `crossbeam-channel`: SIN cambios de sección (ver §4:
  app.rs los usa sin cfg; x11-dl se carga en caliente).
- NUEVO `[target.'cfg(unix)'.dependencies] libc = "0.2"` (cerrar el servidor de
  IA; ver §5).
- Nota de comprobación: en whisper-rs 0.16 la opción `cuda` EXISTE y el
  `default` ya va VACÍO (Cargo.toml.orig de 0.16.0: `default = []`), así que
  `default-features = false` en la práctica no cambia nada y se mantiene por
  dejarlo claro (que el port es solo CPU salvo `--features cuda`). Corregido en
  la revisión del run 6.

## 8. Correcciones del run 6 (revisión del run anterior)

- src/tray.rs (apaño de Linux): `crossbeam_channel::never().1` → `never()`
  (la función devuelve un `Receiver<T>` directamente, no una pareja — comprobado
  en docs.rs/crossbeam-channel; el `.1` NO compila).
- Las llaves y paréntesis de los ficheros del reparto comprobados con un lector
  básico (sin compilar, la tarea lo prohíbe): cuadran todos en 0.
- Comprobado en las fuentes del crate (docs.rs global-hotkey 0.6.4 lib.rs) que
  `GlobalHotKeyEvent { pub id: u32, pub state: HotKeyState }` se puede construir
  en el listener de Linux (sin `#[non_exhaustive]`): ok.

## 9. Pendientes explícitos (honestidad de la task)

- Compilar el binario de verdad en Linux (al agente le está prohibido): el usuario
  debe lanzar `cargo build --release` y contar los errores de compilación que el
  análisis leyendo el código no pilla (cfg, APIs de crates que hayan cambiado).
- Probar el micrófono de verdad (§1), transcribir de verdad (§2), el bind de
  Hyprland de verdad (§4 HOTKEY-HYPR.md) y arrancar el servidor de IA de verdad
  (§5).
- Bandeja: parte pendiente (§3) — hacen falta las librerías GTK y un host SNI;
  decisión de producto pendiente (¿vale la pena para Hyprland?).
- La tarea hermana «Parte 1» (inventario) no estaba subida a origin al hacer este
  trabajo (fetch origin no mostró feature/port-linux-inventario); la fusión final
  la coordina Port/qa-01.
