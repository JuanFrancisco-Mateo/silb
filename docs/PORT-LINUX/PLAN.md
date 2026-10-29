# PLAN — Port de Whispr Open a Linux (Wayland / Hyprland / Omarchy)

Plan módulo a módulo, en orden de dependencias, con el reparto de ficheros entre
agentes y la estrategia de compilación. Cada punto remite a los ids I1…I24 del
INVENTARIO.md (docs/PORT-LINUX/INVENTARIO.md, misma carpeta).

Idea de partida (lo pedía el encargo): **nadie toca un fichero central tipo
src/App**; cada módulo se porta detrás de un cfg, con el código de Windows
intacto dentro de `#[cfg(target_os = "windows")]` y el equivalente de Linux
detrás de `#[cfg(target_os = "linux")]`. Es el mismo patrón que ya usa el repo
(los 51 cfg que ya había).

---

## FASE 0 — Build y toolchain (bloqueante de todo lo demás)

Objetivo: poder hacer `cargo check` (y luego compilar) en Linux.

1. Rust en la máquina: NO está instalado (comprobado:
   `which cargo rustc rustup` → no encontrado; `pacman -Q rust` → no
   instalado). Comando que EJECUTA EL USUARIO (el sudo va por huella, no lo hago
   yo):

   sudo pacman -S --needed rustup cmake base-devel
   rustup default stable

2. dependencias de sistema mínimas para lo que pide el Cargo.toml nuevo:

   sudo pacman -S --needed gtk3 alsa-lib                  # ya están
   sudo pacman -S --needed libayatana-appindicator-gtk3   # tray-icon (I13)
   sudo pacman -S --needed noto-fonts-emoji               # glifos UI (fonts.rs)
   sudo pacman -S --needed wl-clipboard wtype              # ya están (I3/I4)

   (las cabeceras de glibc de base-devel las necesita whisper.cpp a través de
   whisper-rs bindgen; nada de cuda.)

3. primer `cargo check` en Linux (sin compilar la app entera; `cargo check` sí es
   rápido para iterar y NO es una compilación release). El lock se regenera con
   las opciones de cada sistema (afecta a tray-icon/global-hotkey/enigo, que
   desaparecen del objetivo Linux y con ello sus dependencias GTK/X11). Aceptado
   y documentado en INVENTARIO.md §10.

Riesgo: `cmake` configura whisper.cpp en la primera compilación (obligatorio
para whisper-rs en cualquier sistema). Solo aplica con GPU=no.

Criterio para pasar la fase 0: `cargo check` termina con la lista de errores en
`src/` (nada de fallos de dependencias) y se repite en verde con las opciones por
defecto sin cuda.

## FASE 1 — Módulos triviales, sin dependencia de ninguna otra fase

Orden: a la par que el inventario. Cada agente SU fichero (ver §4 reparto).

1. (I1/I2) single_instance.rs — flock + notify o eprintln.
   Criterio: la segunda instancia sale con aviso; test nuevo de flock en los
   tests del mismo fichero.
2. (I18-I22) llm.rs — cfgs de proceso/matar/rutas. Sin tocar ninguna otra pieza
   de llm.
   Criterio: compila; `is_server_exe` acepta "llama-server" en Linux y
   "llama-server.exe" en Windows (los tests de unidad que ya existen, adaptados).
3. (I24) theme.rs — system_dark_mode() por D-Bus org.freedesktop.appearance, o
   quedarse en oscuro por defecto (ya funciona así como apaño — si es temporal,
   documentado).
   Criterio: compila en Linux y en Windows sin cambios por ninguno de los dos
   lados.

## FASE 2 — Inyección de texto (las del flujo real del producto)

Depende de: nada de la FASE 1 (puede ir en paralelo, pero solo si son agentes
distintos y tocan ficheros distintos).

1. (I3) arboard: activarlo en Linux (Wayland ya funciona), y dejar el apaño de
   inject.rs *intacto* para no romper app.rs.
   Criterio: `cargo check` (y un `ls` de las dependencias de copy_to_clipboard)
   confirman que la rama linux llega con arboard de verdad.
2. (I4) Ctrl+V / tecleo — lanzar `wtype` (o `ydotool` para quien tenga el
   servicio). Fichero nuevo que envuelva los comandos; inject.rs lo usa.
   Alternativa decente si la app de destino no recibe lo que manda wtype:
   **wl-roots virtual keyboard**: hace falta permiso de uinput en la sesión del
   usuario (con el servicio `ydotool` en la sesión systemd del usuario funciona).
   Criterio: pegar de verdad en 3 apps (kitty, zen, libreoffice) — prueba manual
   del usuario; `cargo check` en el resto.
3. (I5) título de la ventana activa para window_filter — `hyprctl activewindow -j`
   con `serde_json`; si no está hyprctl, `None`.

## FASE 3 — Burbuja (la pieza más gorda del port)

Depende de: FASE 0 (compilar). No depende de las FASES 1/2.

1. (I6/I8/I9) colocar/mover/leer la posición por window::Id de iced
   (multiplataforma). Fácil una vez existe el hilo de ventanas.
2. (I7) NOACTIVATE — regla `nofocus` de Hyprland, documentada para el usuario
   (fragmento de hyprland.conf en packaging/linux/ del plan del port) + un TODO
   en la interfaz para el usuario. No bloquea la compilación.
3. (I10/I11/I12) arrastre — plan: `window::drag(id)` en la rama linux (el
   arrastre manual `BubbleDrag` se queda solo en Windows); si el arrastre propio de
   iced falla en Hyprland, segunda vuelta con ydotool (I11).
   Criterio: arrastrar con el botón derecho funciona en Hyprland, a mano.
4. (I24) la burbuja NO lleva tema propio; usa el de la app.

## FASE 4 — Tray y hotkey (integración con el escritorio)

Depende de: FASE 0 (dependencias de sistema instaladas).

1. (I13/I14) bandeja — tray-icon ya trae libappindicator; probarlo en Hyprland
   con waybar. Con waybar, el StatusNotifierItem debería pintarse bien. Si no se
   pinta: la opción de menú por dbus está documentada pero NO implementada (cuesta
   más).
   Criterio: el icono vive en waybar y sale el menú; el clic Mostrar/Ocultar
   afecta a la ventana principal.
2. (I15) atajo — bind de Hyprland en el config Lua del usuario:
   bind = CTRL SHIFT, D, exec, whispr-open --toggle
   + la opción `--toggle` en main.rs, que avisa a la instancia que ya está
   corriendo (socket unix en $XDG_RUNTIME_DIR/whispr-open.sock; la instancia viva
   lo atiende con std::os::unix::net::UnixListener). hotkey.rs se queda con un
   apaño para Linux + documentación. Prohibido forzar global-hotkey/X11.
   Criterio: Ctrl+Shift+D en Hyprland enciende y apaga la grabación (a mano).

## FASE 5 — Empaquetado y QA (después de todo lo demás)

1. .desktop + icono (packaging/linux/): whispr-open.desktop, icono SVG
   reutilizando el círculo violeta #7C6CFF de la bandeja.
2. Documentación para quien use Linux: cómo hacer el bind de Hyprland, cómo evitar
   que la burbuja robe el foco, qué hace falta de servicio (ydotool OPCIONAL).
3. Pruebas con micrófono de verdad (no se automatiza).

---

## 3. Estrategia de build (resumen de decisión)

- `cuda` de whisper-rs: opción opcional APAGADA por defecto (en el portátil no hay
  GPU NVIDIA; en Windows la RTX 4070 la usa). El Cargo.toml ya está retocado en
  esta rama: whisper-rs SOLO con la opción `cuda` si se pide `--features cuda` (o
  con la configuración de opciones del workspace).
- dependencias de Windows a `[target.'cfg(windows)'.dependencies]`:
  winapi, tray-icon, global-hotkey, enigo. El Cargo.toml ya está retocado.
- `arboard` se queda global (admite Wayland; trae la parte de X11 y la de
  Wayland, y Cargo.lock ya trae clipboard_wayland).
- NOTA: en esta rama todavía NO SE HA TOCADO src/. Este plan solo define la fase
  0 y la compilación. El port va en las fases 1-5.

---

## 4. Reparto de ficheros (este agente vs otros)

DUEÑO actual (dev-01, esta tarea): INVENTARIO.md, PLAN.md, Cargo.toml.
Sin tocar NINGÚN src/.

Reparto PROPUESTO para las fases 1-5 (lo asigna el organizador en las tarjetas
siguientes del kanban; cada fichero tiene un solo dueño):

| Fichero | Fase | Dueño propuesto |
|---|---|---|
| src/single_instance.rs | 1 | dev-02 |
| src/llm.rs (solo bloques cfg de proceso) | 1 | dev-01 |
| src/theme.rs | 1 | dev-03 |
| src/inject.rs | 2 | dev-02 |
| src/bubble.rs | 3 | dev-01 |
| src/tray.rs + ui/window_tray.rs | 4 | dev-03 |
| src/hotkey.rs + src/main.rs (--toggle) | 4 | dev-04 |
| src/fonts.rs + docs usuario | 1(late) | qa-01 |
| packaging/linux/ | 5 | dev-04 |
| QA manual (micrófono, drag, tray) | 5 | qa-02 |

Regla: SIEMPRE con cfg → nadie abre dos ficheros a la vez. Los ficheros que usan
todos (app.rs, ui/settings*, ui/poll.rs) NO SE TOCAN salvo con retoques
quirúrgicos aprobados por el organizador.

---

## 5. Riesgos abiertos (para el orquestador)

- wtype contra Wayland text-input no funciona en TODAS las apps (número de
  pruebas manuales: kitty ✓, zen sin probar, libreoffice sin probar).
  El camino B con ydotool funciona en todas pero necesita servicio + permisos de
  uinput (grupo input / regla udev). Decisión recomendada: empezar con wtype, y si
  falla en alguna app importante → ydotool.
- La bandeja en waybar, aunque libappindicator es antiguo (AppIndicator v1),
  funciona SIEMPRE por StatusNotifierItem; riesgo bajo.
- NOACTIVATE: no se puede garantizar sin una regla del compositor; la experiencia
  puede cambiar.
- llama-server en Linux: no está en los repos de Arch; se resuelve con un
  endpoint externo (Groq/Gemini) o compilando llama.cpp a mano y dejando el
  binario el usuario, FUERA de esta app.
