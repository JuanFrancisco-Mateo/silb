# CODIGO-CORE.md — Port whispr-open a Linux (Parte 2: código platform-specific)

Rama: `feature/port-linux-codigo-core` · Autor: dev-02 · Fecha: 03/10/2026

Qué es: registro de los cambios platform-specific de los 5 ficheros de este
reparto (inject, single_instance, theme, config, bubble) para que la app
compile en Windows Y en Linux (Omarchy: Arch + Hyprland sobre Wayland),
con degradación elegante y sin tocar los ficheros de otros devs.

Ley aplicada: AGENTS.md + docs/ARQUITECTURA.md + docs/ROADMAP.md. Contra la
regla «sin builds»: NO se compiló nada (no hay toolchain Rust en esta
máquina; el análisis es de lectura/razonamiento). El build real lo hará
Port/qa-01 con los pendientes de aquí abajo.

---

## 1. Qué cambió por fichero

### src/inject.rs (núcleo del port)
- Windows intacto tras `#[cfg(target_os = "windows")]`: arboard + enigo,
  guard de foco, filtro de ventana, Ctrl+V con soltar garantizado.
- Linux (`#[cfg(unix)]`), backend elegible por config (`inject_backend`):
  - `wl-clipboard` (default): `wl-copy` (texto por stdin) + `wtype` para la
    pulsación del pegado (byte `\x16` por stdin; A VALIDAR en binario).
  - `ydotool`: `ydotool type -- <texto>` (uinput, universal — Xwayland
    incluido). Sin `ydotoold` corriendo NO teclea: el turno queda en el
    portapapeles y se avisa (gracia degradada, nunca texto perdido).
  - `wtype`: tecleo directo por teclado virtual; PERREO con apps que
    bloquean zwp_virtual_keyboard_v1 → mensaje claro + portapapeles.
- Detección de la ventana con foco: `hyprctl activewindow -j` (title, y
  class como fallback para tiles sin título). Parser JSON mínimo propio
  (`json_string_field`, std, testeado: muestra REAL de hyprctl en tests).
- Flujo por plataforma (común): guard de foco → filtro de ventana →
  retardo → backend. El camino Windows mantiene PURO su comportamiento
  original (ni un solo token winapi fuera de cfg).
- `copy_to_clipboard`: wl-copy por stdin (no CLI args, sin fuga por ps).

### src/single_instance.rs
- Windows: intacto (CreateMutexW, namespace Local\, MessageBoxW aviso).
- Linux: lock file con flock (std `File::try_lock`, estable desde Rust
  1.88) en `$XDG_RUNTIME_DIR/whispr-open/instance.lock` (fallback a
  $TMPDIR). El candado es del kernel: un crash-kill no lo deja pegado.
  Se guarda el pid a modo de pista de diagnóstico (no es la fuente de
  verdad). Aviso al usuario de la segunda instancia vía `notify-send`
  (best-effort, sin daemon → solo log). Regla heredada: si el lock no se
  puede crear (permisos, FS ro), se continúa SIN guard (= nunca rompe el
  arranque), como en Windows.

### src/theme.rs
- Windows (AppControllerTheme): el registro AppsUseLightTheme intacto.
- Linux: `gsettings get org.gnome.desktop.interface color-scheme` (la
  clave que el portal XDG org.freedesktop.appearance expone). Respuesta
  `'prefer-dark'` = oscuro; cualquier fallo (sin gsettings en PATH, sin
  schema en la sesión) → `true` (oscuro, el default de la app). El Poll
  de app.rs refresca cada 5 s y eso ya funciona sin cambios (la función
  se sigue llamando `system_dark_mode()` desde app.rs y ui/poll.rs).
- Por qué gsettings y no D-Bus raw: mismo dato, sin reimplementar el
  protocolo de cable D-Bus a mano — tarea propia de una librería, no de
  este port (regla «no fabriques APIs»).

### src/config.rs
- Campo nuevo `inject_backend: Option<String>` (con doc-comment de los
  tres valores posibles y del fallback por valor desconocido).
- `Config::inject_settings()` ahora construye el `backend` de
  `InjectSettings` (parsing protegido: valor raro → `WlClipboard`).
- `load_api_key_secret` / `save_api_key_secret` ya estaban #[cfg] y NO se
  tocaron (en Linux son no-op: la key solo puede venir por `config.json`
  skip-serializing, como ya estaba).
- dirs::data_dir() ya resuelve XDG en Linux; config_path(),
  dictionary_path() funcionan sin cambios.

### src/bubble.rs
- Windows: TODO el winapi intacto (drag con SetCapture, noactivate,
  posicionamiento por monitor).
- Linux: la burbuja nace SIN poder robar el foco (en iced/winit Wayland
  `window::gain_focus` es Unsupported, xdg_activation_v1 hace el resto) —
  por eso la ausencia de WS_EX_NOACTIVATE es cosmética: no hay nada que
  desactivar.
- LOS STUBS del ratón (left_button_down, right_button_down,
  capture_left_mouse, release_mouse_capture, BubbleDrag) mantienen
  contratos semánticos correctos (no fires del Poll watchdog: valores
  neutros) y dejan log limitaciones «arrastre corto» / «sueltar fuera».
- `apply_bubble_extras` / `move_to_corner` son log informativo: el
  always-on-top y el pin se resuelven con windowrulev2 de Hyprland
  (ver §2), no con IPC embebido (fuera de alcance, ver tarea).

### Cargo.toml
- `winapi` movido a `[target.'cfg(windows)'.dependencies]` (con comentario
  del porqué). Cero dependencias Linux nuevas: el port usa std +
  binarios externos (wl-copy/wtype/ydotool/hyprctl/gsettings/notify-send),
  que el usuario instala con pacman.

## 2. Cómo se ve en Linux (lo que hace falta a mano)

### Dependencias de sistema (las instala el usuario; NO se tocaron)
```
pacman -S wl-clipboard wtype
# hyprctl ya viene con Hyprland (dependencia, no solo paquete)
# opcional: pacman -S ydotool + systemctl --user enable ydotoold
# opcional (aviso de segunda instancia): pacman -S libnotify
```
(gsettings ya está en cualquier sesión de Omarchy.)

### hyprland.conf recomendado (windowrulev2 por título) — recomendación, no código
```
windowrulev2 = float, title:^(WhisprOpen-Bubble)$
windowrulev2 = pin, title:^(WhisprOpen-Bubble)$
windowrulev2 = noborder, title:^(WhisprOpen-Bubble)$
```
El título de la burbuja es fijo y único (`WhisprOpen-Bubble`, constante
`BUBBLE_TITLE`): sin regla, Hyprland la tilea y se pierde. Esta tarea no
implementa el aplicar la windowrule por IPC (fuera de alcance); queda
como ajuste del propio hyprland.conf del usuario.

## 3. Limitaciones conocidas (primera versión del port, prácticas)

1. **Pegado con wtype**: apps Electron (Slack, Zed, etc.) tienen su propio
   binding del pegado: el `\x16` virtual entra como atajo si el
   compositor lo mapea; en caso contrario el texto puede llegar pegado
   por otro camino. A validar en la Parte 3 del port.
2. **ydotool sin daemon**: degrada bien, pero el mensaje al usuario
   («quedó copiado en el portapapeles») no menciona al daemon culpable —
   queda documentado aquí y en el log.
3. **Arrastre de la burbuja con botón derecho**: solo DENTRO de la
   burbuja (Wayland no expone el estado físico del botón fuera). Si el
   usuario suelta fuera, el drag se cierra en el siguiente
   BubbleRightRelease del canal de eventos, en lugar de cancelarse por
   el watchdog.
4. **Clic izquierdo saliendo de la burbuja**: el CursorLeft cierra el
   turno ANTES (sin esperar el soltado físico). Flujo normal no cambia.
5. **windowrule**: requiere hyprland.conf del usuario (no hay IPC).
6. **GNOME/KDE Wayland**: el guard de foco solo pregunta a Hyprland
   (hyprctl); con `inject_backend = ydotool` en cualquier compositor
   funciona (uinput), pero sin guard de ventana (sin filter de título en
   otros compositores; parche futuro fácil: `swaymsg -t get_tree`).
7. **Auto-tema**: corre por gsettings; WSL/SSH/contenedores dan `true`
   (oscuro) por fail-soft. La sesión real de Omarchy tiene gsettings.

## 4. Cómo probarlo a mano (cuando exista el binario)

```bash
# 1. Guard de instancia única: lanzar dos veces el binario.
./whispr-open &
./whispr-open   # debe salir con: "ya hay otra instancia" (notify-send o log)
cat $XDG_RUNTIME_DIR/whispr-open/instance.lock   # pid de la primera

# 2. Backend por config.json (config_path usa XDG, no ~/.config directo;
#    en Linux: $XDG_CONFIG_HOME/whispr-open o ~/.local/share/whispr-open
#    según cómo dirs::data_dir resuelva — ver docs/ARQUITECTURA.md):
#    "inject_backend": "wl-clipboard" | "ydotool" | "wtype"
# dicta; el texto debe caer en el campo activo; sin wtype en PATH → aviso.

# 3. Detección de tema:
gsettings set org.gnome.desktop.interface color-scheme 'prefer-dark'
# (Auto) → la burbuja/la ventana deben girar a oscuro en ≤5 s.
gsettings set org.gnome.desktop.interface color-scheme 'prefer-light'
# (Auto) → claro.

# 4. Filtro de ventana: config.json → "inject_window_filter": "firefox";
# dicta con Firefox enfocado (debe inyectar) y con VS Code (omitido).

# 5. Sin ydotoold (backend ydotool): systemctl --user stop ydotoold;
# dictar → "quedó copiado en el portapapeles", y pegar manual.
```

## 5. Pendientes declarados (para la Parte 3)

1. Validar en binario: `\x16` via wtype, target real de Hyprland, Electron.
2. ydotoold: path del socket bajo systemd --user de Arch (documentado
   arriba; no implementado por código).
3. `swaymsg`/`kdotool` para el guard de foco en otros compositores.
4. Build/release real + cargo test (295 tests) en una máquina con cargo.
5. Posicionamiento por iced Tasks: `window::get_position` es async; el
   flujo actual de persistencia de posición de burbuja solo persiste
   por Windows (drag nativo). Candidato para Parte 3 si se decide el
   circuito.

---

Verificado EN ESTA TAREA (lectura/razonamiento, sin build):
- Callsites (app.rs:883-898, ui/settings.rs:951-954,
  ui/settings_handlers.rs:307, ui/poll.rs:179-190, ui/bubble.rs:20-57,
  app.rs:518, 634, 708, 1971, 1980): todos usan la API que este port
  mantiene (firmas intactas).
- La única novedad de InjectSettings (campo `backend`) solo existe tras
  `#[cfg(unix)]`: Windows no la ve y sigue compilando igual.
- El dato gsettings/portal es documentación estándar de freedesktop
  (org.freedesktop.appearance color-scheme).
- «A validar» en binario está marcado en cada supuesto con riesgo.
