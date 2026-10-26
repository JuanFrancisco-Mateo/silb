# PORT-LINUX / DEPENDENCIAS — checklist del sistema para compilar y correr

Estado: port de trabajo. Todo lo aqui listado es DOCUMENTACION; NINGUN
COMANDO ha sido ejecutado por el agente (regla dura de la tarea: no sudo,
no installs). El paquete `pacman` solo se consulto en modo lectura
(`pacman -Q`) para el inventario de abajo.

## 1. Compilacion (build)

```
sudo pacman -S --needed base-devel cmake
```

- `base-devel` ya esta instalado en la maquina de dev (verificado con
  `pacman -Qs base-devel` → local/base-devel 1-2).
- `cmake` NO esta instalado (verificado `pacman -Q cmake` → no encontrado).
  Es obligatorio porque whisper-rs compila whisper.cpp (C/C++) con cmake.

Rust toolchain: `cargo`/`rustc` NO estaban en el PATH del shell del agente.
Instalar con rustup (https://rustup.rs) o `pacman -S rust`
(sin alternativa probada a 03/10/2026). No se ha instalado (regla sudo).

## 2. Runtime de la app (audio, portapapeles, bandeja, GTK)

```
sudo pacman -S --needed alsa-lib pipewire pulseaudio wl-clipboard gtk3 libappindicator-gtk3
```

Justificado en base al codigo del repo:

| Paquete | Por qué |
|---|---|
| alsa-lib | cpal 0.15 abre el mic via ALSA (src/audio.rs). |
| pipewire | Escritorio moderno Omarchy; cpal puede hablar con PW via ALSA layer. |
| pulseaudio (+pulsed-audio bridge de PW) | Fallback de cpal si un dispositivo solo expone pulse. |
| wl-clipboard | Puente para la inyección de texto alternativa (paste); inject.rs sigue en modo stub Linux (ver CODIGO-UI.md). |
| gtk3 | rfd 0.15 (file dialogs) usa el backend GTK nativo en Linux. |
| libappindicator-gtk3 | tray-icon crate (src/tray.rs) para bandeja con backend dbus/appindicator. |

## 3. LLVM / link-time-optimization

El [profile.release] usa lto=true (Cargo.toml:76) → liga necesaria en un
`cc`/`ld` nativo de Arch (parte de base-devel). Sin CUDA no hace falta
nada NVCC/paquete cublas.

## 4. CUDA (NO activo en Linux por defecto)

En Cargo.toml, whisper-rs va con `default-features = false` (CPU) en la
seccion [dependencies], y whisper-rs con `features = ["cuda"]` solo en
[target.'cfg(windows)'.dependencies] — es decir, el build Linux es CPU
por diseño y en Windows se mantiene la config original tal cual.

Si quisiéramos CUDA en Linux hace falta: `pacman -S cuda`, cmake y el
flag `--cuda` de scripts/build-linux.sh reactivado; ese cableado queda
explicitamente FUERA del alcance de esta parte (Parte 4). Ver
scripts/build-linux.sh --cuda: aborta con exit 2 y el detalle en pantalla.

## 5. Desktop (Wayland/Hyprland)

- La raiz es un session Wayland (XDG_SESSION_TYPE=wayland en Omarchy).
- iced 0.13/winit 0.30 usa Wayland nativo; sin dependencias extra mas alla
  de las librerías estándar de Hyprland (wm de sistema).
- run-linux.sh hace `export WINIT_UNIX_BACKEND=wayland` para forzarlo
  explicito (documentado como declarativo: winit moderno ya auto-elige
  Wayland si `WAYLAND_DISPLAY` existe).

## 6. Inventario verificado en la maquina host (03/10/2026)

| Paquete | Estado |
|---|---|
| alsa-lib 1.2.16.1-1 | ✅ |
| libpulse 17.0+r98+gb096704c0-1 | ✅ |
| pipewire 1:1.6.8-1 | ✅ |
| wl-clipboard 1:2.3.0-1 | ✅ |
| base-devel 1-2 | ✅ |
| cmake | ❌ falta |
| gtk3, libappindicator-gtk3 | ⚠️ no verificado (agenda de instalacion) |
