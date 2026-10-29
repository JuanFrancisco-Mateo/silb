# PORT-LINUX / SCRIPTS — lo que cambia respecto a los de Windows

## Lo que ya existia (solo Windows, se queda como esta)

| Script | Funcion |
|---|---|
| scripts/check-arp.bat | Inspector de entradas ARP/registro de la instalacion Windows. |
| scripts/dumpbin-headers.bat | Inspeccion MSVC dumpbin del .exe. |
| scripts/extract-icon.bat | Extrae icono .ico del binario (requiere Windows). |
| scripts/install-test.bat | Ejecuta el instalador en modo silencioso (Windows). |
| scripts/measure-ram.bat | RAM del proceso .exe (Windows). |
| scripts/measure-release-profile.sh | Medidor del impacto de flags de release; usa cargo de WSL (ruta /mnt/c). Intocable: utilitat Windows-specific, no bloquea el port. |
| scripts/audit-doc-comments.py, install-smoke.py, verify-ico.py | Python, agnósticos de SO (verify-ico valida .ico: solo relevante en Windows). |

## Scripts nuevos Linux (Parte 4)

### scripts/build-linux.sh

- Para qué: `cargo build --release` con whisper-rs de CPU (CUDA APAGADO, que es
  la configuración del Cargo.toml en Linux tal como el `cfg(windows)` la anula).
- Comprueba en orden cargo/rustc/cmake/cc y se para con un mensaje claro si falta
  algo (nunca intenta instalar: dentro del script no hay sudo).
- `--cuda` se para con exit 2: activar CUDA en Linux no entra en esta parte; ver
  DEPENDENCIAS.md §4.
- COMPROBADO: la sintaxis pasa `bash -n`. NO se ha ejecutado entero (regla de la
  tarea: nada de `cargo build` en este portátil de 14 GiB).

### scripts/run-linux.sh

- Envuelve el binario `target/release/silb` con `WINIT_UNIX_BACKEND=wayland`
  exportado y `exec`.
- Se para con un mensaje claro si no hay binario («Ejecuta antes
  build-linux.sh»).
- OJO: el binario se llama **`silb`** desde el cambio de nombre (`Cargo.toml` →
  `[[bin]] name = "silb"`); los scripts apuntaban a `whispr-open` y ejecutaban un
  binario viejo. Corregido en build-linux.sh y run-linux.sh.

### scripts/linux-check.sh

- Comprueba todo de una pasada, pensado para lanzarlo por SSH en el portátil:
  `ssh omarchy 'bash ~/silb/scripts/linux-check.sh'`.
- Pasos: `cargo check` → `cargo check --tests` → `cargo test` →
  `cargo build --release` → medidas (tamaño del binario + `ldd`).
- Escribe `docs/VERIFICACION-LINUX-<fecha>.txt` y sale con el número de fallos.
  `SKIP_BUILD=1` se salta la compilación (para iterar solo con los tests).
- El `cargo test` es el dato que faltaba: en WSL con rustc 1.94 el compilador
  petaba (ICE); en el portátil (Arch) se verá si el toolchain lo pasa.


## Pendientes declarados

- Ejecutar los dos contra un cargo y un toolchain de Linux de verdad (hace falta
  instalar cargo/cmake, fuera de alcance: la regla de no usar sudo choca con el
  usuario del agente).
- Empaquetado nativo de Linux (PKGBUILD de AUR / flatpak / AppImage) NO
  empezado en esta parte: packaging/whispr-open.iss aquí es solo informativo y
  se queda intacto.
