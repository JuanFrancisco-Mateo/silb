# CODIGO-CORE.md — Port whispr-open a Linux (Parte 2: código platform-specific)

Rama: `feature/port-linux-codigo-core` · Autor: dev-02 · Fecha: 03/10/2026

Qué es: el registro de los cambios de los 5 ficheros de este reparto (inject,
single_instance, theme, config, bubble) para que la app compile en Windows Y en
Linux (Omarchy: Arch + Hyprland sobre Wayland), degradándose con elegancia y sin
tocar los ficheros de otros devs.

Normas aplicadas: AGENTS.md + docs/ARQUITECTURA.md + docs/ROADMAP.md. Frente a la
regla de «sin builds»: NO se compiló nada (en esta máquina no hay herramientas de
Rust; el análisis es de leer y razonar). La compilación de verdad la hará
Port/qa-01 con los pendientes de aquí abajo.

---

## 1. Qué cambió por fichero

### src/inject.rs (núcleo del port)
- Windows intacto detrás de `#[cfg(target_os = "windows")]`: arboard + enigo,
  guard de foco, filtro de ventana, Ctrl+V con soltar garantizado.
- Linux (`#[cfg(unix)]`), con la forma de escribir elegible por configuración
  (`inject_backend`):
  - `wl-clipboard` (por defecto): `wl-copy` (el texto por stdin) + `wtype` para
    la pulsación del pegado (el byte `\x16` por stdin; A VALIDAR en el binario).
  - `ydotool`: `ydotool type -- <texto>` (uinput, funciona en todas partes —
    Xwayland incluido). Sin `ydotoold` corriendo NO teclea: el turno se queda en
    el portapapeles y se avisa (se degrada, pero nunca se pierde texto).
  - `wtype`: tecleo directo por teclado virtual; SE ATASCA con las apps que
    bloquean zwp_virtual_keyboard_v1 → mensaje claro + portapapeles.
- Detección de la ventana con el foco: `hyprctl activewindow -j` (title y, si
  falta, class, para las ventanas sin título). Lector de JSON mínimo propio
  (`json_string_field`, con la biblioteca estándar, con tests: usa muestras
  REALES de hyprctl).
- El recorrido es el mismo en ambos sistemas: guard de foco → filtro de ventana →
  espera → forma de escribir. El camino de Windows mantiene PURO su comportamiento
  original (ni una sola palabra de winapi fuera de su cfg).
- `copy_to_clipboard`: wl-copy por stdin (no por argumentos, así no se ve el texto
  en `ps`).

### src/single_instance.rs
- Windows: intacto (CreateMutexW, espacio de nombres Local\, aviso con
  MessageBoxW).
- Linux: fichero de bloqueo con flock (std `File::try_lock`, estable desde Rust
  1.88) en `$XDG_RUNTIME_DIR/whispr-open/instance.lock` (si no, en $TMPDIR). El
  candado es del propio sistema: un cierre forzado no lo deja pegado. Se guarda el
  pid como pista para diagnosticar (no es la referencia de verdad). Aviso al
  usuario de la segunda instancia con `notify-send` (si se puede, sin servicio →
  solo registro). Regla heredada: si no se puede crear el bloqueo (permisos,
  disco de solo lectura), se sigue SIN guard (= nunca rompe el arranque), como en
  Windows.

### src/theme.rs
- Windows (AppControllerTheme): el registro AppsUseLightTheme intacto.
- Linux: `gsettings get org.gnome.desktop.interface color-scheme` (la clave que
  expone el portal XDG org.freedesktop.appearance). Respuesta `'prefer-dark'` =
  oscuro; cualquier fallo (sin gsettings en el PATH, sin el esquema en la sesión)
  → `true` (oscuro, lo normal en la app). El Poll de app.rs refresca cada 5 s y
  eso ya funciona sin cambios (la función se sigue llamando `system_dark_mode()`
  desde app.rs y ui/poll.rs).
- Por qué gsettings y no D-Bus a pelo: es el mismo dato, sin rehacer el protocolo
  de D-Bus a mano — eso es trabajo de una librería, no de este port (regla «no
  fabriques APIs»).

### src/config.rs
- Campo nuevo `inject_backend: Option<String>` (con comentario de documentación de
  los tres valores posibles y de qué pasa con un valor desconocido).
- `Config::inject_settings()` ahora arma el `backend` de
  `InjectSettings` (lectura protegida: valor raro → `WlClipboard`).
- `load_api_key_secret` / `save_api_key_secret` ya estaban con #[cfg] y NO se
  tocaron (en Linux no hacen nada: la key solo puede venir por `config.json`
  skip-serializing, como ya estaba).
- dirs::data_dir() ya resuelve XDG en Linux; config_path() y
  dictionary_path() funcionan sin cambios.

### src/bubble.rs
- Windows: TODO el winapi intacto (arrastre con SetCapture, noactivate,
  colocación por monitor).
- Linux: la burbuja nace SIN poder robar el foco (en iced/winit sobre Wayland
  `window::gain_focus` es Unsupported, y xdg_activation_v1 hace el resto) — por eso
  que no exista WS_EX_NOACTIVATE es solo estético: no hay nada que desactivar.
- Los APAÑOS del ratón (left_button_down, right_button_down,
  capture_left_mouse, release_mouse_capture, BubbleDrag) mantienen contratos con
  sentido (no disparan el vigilante del Poll: valores neutros) y dejan en el
  registro las limitaciones «arrastre corto» / «soltar fuera».
- `apply_bubble_extras` / `move_to_corner` solo apuntan en el registro: lo de estar
  siempre encima y el fijado se resuelven con windowrulev2 de Hyprland
  (ver §2), no hablando con el compositor desde la app (fuera de alcance, ver la
  tarea).

### Cargo.toml
- `winapi` movido a `[target.'cfg(windows)'.dependencies]` (con un comentario
  del porqué). Cero dependencias nuevas para Linux: el port usa la biblioteca
  estándar + programas externos (wl-copy/wtype/ydotool/hyprctl/gsettings/
  notify-send), que el usuario instala con pacman.

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
`BUBBLE_TITLE`): sin la regla, Hyprland la coloca en mosaico y se pierde. Esta
tarea no pone la windowrule hablando con el compositor (fuera de alcance); queda
como ajuste del hyprland.conf del usuario.

## 3. Limitaciones conocidas (primera versión del port, prácticas)

1. **Pegar con wtype**: las apps Electron (Slack, Zed, etc.) tienen su propia
   combinación de pegado: el `\x16` virtual entra como atajo si el compositor lo
   traduce; si no, el texto puede llegar pegado por otro camino. A validar en la
   Parte 3 del port.
2. **ydotool sin servicio**: se degrada bien, pero el mensaje al usuario
   («quedó copiado en el portapapeles») no dice que el culpable es el servicio —
   queda documentado aquí y en el registro.
3. **Arrastrar la burbuja con el botón derecho**: solo DENTRO de la burbuja
   (Wayland no expone el estado físico del botón fuera de ella). Si el usuario
   suelta fuera, el arrastre se cierra en el siguiente BubbleRightRelease del
   canal de eventos, en lugar de cancelarse por el vigilante.
4. **Clic izquierdo saliendo de la burbuja**: el CursorLeft cierra el turno ANTES
   (sin esperar a que se suelte el botón). El flujo normal no cambia.
5. **windowrule**: hace falta el hyprland.conf del usuario (no hay forma de
   hablarlo desde la app).
6. **GNOME/KDE Wayland**: el guard de foco solo pregunta a Hyprland
   (hyprctl); con `inject_backend = ydotool` en cualquier compositor
   funciona (uinput), pero sin guard de ventana (sin filtro de título en
   otros compositores; arreglo futuro fácil: `swaymsg -t get_tree`).
7. **Auto-tema**: va por gsettings; WSL/SSH/contenedores dan `true`
   (oscuro) porque falla y se queda con lo normal. La sesión real de Omarchy tiene
   gsettings.

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

1. Validar en el binario: `\x16` por wtype, ventana real de Hyprland, Electron.
2. ydotoold: ruta del socket bajo systemd --user de Arch (documentado
   arriba; no implementado en código).
3. `swaymsg`/`kdotool` para el guard de foco en otros compositores.
4. Compilación y release de verdad + cargo test (295 tests) en una máquina con
   cargo.
5. Colocar la burbuja con tareas de iced: `window::get_position` es asíncrono; el
   flujo actual de guardar la posición de la burbuja solo guarda
   por Windows (arrastre nativo). Candidato para la Parte 3 si se decide cómo.

---

Comprobado EN ESTA TAREA (leyendo y razonando, sin compilar):
- Los sitios que llaman (app.rs:883-898, ui/settings.rs:951-954,
  ui/settings_handlers.rs:307, ui/poll.rs:179-190, ui/bubble.rs:20-57,
  app.rs:518, 634, 708, 1971, 1980): todos usan la API que este port
  mantiene (las firmas no cambian).
- La única novedad de InjectSettings (el campo `backend`) solo existe tras
  `#[cfg(unix)]`: Windows no la ve y sigue compilando igual.
- El dato de gsettings/portal es documentación estándar de freedesktop
  (org.freedesktop.appearance color-scheme).
- «A validar» en el binario está marcado en cada suposición con riesgo.
