# Atajo global en Hyprland (backend "hyprland")

Estado: LISTO PARA VALIDAR (la compilación y la prueba real de teclado las hace
el usuario — el agente no puede ejecutar `cargo build` en esta máquina).

## Por qué no hay atajo global nativo

En Windows, Whispr Open registra su atajo con `global-hotkey` (registro global del
sistema: funciona sobre cualquier app que tenga el foco). En Linux con Wayland ese
mecanismo no existe y no puede existir: el protocolo no permite que una app capture
teclas de otras ventanas (es una decisión de seguridad de Wayland, no un fallo de
Hyprland ni de cpal/winit).

`global-hotkey` 0.6 en Linux solo soporta X11 (docs del crate: "Linux (X11
Only)"); en una sesión Hyprland/Wayland pura el registro no hace nada. No se usa.

## Solución del port: socket UNIX + bind de Hyprland

La app, al arrancar (parte «hyprland»), crea un socket UNIX a la escucha en:

    $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

y un hilo propio hace `accept` en bucle. Cualquier línea que se escriba
en el socket se convierte en un evento de atajo interno:

- `toggle` → enciende o apaga el dictado (como pulsar el atajo).
- `off` → palabra reservada: no hace nada (útil para probar el socket sin
  disparar el dictado).
- cualquier otra línea no vacía → se trata como toggle (y se apunta en el registro
  como comando desconocido: el socket aguanta lo que le llegue, no se rompe por
  basura).

El atajo configurado en la interfaz o en la configuración (`shortcut`, por defecto
"Ctrl+Shift+D") se queda como etiqueta informativa; en Wayland ES el bind de
Hyprland el que decide qué tecla dispara el toggle (la referencia de verdad del
bind es el `hyprland.conf` del usuario).

## Bind recomendado en hyprland.conf

    # Whispr Open — toggle de dictado (Ctrl+Shift+D equivale a SUPER SHIFT D)
    bind = SUPER SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

Con `$mainMod` (si tu configuración lo define, que es lo habitual:
`$mainMod = SUPER`):

    bind = $mainMod SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

Notas:

- `echo` escribe «toggle\n» y cierra: cada pulsación = UNA conexión = UN toggle.
  Fiable aunque pulses varias veces seguidas (cada exec es un proceso nuevo y una
  conexión nueva).
- La app tiene que estar corriendo ANTES de la primera pulsación; si el socket no
  existe, el shell devuelve «No such file or directory» y no pasa nada (el bind
  falla en silencio, no rompe Hyprland).
- Ruta alternativa si no existe `$XDG_RUNTIME_DIR` (no debería pasar en una sesión
  Wayland normal): la app usa `~/.cache/whispr-open/hotkey.sock` y lo apunta al
  arrancar. El bind debería apuntar a esa misma ruta.

## Cómo probar (checklist del usuario, sin tocar código)

1. Arrancar la app en un terminal: `cargo run` (o el binario release).
2. Comprobar que el socket existe:

       ls -l $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

3. Probar el toggle sin teclado:

       echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

   → la ventana de la app debe pasar a estado «Grabando» (o el editor abre el turno
   de dictado, según cómo esté ahora).

4. Añadir el bind de arriba a `~/.config/hypr/hyprland.conf` y recargar
   (`hyprctl reload`). Pulsar `SUPER+SHIFT+D` → toggle.

5. Palabra reservada (no cambia nada): `echo off > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock`
   → en el registro de la app aparece «socket recibió «off» (reservado; sin
   efecto)». Sirve para verificar que está todo conectado sin disparar el dictado.

## Permisos y seguridad

- El socket vive en `$XDG_RUNTIME_DIR` (memoria temporal del usuario, 0700): otro
  usuario del sistema no puede ni verlo ni escribir en él.
- Los procesos del MISMO usuario sí pueden: es cómo funciona el bind de Hyprland
  (se ejecuta como tu usuario). Es el mismo nivel de confianza que un atajo
  registrado en el sistema.
- El hilo del socket NO ejecuta nada de lo que recibe: solo cuenta líneas y emite
  «toggle» (cualquier otra cosa va al registro, nunca a un shell).

## Detalles de implementación (para quien mantenga esto)

- Código: `src/hotkey.rs` — parte `hyprland` (módulo `hyprland`), listener
  `HotkeySocketListener` en `HotkeyManager::register()` bajo
  `#[cfg(target_os = "linux")]`.
- Eventos: el hilo emite `GlobalHotKeyEvent { id: <hash del atajo canónico>,
  state: Pressed }` al canal que entrega `HotkeyManager::listen()`; el Poll de la
  app (100 ms) filtra por `hotkey.id()` y hace toggle. El mismo contrato que la
  parte de Windows (app.rs/ui/poll.rs no distinguen uno de otro).
- Id del evento: FNV-1a de 32 bits del texto canónico del atajo ("Ctrl+Shift+D"),
  el mismo análisis que se usa al normalizar la configuración común a los dos
  sistemas (hay tests que lo fijan).
- Limpieza: al morir la app el socket se borra (y en cada arranque se hace un
  unlink previo de un socket abandonado por una ejecución anterior).
- Palabra reservada «off» y aguante ante líneas desconocidas: ver arriba.
