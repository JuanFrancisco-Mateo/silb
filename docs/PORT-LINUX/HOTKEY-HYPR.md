# Atajo global en Hyprland (backend "hyprland")

Estado: LISTO A VALIDAR (compilación y prueba real de teclado pendientes del
usuario — el agente no puede ejecutar `cargo build` en esta máquina).

## Por qué no hay atajo global nativo

En Windows, Whispr Open registra su atajo con `global-hotkey` (registro
global del sistema: funciona sobre cualquier app con foco). En Linux con
Wayland ese mecanismo no existe y no puede existir: el protocolo no permite
a una app capturar teclas de otras superficies (es una decisión de seguridad
de Wayland, no un bug de Hyprland ni de cpal/winit).

`global-hotkey` 0.6 en Linux solo soporta X11 (docs del crate:
"Linux (X11 Only)"); en una sesión Hyprland/Wayland pura el registro no
tiene efecto. No se usa.

## Solución del port: socket UNIX + bind de Hyprland

La app, al arrancar (backend "hyprland"), crea un socket UNIX de escucha en:

    $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

y un hilo propio hace `accept` en bucle. Cualquier línea escrita en el
socket se traduce a un evento de atajo interno:

- `toggle` → alterna dictado (equivalente a pulsar el atajo).
- `off` → palabra reservada: no hace nada (útil para probar el socket sin
  disparar el dictado).
- cualquier otra línea no vacía → tratada como toggle (y logueada como
  comando desconocido: el socket es tolerante, no rompe por basura).

El atajo configurado en la UI/config (`shortcut`, default "Ctrl+Shift+D")
se conserva como etiqueta informativa; en Wayland ES el bind de Hyprland el
que decide qué tecla dispara el toggle (fuente de verdad del bind: el
`hyprland.conf` del usuario).

## Bind recomendado en hyprland.conf

    # Whispr Open — toggle de dictado (Ctrl+Shift+D equivale a SUPER SHIFT D)
    bind = SUPER SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

Con `$mainMod` (si tu conf lo define, es habitual: `$mainMod = SUPER`):

    bind = $mainMod SHIFT, D, exec, echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

Notas:

- `echo` escribe «toggle\n» y cierra: cada pulsación = UN connect = UN
  toggle. Fiable aunque se pulse varias veces seguidas (cada exec es un
  proceso nuevo y un connect nuevo).
- La app debe estar corriendo ANTES de la primera pulsación; si el socket
  no existe, el shell devuelve «No such file or directory» y no pasa nada
  (el bind falla silenciosamente, no crashea Hyprland).
- Ruta alternativa si no existe `$XDG_RUNTIME_DIR` (no debería en una
  sesión Wayland estándar): la app cae a `~/.cache/whispr-open/hotkey.sock`
  y lo loguea en su arranque. El bind debería apuntar a la misma ruta.

## Cómo probar (checklist del usuario, sin tocar código)

1. Arrancar la app en un terminal: `cargo run` (o el binario release).
2. Comprobar que el socket existe:

       ls -l $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

3. Probar el toggle sin teclado:

       echo toggle > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock

   → la ventana de la app debe pasar a estado «Grabando» (o el editor
   abre el turno de dictado, según el estado actual).

4. Añadir el bind de arriba a `~/.config/hypr/hyprland.conf` y recargar
   (`hyprctl reload`). Pulsar `SUPER+SHIFT+D` → toggle.

5. Palabra reservada (no togglea): `echo off > $XDG_RUNTIME_DIR/whispr-open/hotkey.sock`
   → en el log de la app aparece «socket recibió «off» (reservado; sin
   efecto)». Sirve para verificar el cableado sin disparar el dictado.

## Permisos y seguridad

- El socket vive en `$XDG_RUNTIME_DIR` (tmpfs del usuario, 0700): otro
  usuario del sistema no puede ni verlo ni escribirlo.
- Procesos del MISMO usuario sí pueden: es el modelo del bind de Hyprland
  (se ejecuta como tu usuario). Es el mismo nivel de confianza que un
  atajo registrado en el sistema.
- El hilo del socket NO ejecuta nada de lo que recibe: solo cuenta líneas
  y emite "toggle" (cualquier otra cosa va al log, nunca a un shell).

## Detalles de implementación (para quien mantenga esto)

- Código: `src/hotkey.rs` — backend `hyprland` (módulo `hyprland`), listener
  `HotkeySocketListener` en `HotkeyManager::register()` bajo
  `#[cfg(target_os = "linux")]`.
- Eventos: el hilo emite `GlobalHotKeyEvent { id: <hash del atajo canónico>,
  state: Pressed }` al canal que `HotkeyManager::listen()` entrega; el Poll
  de la app (100 ms) filtra por `hotkey.id()` y hace toggle. Mismo contrato
  que el backend Windows (app.rs/ui/poll.rs no distinguen backends).
- Id del evento: FNV-1a 32 bits del label canónico ("Ctrl+Shift+D"), el
  mismo parse que normaliza config compartida entre SO (tests lo fijan).
- Limpieza: al morir la app el socket se borra (y a cada arranque se hace
  unlink previo de un socket huérfano de una ejecución anterior).
- Palabra reservada "off" y tolerancia a líneas desconocidas: ver arriba.
