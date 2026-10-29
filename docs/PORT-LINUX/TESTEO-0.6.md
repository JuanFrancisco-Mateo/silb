# TESTEO 0.6 en el portátil (Arch + Hyprland/Wayland)

Todo esto ya está hecho y comprobado desde el sobremesa; lo que falta es la
prueba con las manos.

## Preparado

- Código y binario: `~/silb-linux` (el mismo `src/` que `release/0.6.0`,
  comprobado por hash) → `target/release/silb`, ~23 MB.
- Modelo: `~/.local/share/whispr-open/models/ggml-small.bin` (487 MB, cabecera
  GGML válida, ya cacheado por la app).
- Micrófono: PipeWire en marcha, `alsa_input.pci-0000_07_00.6.analog-stereo`.
- Escritura del texto: `wl-copy` y `wtype` instalados (no hay `ydotool`).
- Atajo: bind de Hyprland añadido a `~/.config/hypr/bindings.lua` →
  **SUPER + SHIFT + D** escribe `toggle` en
  `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock`. Recargado con `hyprctl reload`.
- VAD: no hay `ggml-vad-q8.bin`. Es opcional; si se quiere probar, se deja en la
  carpeta de modelos.

## Lanzar

```bash
bash ~/silb-linux/scripts/run-linux.sh
```

En Hyprland no hay icono junto al reloj (no hay nada que lo pinte): la app se
cierra desde su propia ventana.

## Qué comprobar (y qué apuntar)

| # | Prueba | OK si… |
|---|---|---|
| 1 | Arranca y se ve la ventana | aparece la ventana principal, sin «Cargando» infinito |
| 2 | Prerroll del modelo | el log dice `whisper: modelo listo en cache (prerroll)` |
| 3 | SUPER+SHIFT+D | alterna dictado (el estado del micro cambia en la UI) |
| 4 | Dictado real | la transcripción aparece en la ventana con texto correcto en español |
| 5 | Inyección | con el foco en otra app, el texto se pega/teclea al terminar |
| 6 | Cambio de modelo (Ajustes) | tiny/base descargan y transcriben; volver a small |
| 7 | Corrección IA | desactivada por defecto (no hay GGUF): el dictado funciona igual |
| 8 | Cierre | se cierra limpiamente desde la UI y no queda proceso `silb` |

## Dónde mirar cuando algo falle

- Registro de la app: `~/.local/share/whispr-open/whispr-open.log`
- Comprobación automática: `bash ~/silb-linux/scripts/linux-check.sh`
  (deja `docs/VERIFICACION-LINUX-<fecha>.txt`)
- Socket del atajo: `$XDG_RUNTIME_DIR/whispr-open/hotkey.sock`

Lo que falle, con la línea exacta del registro, se apunta en el informe de la 0.6.
