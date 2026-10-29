# PORT-LINUX / DEPENDENCIAS — checklist del sistema para compilar y correr

Estado: port en marcha. Todo lo que aparece aquí es DOCUMENTACIÓN; el agente NO
ha ejecutado NINGÚN COMANDO (regla dura de la tarea: nada de sudo, nada de
instalar). El paquete `pacman` solo se consultó en modo lectura (`pacman -Q`) para
el inventario de abajo.

## 1. Compilacion (build)

```
sudo pacman -S --needed base-devel cmake
```

- `base-devel` ya está instalado en la máquina de desarrollo (comprobado con
  `pacman -Qs base-devel` → local/base-devel 1-2).
- `cmake` NO está instalado (comprobado `pacman -Q cmake` → no encontrado). Es
  obligatorio porque whisper-rs compila whisper.cpp (C/C++) con cmake.

Rust: `cargo`/`rustc` NO estaban en el PATH del shell del agente. Se instalan con
rustup (https://rustup.rs) o con `pacman -S rust` (no hay otra alternativa
probada a 03/10/2026). No se ha instalado (regla de no usar sudo).

## 2. Runtime de la app (audio, portapapeles, bandeja, GTK)

```
sudo pacman -S --needed alsa-lib pipewire pulseaudio wl-clipboard gtk3 libappindicator-gtk3
```

Justificado con el código del repo:

| Paquete | Por qué |
|---|---|
| alsa-lib | cpal 0.15 abre el micro vía ALSA (src/audio.rs). |
| pipewire | Escritorio moderno Omarchy; cpal puede hablar con PW a través de la capa ALSA. |
| pulseaudio (+pulsed-audio bridge de PW) | Alternativa de cpal si un dispositivo solo se muestra por pulse. |
| wl-clipboard | Puente para la otra forma de escribir el texto (pegar); inject.rs sigue con el apaño de Linux (ver CODIGO-UI.md). |
| gtk3 | rfd 0.15 (diálogos de fichero) usa la parte nativa GTK en Linux. |
| libappindicator-gtk3 | el crate tray-icon (src/tray.rs) para la bandeja, con la parte dbus/appindicator. |

## 3. LLVM / link-time-optimization

El [profile.release] usa lto=true (Cargo.toml:76) → hace falta un `cc`/`ld`
nativo en Arch (viene con base-devel). Sin CUDA no hace falta nada de NVCC ni el
paquete cublas.

## 4. CUDA (NO activo en Linux por defecto)

En el Cargo.toml, whisper-rs va con `default-features = false` (CPU) en la
sección [dependencies], y whisper-rs con `features = ["cuda"]` solo en
[target.'cfg(windows)'.dependencies] — o sea, la compilación de Linux es de CPU
por diseño y en Windows se mantiene la configuración original tal cual.

Si quisiéramos CUDA en Linux haría falta: `pacman -S cuda`, cmake y reactivar la
opción `--cuda` de scripts/build-linux.sh; ese trabajo queda explícitamente FUERA
del alcance de esta parte (Parte 4). Ver scripts/build-linux.sh --cuda: se para
con exit 2 y el detalle en pantalla.

## 5. Desktop (Wayland/Hyprland)

- La sesión es Wayland (XDG_SESSION_TYPE=wayland en Omarchy).
- iced 0.13/winit 0.30 usa Wayland de forma nativa; no hace falta nada más allá
  de las librerías normales de Hyprland (el gestor de ventanas del sistema).
- run-linux.sh hace `export WINIT_UNIX_BACKEND=wayland` para forzarlo de forma
  explícita (está puesto por claridad: winit moderno ya elige Wayland solo si
  existe `WAYLAND_DISPLAY`).

## 6. Inventario verificado en la maquina host (03/10/2026)

| Paquete | Estado |
|---|---|
| alsa-lib 1.2.16.1-1 | ✅ |
| libpulse 17.0+r98+gb096704c0-1 | ✅ |
| pipewire 1:1.6.8-1 | ✅ |
| wl-clipboard 1:2.3.0-1 | ✅ |
| base-devel 1-2 | ✅ |
| cmake | ❌ falta |
| gtk3, libappindicator-gtk3 | ⚠️ no verificado (pendiente de instalar) |
