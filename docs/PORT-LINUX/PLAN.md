# PLAN — Port de Whispr Open a Linux (Wayland / Hyprland / Omarchy)

Plan módulo a módulo con orden de dependencias, reparto de ficheros entre
agentes y estrategia de build. Referencia de cada punto: los ids I1…I24
del INVENTARIO.md (docs/PORT-LINUX/INVENTARIO.md, mismo directorio).

Principio rector (requisito del encargo): **nadie toca un fichero
central tipo src/App**; cada módulo se porta tras cfg, con el código
Windows intacto tras `#[cfg(target_os = "windows")]` y el equivalente
Linux detrás de `#[cfg(target_os = "linux")]`. Es el mismo patrón que el
repo ya usa (los 51 cfg existentes).

---

## FASE 0 — Build y toolchain (bloqueante de todo lo demás)

Objetivo: poder hacer `cargo check` (y luego build) en Linux.

1. toolchain de Rust en la máquina: NO instalado (verificado:
   `which cargo rustc rustup` → no encontrado; `pacman -Q rust` → no
   instalado). Comando que EJECUTA EL USUARIO (sudo por huella, no yo):

   sudo pacman -S --needed rustup cmake base-devel
   rustup default stable

2. deps de sistema mínimas para el grafo del Cargo.toml nuevo:

   sudo pacman -S --needed gtk3 alsa-lib                  # ya están
   sudo pacman -S --needed libayatana-appindicator-gtk3   # tray-icon (I13)
   sudo pacman -S --needed noto-fonts-emoji               # glifos UI (fonts.rs)
   sudo pacman -S --needed wl-clipboard wtype              # ya están (I3/I4)

   (glibc headers de base-devel hacen falta a whisper.cpp vía
   whisper-rs bindgen; no cuda.)

3. primer `cargo check` en Linux (sin compilar la app completa;
   `cargo check` sí es iterativo y NO es un build release). El lock se
   regenera con las features por target (afecta a tray-icon/global-hotkey/
   enigo, que desaparecen del target Linux y con ello sus deps GTK/X11
   del grafo). Aceptado y documentado en INVENTARIO.md §10.

Riesgo: `cmake` configura whisper.cpp en el primer build (obligatorio
para whisper-rs en cualquier target). Solo aplica con GPU=no.

Criterio de pase de fase 0: `cargo check` termina lista de errores en
`src/` (nada de fallos de deps) y `cargo check` se repite verde con
features default sin cuda.

## FASE 1 — Módulos triviales, sin dependencia de ninguna otra fase

Orden: mano a mano con inventario. Cada agente SU fichero (ver §4 reparto).

1. (I1/I2) single_instance.rs — flock + notify o eprintln.
   Criterio: la segunda instancia sale con aviso; test nuevo de flock en
   tests del mismo fichero.
2. (I18-I22) llm.rs — cfgs de proceso/matasar/rutas. Sin tocar ninguna
   otra pieza de llm.
   Criterio: compila; `is_server_exe` acepta "llama-server" en Linux y
   "llama-server.exe" en Windows (tests unidad ya existentes adaptados).
3. (I24) theme.rs — system_dark_mode() via D-Bus org.freedesktop.appearance
   o caer a oscuro default (ya funciona como stub — si es temporal
   documentado).
   Criterio: compila en Linux y en Windows sin cambios de ambos lados.

## FASE 2 — Inyección de texto (las del flujo real del producto)

Depende de: nada de FASE 1 (puede ir en paralelo solo si los agentes son
distintos y tocan ficheros distintos).

1. (I3) arboard: activar en Linux (ya funciona Wayland), mantener stub por
   superficie del módulo inject.rs *intacta* para no romper app.rs.
   Criterio: `cargo check` (y ls de deps de copy_to_clipboard) la rama
   linux llega con arboard real.
2. (I4) Ctrl+V / tecleo — spawn de `wtype` (o `ydotool` para quien tenga
   daemon). Fichero nuevo que envuelva los comandos; inyect.rs consume.
   Alternativa digna si la app destino no recibe wtype inputs: **wl-roots
   virtual keyboard**: requiere permiso ucode input del user session (con
   ydotool daemon en el systemd user session funciona).
   Criterio: paste real en 3 apps (kitty, zen, libreoffice) — test manual
   del usuario; cargo check en el resto.
3. (I5) título de la ventana activa para window_filter — `hyprctl
   activewindow -j` con `serde_json`; fallback `None` si hyprctl no está.

## FASE 3 — Burbuja (la pieza más gorda del port)

Depende de: FASE 0 (compilar). Sin dependencias de FASE 1/2.

1. (I6/I8/I9) posicionar/mover/leer posición por window::Id de iced
   (multiplataforma). Trivial una vez el hilo de ventanas existe.
2. (I7) NOACTIVATE — regla hyprland `nofocus` documentada para el usuario
   (fragmento hyprland.conf en packaging/linux/ del plan del port) +
   TODO en la UI para el usuario. No bloquea el build.
3. (I10/I11/I12) arrastre — plan: `window::drag(id)` en la rama linux
   (el drag manual `BubbleDrag` queda windows-only); si el arrastre nativo
   de iced falla en Hyprland, segunda iteración con ydotool (I11).
   Criterio: drag con botón derecho funciona en Hyprland manual.
4. (I24) la burbuja NO lleva tema propio; usa theme.

## FASE 4 — Tray y hotkey (integración con el escritorio)

Depende de: FASE 0 (deps de sistema instaladas).

1. (I13/I14) tray — tray-icon ya emporta libappindicator; probarlo en
   Hyprland con waybar. Con waybar el StatusNotifierItem debería pintarlo
   bien. Si no pinta: opción panel dbus-menu (menú dbus)
   documentada pero NO implementada (más costo).
   Criterio: icono vive en waybar y menú sale; clic Mostrar/Ocultar
   aplica a la ventana principal.
2. (I15) hotkey — bind de Hyprland en el config Lua del usuario:
   bind = CTRL SHIFT, D, exec, whispr-open --toggle
   + flag --toggle en main.rs que emite señal a la instancia viva
   (unix socket en $XDG_RUNTIME_DIR/whispr-open.sock; la instancia viva
   lo sirve con std::os::unix::net::UnixListener). hotkey.rs queda
   stub Linux + doc. Prohibido forzar global-hotkey/X11.
   Criterio: Ctrl+Shift+D en Hyprland togglea grabación (manual).

## FASE 5 — Empaquetado y QA (después de todo lo demás)

1. .desktop + icono (packaging/linux/): whispr-open.desktop, icono SVG
   reutilizando el círculo violeta #7C6CFF del tray.
2. Documentación de usuario Linux: cómo hacer el bind de Hyprland, cómo evitar que la
   burbuja robe el foco, qué hace falta de daemon (ydotool OPCIONAL).
3. QA con micrófono real (no se automatiza).

---

## 3. Estrategia de build (resumen de decisión)

- `cuda` de whisper-rs: feature opcional OFF por defecto (no hay GPU
  NVIDIA en el portátil; en Windows la RTX 4070 la usa). Cargo.toml ya
  retocado en esta rama: whisper-rs SOLO con feature `cuda` si se pide
  `--features cuda` (o config de feature en el workspace).
- deps de Windows a `[target.'cfg(windows)'.dependencies]`:
  winapi, tray-icon, global-hotkey, enigo. Cargo.toml ya retocado.
- `arboard` queda global (soporta Wayland; trae backend X11/Wayland,
  Cargo.lock ya trae clipboard_wayland).
- NOTA: src/ AÚN NO SE HA TOCADO en esta rama. Este plan solo define
  fase 0 + build. El port va en las fases 1-5.

---

## 4. Reparto de ficheros (este agente vs otros)

DUEÑO actual (dev-01, esta tarea): INVENTARIO.md, PLAN.md, Cargo.toml.
Sin tocar NINGÚN src/.

Reparto PROPUESTO para las fases 1-5 (a asignar por el orquestador en
cards kanban subsiguientes; cada fichero tiene un solo dueño):

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

Regla: SIEMPRE cfg → nadie abre dos ficheros a la vez. Los ficheros
cross-cutting (app.rs, ui/settings*, ui/poll.rs) NO SE TOCAN salvo por
patch quirúrgicos aprobados por el orquestador.

---

## 5. Riesgos abiertos (para el orquestador)

- wtype contra Wayland text-input no funciona en TODAS las apps
  (núm. de tests manuales: kitty ✓, zen desconocido, libreoffice desconocido).
  Path B vía ydotool funciona en todo pero requiere daemon + permisos
  de uinput (grupo input / udev rule). Decisión recomendada: empezar con
  wtype, y si falla en alguna app crítica → ydotool.
- El tray en waybar aunque libappindicator es legacy (AppIndicator v1),
  SIEMPRE funciona vía StatusNotifierItem; riesgo bajo.
- NOACTIVATE: no garantizable sin regla de compositor; UX puede variar.
- llama-server en Linux: no está en repos Arch; resolver con endpoint
  externo (Groq/Gemini) o build local de llama.cpp empaquetado a binario por
  el usuario, FUERA de esta app.
