# PORT-LINUX / CODIGO-UI — resquicios windows-especificos que quedan

Parte 4 no es la dueña de los ficheros de inyeccion/burbuja/tray (Partes 2
y 3). Aqui se documenta CADA residuo Windows-especifico detectado leyendo
el codigo, con el fichero/linea real, el estado actual, y a quien
corresponde portarlo.

## Resumen del inventario (03/10/2026)

| Fichero | residuo | Estado | Dueño |
|---|---|---|---|
| src/inject.rs | foreground_window_title() usa GetForegroundWindow/GetWindowTextW para filtrar ventana destino; copy/inject igualmente stub en no-Windows | cfg existe, funcionalmente sin port Wayland | Parte 3 (audio runtime) / Parte 2 |
| src/bubble.rs | WS_EX_NOACTIVATE, SetCapture de drag, WindowAttributesExtWindows; ya envuelto todo en cfg(target_os=windows) con stubs | ya portado a stub; port real Wayland queda como tecnica futura | Parte 3 |
| src/tray.rs | tray-icon 0.24 ventana oculta Win32 | stub en no-Windows (verificar) | Parte 3 |
| src/hotkey.rs | global-hotkey 0.6 (crate cross-platform) | sin residuos Windows detectados en el fichero (grep limpio) | ya jugado |
| src/fonts.rs | candidatos emoji con 2 rutas C:\Windows\... (lineas 32-33) y rutas /usr/share/fonts/noto* (lineas 35-38) | lista multiplataforma ya operativa en Linux (cae a NotoColorEmoji); los tests de las lineas 139-141 solo asumen orden de la lista | ok en el port; ajuste cosmético pendiente |
| src/single_instance.rs | CreateMutexW guard; stub no-Windows | ya portado (stub true) | ya jugado |
| src/config.rs | load_api_key_secret/save via WinAPI Credential Manager; stub no-Windows (nunca guarda key) | ya portado; port keyring Linux (libsecret) queda como TASK FUERA de la parte | pendiente (TODO fijo citado abajo) |
| src/llm.rs | launch_server cfg(windows) CREATE_NO_WINDOW; resolve_model_path ya usa roots por cfg | ya portado en roots (cfg en Windows C:/D: vs Linux ~/.local/share/llama.cpp, /opt/llama.cpp) | ya jugado (roots), Parte 2/3 real port del lanzamiento del server |
| src/audio.rs | sin grep windows | limpio | – |
| src/app.rs | Iced UI | portable (Iced cross-plataforma), sin cfg | – |
| src/transcribe.rs | cuda en cfg(windows) dep (Cargo.toml); models_dir ya por cfg | ya portado en esta parte (Cargo.toml whisper-rs default-features=false + cfg(windows) cublas) | ya jugado |
| src/custom_image.rs, src/icon_header.rs, src/ui/*.rs | sin residuos window-specific detectados (grep limpio salvo doc-comments) | – | – |

## Pendientes declarados (a quien va)

1. **Keyring de API key en Linux.** `save_api_key_secret` en
   `src/config.rs` es stub no-Windows: la API key de la corrección IA no
   se persiste en Linux. Port adecuado: crate `keyring` + libsecret
   (gnome-keyring), o cifrar en config.local. → tarea futura, Parts 2/3 o
   ticket dedicado.
2. **Inyección de texto en Wayland.** inject.rs: copy_to_clipboard SÍ
   está portado a arboard en Linux (funciona en Wayland via data-control).
   El tecleo con enigo no funciona en Wayland (enigo solo X11).
   Alternativa wl-clipboard + ydotool (requiere permisos input daemon)
   — cae en el reparto de inyección (Parte 3).
3. **tray-icon en Linux.** src/tray.rs stub non-Windows: bandeja no existe
   en el port actual. Alternativa: libappindicator-gtk3 via tray-icon con
   feature dbus → camino pendiente de probe (Parte 3).
4. **Burbuja flotante.** src/bubble.rs ya compila stub no-Windows. Port
   real Wayland (winit LayerShell, with_active/unfocus) es Parte 3.
5. **build.rs / winresource (.ico + manifest).** Fuera de la parte 4 por
   diseño (Parte 3: recursos de build); en Windows deben conservar el
   recurso incrustado; en no-Windows winresource se salta (build.rs a
   revisar en su parte).
