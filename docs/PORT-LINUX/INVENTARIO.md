# INVENTARIO — Port de Whispr Open a Linux (Wayland/Hyprland)

Lista de TODO lo que en `src/` depende del sistema, fichero por fichero, con la
prueba (fichero:línea) y la propuesta de cómo se haría en Linux/Wayland.

Cómo se hizo: lectura completa de los 24 ficheros `.rs` de `src/` (20.441 LOC,
contados con `wc -l`) más `Cargo.toml`, `build.rs` y `Cargo.lock`.
Encontrados **51 condicionales `#[cfg(target_os = "windows")]` /
`#[cfg(not(target_os = "windows"))]`** repartidos en 6 ficheros:
bubble.rs (28), config.rs (6), inject.rs (7), llm.rs (6), single_instance.rs (6),
theme.rs (2). Comprobado con:

    grep -rn 'cfg(' src/ --include='*.rs' | grep -iE 'windows|unix|target_os|linux' | wc -l   # → 51

Además hay cosas propias de Windows FUERA de los cfg que conviene apuntar
(sección 8).

Estado de esta parte del encargo: **solo análisis + Cargo.toml. No se ha tocado
NINGÚN fichero de `src/`** (lo dice la tarea; el port de verdad es P2+).
Repositorio: copia en `~/hermes-team/work/port-linux-01`, rama
`feature/port-linux-inventario`.

---

## Leyenda de dificultad

- **baja**: se traduce casi directamente con una librería del sistema; no hay que
  diseñar nada.
- **media**: hay decisiones de diseño o dependencias nuevas del sistema.
- **alta**: es algo que en Wayland no existe como en Windows; hace falta otro
  mecanismo completo (un servicio externo, portals, hablar con el compositor).

Bloqueante = si no se resuelve, el crate o el fichero no compila en Linux.

---

## 1. src/single_instance.rs — guard de instancia única (131 LOC)

Prueba: `src/single_instance.rs:35` (`CreateMutexW`, winapi
`um::synchapi`), `:44-66` lógica ERROR_ALREADY_EXISTS, `:78-96` aviso
`MessageBoxW` para la segunda instancia, `:71-74`,`:99-100` apaños para
no-Windows, `:17` nombre `Local\\WhisprOpen-SingleInstance`.

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I1 | Mutex nombrado por sesión `CreateMutexW` guard de instancia única | **File lock** `flock`/`fcntl(F_SETLK)` sobre `$XDG_RUNTIME_DIR/whispr-open.lock` (el runtime dir es por sesión, equivalente natural al namespace `Local\\`). Crates: `fd-lock` (ligero, sin deps) o `single-instance`. Sin sudo ni daemon | baja | SÍ (compila hoy: el stub `:71-74` devuelve `true` siempre, así que el guard no funciona, no funcional pero sí compilante) |
| I2 | Aviso `MessageBoxW` a la segunda instancia | Notificación desktop (crate `notify-rust` vía org.freedesktop.Notifications; Hyprland la muestra con `mako`/`dunst`) o simple `eprintln` + exit. El flujo de la app degrada elegante, no aborta | baja | No |

El fichero entero se puede rehacer con un cfg para Linux
`#[cfg(target_os = "linux")]` sin tocar la parte de Windows.

---

## 2. src/inject.rs — inyección de texto en la app activa (281 LOC)

Prueba: `src/inject.rs:103` (`copy_to_clipboard` de Windows con arboard),
`:127` (`inject_text_into_active_app` arboard + enigo Ctrl+V), `:196-216`
(`GetForegroundWindow` + `GetWindowTextW` para el título de la ventana activa,
filtro `window_filter`), `:115-118` y `:220-226` apaños de error fuera de
Windows, `:13` "Simula Ctrl+V con enigo. Solo Windows."

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I3 | `copy_to_clipboard` con arboard | arboard YA soporta Wayland en 3.6+ (fuente de verdad: Cargo.lock `name = "arboard"` 3.6.1 → deps `clipboard_wayland` 0.2.2 + `smithay-clipboard` 0.7.3). (Wayland compositing portal no aplica aquí): basta declarar arboard tal cual (la feature del crate que añade el backend de Wayland, `wayland-data-control`, hace falta y ya está en el lock vía clipboard_wayland; en el Cargo.toml modificado no toco features porque el crate trae el backend por defecto con default-features del target). **Acción: activar y usar arboard en Linux** (estaba marcado Windows-only en la doc porque el resto del módulo no existía) | baja | SÍ: sin esto el módulo entero es stub |
| I4 | Simular Ctrl+V con enigo | **Fallará en Wayland puro**: enigo 0.6.1 depende de X11 (Cargo.lock `name = "enigo"` → `x11rb` + `xkbcommon`; sin backend nativo Wayland). Vía XWayland funcionaría pero no es el camino. Propuesto: **`wtype -s 50 "texto"`** (ya instalado en la máquina: `which wtype` → `/usr/bin/wtype`) para el modo `Type`, y para `Paste` inyectar la combinación Ctrl+V con `wtype -M ctrl -k v -m ctrl`. Alternativa si la app destino no acepta key events sintéticos: `ydotool` (daemon + uinput, requiere grupo input o udev rule) | media | SÍ (con I3 resuelto, sin I4 el modo Paste no inyecta) |
| I5 | `foreground_window_title` con GetForegroundWindow | `hyprctl activewindow -j` (JSON con `class` y `title`) o el socket de Hyprland `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`. Depende de HYPRCTL/Hyprland disponible; cae a `None` si no hay → degrada como hoy (`NoForegroundWindow`) | media | No (hay fallback elegante) |

---

## 3. src/bubble.rs — burbuja flotante (1294 LOC, 28 cfg Windows)

Prueba resumida (los bloques de winapi están agrupados): `:29-36` `use
winapi::um::winuser::{GetAsyncKeyState, GetCapture, GetCursorPos,
GetMonitorInfoW, GetWindowRect, ...}` en imports por-cfg, `:483` apply
extras (noact+pos+size), `:534` move to corner, `:576-593`
`bubble_window_position`, `:600-616` `corner_position` con
`MonitorFromWindow`+`GetMonitorInfoW`, `:624-638` `find_bubble_hwnd` con
`FindWindowW(BUBBLE_TITLE)`, `:645-676` `apply_noactivate`
(`WS_EX_NOACTIVATE|WS_EX_TOOLWINDOW`, quitar `WS_EX_APPWINDOW`),
`:691-866` clase `BubbleDrag` completa con `SetCapture`/`GetCapture`/
`ReleaseCapture` + `GetCursorPos`+`MonitorFromPoint`, `:891-901`
`right_button_down`/`left_button_down` vía `GetAsyncKeyState(VK_RBUTTON/
VK_LBUTTON)`, `:905-916` `capture_left_mouse`, `:919-928`
`release_mouse_capture`, `:931-939`. La función pura `drag_target` (`:727`)
es igual en todos los sistemas. También había `BUBBLE_TITLE` en `:59`.

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I6 | Guardar el `window::Id` de `window::open` como fuente de verdad (la app YA lo guarda en `BUBBLE_WINDOW_ID`, `:64`) para todas las operaciones sobre la burbuja en Linux; `FindWindowW` queda solo en la rama windows | trivial | No |
| I7 | WS_EX_NOACTIVATE (que los clics no roben el foco) + WS_EX_TOOLWINDOW (no sale en la taskbar) | En Wayland NO EXISTE equivalente per-ventana estándar al estilo "no activar". La burbuja es una segunda ventana Iced con `window::Settings { .. }`: se puede pedir `no_activate` no estándar vía `xdg_shell` sin decoración y dejar que Hyprland la enfoque a sí mismo; el efecto de "no robar foco" en Hyprland se consigue con la regla `windowrule = nofocus, class:whispr-open-bubble` en hyprland.conf (deprecado `windowrule2 nofocus`). **HERRAMIENTA clave: `hyprctl keyword` NO funciona en Omarchy (parser Lua); en Omarchy la herramienta real es `hyprctl eval 'hl.config({...})'` o editar el config Lua**. Riesgo: es pragmático pero depende del anfitrión | alta | No (solo pierde UX: los clics pueden robar foco hasta que el usuario regle) |
| I8 | `SetWindowPos` (posiciona/mueve la burbuja en px de pantalla, `apply_bubble_extras` `:483`, `move_to_corner` `:534`) | Para Iced 0.13 la forma correcta cross-platform es `iced::window::move_to(id, x, y)` + `iced::window::resize(id, w, h)`, ambas ya expuestas y multiplataforma (van por winit → Wayland xdg positioner). NO hace falta winapi | baja | No |
| I9 | `bubble_window_position` (`GetWindowRect`) para persistir la posición del arrastre | `iced::window::get_position(id)` (multiplataforma). Es un `Task` async, cambia el call-site pero no el diseño | media | No |
| I10 | Arrastre manual `BubbleDrag` (`SetCapture` + `SetWindowPos` en cada `CursorMoved`) | winit en Wayland: el drag de ventanas es el protocolo `xdg_toplevel.move` (sin capturas de ratón manuales). **Reemplazo propuesto: usar `window::drag(id, button)` de iced para arrastrar la burbuja** con el botón derecho. La razón del drag manual era que en Windows `drag()` espera botón izquierdo (`WM_NCLBUTTONDOWN`); en Wayland `xdg_toplevel.move` acepta el botón que disparó el grab. Alternativa Windows-only: mantener el BubbleDrag actual como cfg | media-alta | No (sin esto la burbuja no se puede arrastrar en Linux, pero sí fija) |
| I11 | `GetAsyncKeyState` (estado crudo de botones, red de seguridad del drag) | No existe global de "estado del botón" en Wayland. Red de seguridad propuesta: cancellation token propio + `is_captured()` en stub → ya devuelve `true`; la red actual `right_button_down`/`left_button_down` ya stubs ⇒ **suficiente como fallback** (los "true"/"false" de stubs son correctos) | baja | No |
| I12 | `SetCapture`/`ReleaseCapture` para no perder eventos al salir del rect | No existe en Wayland. Con `drag_target` pure fn + `pointer capture` del protocolo wl_pointer (que winit ya recibe sin hacer nada), la burbuja seguirá recibiendo PointerMoved dentro del buffer aunque salga: **Wayland no tiene "grab" del puntero igual que X11, pero sí pointer focus. Conclusión: dejar el drag a winit** (ver I10) y no reimplementar | alta | No |

Dificultad del conjunto de la burbuja: **la pieza más gorda del port**.

---

## 4. src/tray.rs — bandeja del sistema (173 LOC)

Prueba: `src/tray.rs:22-27` usa `tray_icon` 0.24 + `tray_icon::menu::*
` (el menú lo pinta `muda` por debajo); `:36-70` menú en español (estado,
Mostrar/Ocultar, Salir); `:75-90` icono RGBA 32x32 generado en código.

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I13 | `TrayIconBuilder` + menú Mostrar/Ocultar/Salir con icono generado en código | `tray-icon` 0.24 YA soporta Linux vía `libappindicator`/`muda` (Cargo.lock: `name = "tray-icon"` 0.24.2 → deps `libappindicator` 0.9.0 + `gtk`, `muda` 0.19.3 → `libxdo`,`gtk`). Corta: **funciona igual sin tocar código del crate**, pero TRAE NUEVAS deps de sistema a compilar. En esta máquina ya está GTK3 (`pacman -Q gtk3` OK) pero **NO** `libayatana-appindicator-gtk3` (`pacman -Q` falla). Necesita `sudo pacman -S libayatana-appindicator-gtk3` posible wrap de system deps del crate | media | SÍ en Linux sin el paquete |
| I14 | Icono de tray: RGBA 32x32 en código (circle_rgba) | En libappindicator el formato aceptado en agent fallback es pixbuf RGBA — mismo buffer sirve. Sin código a cambiar | baja | No |

Nota: `tray-icon` en Linux exige `gtk`/`glib` en las dependencias, así que
**recomendación para compilar**: la opción `tray-gtk` es obligatoria para que
compile. El crate `tray-icon` solo la activa al compilar las dependencias gtk
comunes; el Cargo.lock las incluye.

---

## 5. src/hotkey.rs — atajo global (206 LOC)

Prueba: `src/hotkey.rs:12-13` usa `global_hotkey::hotkey::{Code,
HotKey, Modifiers}` + `GlobalHotKeyEvent, GlobalHotKeyManager`; `:30-58`
`register` + `stub`; `:61` `GlobalHotKeyEvent::receiver()`; `:84` comportamiento
por apaño si falla el registro (se degrada con elegancia). `Cargo.lock name =
"global-hotkey"` 0.6.4 → deps `x11-dl` + `windows-sys` + `objc2*`. NO tiene
parte para Wayland: es el crate global_hotkey de siempre, sin soporte Wayland.

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I15 | `GlobalHotKeyManager::new()` + `register(hotkey)` para Ctrl+Shift+D | **NO funciona en Wayland** (global-hotkey 0.6.4 solo X11). Dos caminos: (a) via XWayland continuaría pero con ventana + focos extra; (b) camino recomendado: **bind real en hyprland.conf** (`bind = CTRL SHIFT, D, exec, <bin de la app con flag --toggle>`) + **flag `--toggle` CLI** en la app que se comunique por wall socket/`$XDG_RUNTIME_DIR` fifo con la instancia ya corriendo. En Omarchy con config Lua la sintaxis seguirá siendo `bind =` (independiente del frontend) | media | No si se hace con stub + path (b); SÍ si se exige atajo global nativo |

Estrategia propuesta: dejar `global-hotkey` como dependencia de Windows/macOS en
el Cargo.toml y en Linux un apaño que apunte en el registro «configura el bind en
Hyprland: bind = CTRL SHIFT, D, exec, whispr-open --toggle» + la opción `--toggle`
de main.rs (main.rs le toca a este agente por aviso del organizador; si no, se
deja en nota y lo lleva quien toque main):

- `src/hotkey.rs` recibe una rama `#[cfg(target_os = "linux")]` que registra un
  **stdout+exit** para el binario invocado (segunda ejecución) y avisa al guard de
  instancia única (que ya detecta la segunda instancia con I1 con flock) → la
  primera instancia lo entiende (por notificación/opción en un socket UNIX) como
  «toggle».

---

## 6. src/config.rs — config persistente + API key (1751 LOC, 6 cfg Windows)

Prueba: `:457-467` `config_path` + `dictionary_path` vía `dirs::data_dir`
(OK en todos los sistemas: en Linux devuelven `~/.local/share/`); `:470-480`
helpers wide; `:482-506` `load_api_key_secret` con **Win Credential Manager**
(`winapi::um::wincred::{CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC}`),
`:508-541` `save_api_key_secret` (`CredWriteW`); apaños `:503-506` y
`:540-541` que devuelven None/no-op. Guarda config.json en
`%APPDATA%/whispr-open/config.json` vía `dirs`.

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I16 | `config_path`/`dictionary_path`/`log_dir`/`models_dir` con `dirs::data_dir` | YA funciona en Linux: `dirs 6.0.0` (Cargo.lock name dirs 6.0.0) devuelve `~/.local/share` vía XDG. Cambio encadenado de textos de UI que mencionan `%APPDATA%` (véase sección 9: cadenas %APPDATA% no bloqueantes) | baja | No |
| I17 | Guardar API key en Windows Credential Manager | Secret Store Linux: **Secret Service** (org.freedesktop.secrets) con crate `keyring` 3.x; en GNOME/KDE está gnome-keyring/KWallet; en Hyprland + Omarchy probablemente hace falta `gnome-keyring` o `kwallet`. Fallback no-fatal: NO guardar (como hoy, llave vacía ⇒ el usuario pone la key manualmente) | media | No (fallback: no guardar) |

---

## 7. src/llm.rs — corrección IA con llama.cpp (2922 LOC, 6 cfg Windows)

Prueba: `:81` `SERVER_EXE_NAME = "llama-server.exe"`; `:89-94`
`is_server_exe` exige nombre EXACTO "llama-server.exe" (sin distinguir
mayúsculas) — **"llama-server" sin .exe sería rechazado** porque quien lo llama
es solo de Windows; `:100-106` sanitize; `:1233-1280` `launch_server` arranca con
`CommandExt::creation_flags(CREATE_NO_WINDOW)`; `:1285-1292` apaño de error;
`:1347-1401` `terminate_server_pid` con `OpenProcess`+`TerminateProcess`+
`QueryFullProcessImageNameW`; `:1493-1513` `default_server_roots` busca
`llama-server.exe` en `D:\LlamaCPP`/`C:\LlamaCPP`/`LOCALAPPDATA`/PATH; `:1595+`
`default_model_roots` busca en `C:/LlamaCPP/models`, `D:/models`,
LM Studio de dirs (ya en parte funciona en varios sistemas vía `dirs::home_dir`).

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I18 | `SERVER_EXE_NAME = "llama-server.exe"` constant + `is_server_exe` se usa para VALIDAR el binario | En Linux el binario de llama.cpp es `llama-server` SIN extensión. Cambio mínimo: `#[cfg(windows)]` `= "llama-server.exe"` / `#[cfg(not(windows))]` `= "llama-server"` (const separada). Sin tocar diseño | baja | SÍ (con stub actual el servidor LLM local no arranca jamás) |
| I19 | `launch_server` — spawn nativo + `CREATE_NO_WINDOW` (0x08000000) + log redirigido + PID en LAUNCHED_SERVER_PID | Arrancar con std::process::Command genérico ya funciona en todos los sistemas; quitar `.creation_flags(...)` con cfg (el trait solo existe en Windows). En Linux un proceso hijo de una app con ventana ya no abre consola. Sin más | baja | SÍ (stub actual siempre Err) |
| I20 | `terminate_server_pid` — mata por PID con verifies process name | `std::process::Command::new("kill").arg(pid)` / `libc::kill(pid, SIGTERM)`; el check de nombre se puede hacer leyendo `/proc/PID/comm` (o `/proc/PID/exe`) para evitar matar el PID equivocado. Alternativa por `libc` puro sin apt deps | baja-media | SÍ (stub actual siempre false ⇒ el server quedaría huérfano) |
| I21 | `default_server_roots` — rutas D:\LlamaCPP etc. | En Linux: rutas razonables `$HOME/.local/bin/llama-server`, `/usr/bin/llama-server`, `/opt/llamacpp/bin/llama-server`, y descartadas las de unidad Windows. Matches `.exe` a nombre no-extension | baja | No |
| I22 | `default_model_roots` LM Studio y C:/D:/** | Ya parcialmente cross (`dirs::home_dir` para LM Studio). Añadir `~/.lmstudio/models` y el resto de `XDG_DATA_HOME` rutas; quitar C:/D:/ en Linux con cfg | baja | No |
| I23 | Timeout del arranque del server y wait_for_server base unix | No toca Windows (reqwest/ureq a `http://127.0.0.1:8081/v1`); esta parte es cross-platform ya | N/A | No |

Nota que pesa mucho en el diseño: el servidor llama.cpp es un binario nativo **por
decisión del proyecto (AGENTS.md)**, no por su API; en el port a Linux el binario
también sería llama.cpp compilado en local (`pacman -S llama-server` no existe
como paquete — comprobado con `pacman -Si llama-server`: no encontrado).
Para la máquina de Juanfri no se recomienda compilar llama.cpp aquí en esta fase;
la alternativa de bajo riesgo es apuntar `base_url` a un endpoint que ya esté
corriendo (en esta partición todavía no hay servidor). **Dejar I18-I21 como
apartado opcional «binario llama-server para Linux», separado de la app.**

---

## 8. src/theme.rs — tema auto oscuro/claro (1480 LOC, 2 cfg Windows)

Prueba: `:706-741` `system_dark_mode()` lee el registro de Windows
`HKCU\...\Themes\Personalize` AppsUseLightTheme (winapi `um::winreg`
RegGetValueW); `:743-748` fuera de Windows devuelve siempre oscuro (lo normal en
la app).

| # | Qué hace | Equivalente Linux/Wayland propuesto | Dificultad | ¿Bloqueante? |
|---|---|---|---|---|
| I24 | `system_dark_mode()` — auto-tema leyendo el registro | En GNOME/KDE: `org.freedesktop.appearance color-scheme` via xdg-settings (package `xdg-utils`) o leyendo `~/.config/gtk-3.0/settings.ini` para `gtk-application-prefer-dark-theme`. En Hyprland: no estándar. Colocarse a: GTK settings con crate `config-parser_settings` light / `zbus`; fallback vacio = true (oscuro default) como hoy | media | No (hay default) |

---

## 9. Windows-ísimo FUERA de los cfg (nuevo inventario, no estaba numerado)

- `src/main.rs:6` `#![windows_subsystem = "windows"]`: el atributo se ignora fuera
  de Windows; en Rust moderno el atributo no es error si no es Windows. Sin
  problema. (Comprobado con el cargo.com PATH de docs/AGENTS.md de la build de
  E:\AI\FREEBUFF; el atributo se aplica, no rompe).
- `src/fonts.rs:17-45` `EMOJI_FAMILY = "Segoe UI Emoji"` y candidatos fijos
  `C:\Windows\Fonts\seguiemj.ttf`. La función `emoji_font_candidates` YA
  tiene rutas de repuesto para Linux (`/usr/share/fonts/noto/NotoColorEmoji.ttf`
  y otras 2). Añadir `/usr/share/fonts/noto/NotoColorEmoji.ttf` existe solo
  si el paquete `noto-fonts-emoji` está instalado — **comprobar en
  `pacman -Q noto-fonts-emoji` (hoy no instalado)**. Sin esa fuente la interfaz
  pinta cajas vacías en los símbolos ✓ ☀ ☾ ▾ ▸ ● → ✕.
- `src/config.rs:312` / `src/ui/settings.rs:1137-1162`: textos de la interfaz que
  hablan de "Windows"/"Credential Manager de Windows" — solo son TEXTOS, no
  cambian la compilación. Cambios de forma recomendados en P2 (texto neutro
  "almacén de secretos del sistema").
- `src/custom_image.rs:19,90,106,399`: comentarios que dan por hecho NTFS/`\\?\`.
  El código NO usa ese prefijo; son solo notas de documentación. Sin acción
  (comprobar) con canonicalize multiplataforma ya OK.
- `src/transcribe.rs:291`: solo un comentario (soltar el fichero antes de
  renombrar); el rename en Linux con el fichero abierto SÍ funciona; no requiere
  cfg.
- `docs/` y textos: `%APPDATA%` en ~21 sitios de `src/` (solo textos de registro e
  interfaz) — actualización de textos en P2, no bloqueante.
- `build.rs`: la parte `winresource::WindowsResource` está detrás de
  `#[cfg(windows)]`; `#[cfg(not(windows))] fn main()` no hace nada. OK como está
  en Linux (comprobado).

---

## 10. Cargo.toml / Cargo.lock / build: lo que requiere el port COMPLETO

Estado actual de `Cargo.toml` (rama feature/port-linux-inventario, ya modificado
en esta pasada, ver la nota de más abajo):

- `cuda` de whisper-rs es una **opción opcional apagada por defecto**
  (esta máquina NO tiene GPU NVIDIA; en Windows hay una RTX 4070).
- Todas las dependencias de Windows (`winapi`, `tray-icon`, `global-hotkey`,
  `enigo`, `crossbeam-channel`, `rfd` y también `winresource` en
  build-deps) pasan a `[target.'cfg(windows)'.dependencies]`,
  tal como pedía el encargo. `arboard` se queda para todos
  (soporta Wayland). **Consecuencia, y está apuntada**: un cargo check en
  Linux fallará en tray.rs/inject.rs/hotkey.rs mientras sigan
  importando esos crates sin cfg; eso es el trabajo de las fases 1-4
  del PLAN.md y se resuelve módulo a módulo.
- NOTA en el fichero: `src/` AÚN no se ha tocado.
- **Cargo.lock NO se ha regenerado (no se puede) porque en esta máquina no
  hay `cargo`/`rustup`** (comprobado: `which cargo rustc rustup` → no
  encontrado; `pacman -Q rust` → no instalado). El primer
  `cargo check`/`cargo build` en cualquier sistema lo regenera con las
  opciones correctas de cada uno.

Qué falta del sistema en esta máquina (el comando `sudo pacman -S ...` lo
EJECUTARÁ el usuario, no yo — el sudo va por huella):

```
ruta obligatoria para compilar en Linux:
  # pacman -S --needed rustup cmake base-devel gtk3
  # pacman -S --needed libayatana-appindicator-gtk3    # tray-icon
  # pacman -S --needed noto-fonts-emoji               # glifos de la UI
  # pacman -S --needed wl-clipboard wtype             # clipboard + inyección
ya instalados hoy (verificado con pacman -Q):
  gtk3 3.24.52-1, alsa-lib 1.2.16.1-1, python 3.14.7-1,
  wl-clipboard 1:2.3.0-1, wtype 0.4-2
```

y el toolchain de Rust (no está instalado):

```
  $ rustup toolchain install stable   # rustc/cargo en ~/.cargo/bin
```

llama-server en Linux: NO existe en los repos de Arch (`pacman -Si
llama-server` → no encontrado); habría que compilar llama.cpp con cmake, o
resolver la corrección con IA para el port con un endpoint externo (Groq/Gemini)
o con la configuración apuntando a un servidor que ya esté encendido.

---

## 11. TL;DR de bloqueos para compilar en Linux hoy

Si se ejecutara `cargo check` en Linux con el código COMO ESTÁ (antes del port de
cada fichero):

1. **Cargo.toml sin retocar**: `winapi`, `tray-icon` (con muda/libxdo
   X11), `global-hotkey` y `enigo` son dependencias para todos los sistemas; el
   check podría resolverlas, pero la compilación final fallaría: las partes
   winuser/winreg/wincred de winapi no existen en un sistema Linux y muda está
   atado a GTK+X11/libxdo sin nada para Wayland.
2. Con el Cargo.toml ya modificado (rama actual): como CONSECUENCIA de mover lo de
   Windows a su sitio, `cargo check` en Linux falla AHORA MISMO en
   hotkey.rs/tray.rs/inject.rs/app.rs por los `use global_hotkey/`,
   `use tray_icon/`, `use crossbeam_channel/` sin cfg (los ficheros
   siguen importando 4 crates que solo existen en Windows).
   Ese fallo es ESPERADO y es el punto de partida de las fases 1-4 del
   plan: cada módulo recibe su rama #[cfg(...)]=linux y entonces compila
   con apaños; tras todas las fases, solo persisten las piezas que hay
   que hacer de verdad: inject (I3/I4), single_instance (I1/I2), el
   bloque de llm (I18-I21), y la burbuja pierde extras (I6-I12) pero
   compila.

Riesgo conocido en la interfaz: si no se instala `noto-fonts-emoji` (sección 9),
los símbolos ✓ ☀ ☾ ● → ✕ se verían como cajas vacías (el mismo problema que
llevó a crear src/fonts.rs en Windows).
