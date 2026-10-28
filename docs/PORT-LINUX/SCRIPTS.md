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

- Proposito: `cargo build --release` con whisper-rs CPU (CUDA OFF, es la
  config de Cargo.toml en Linux tal cual el target cfg(windows) la anula).
- Comprueba en orden cargo/rustc/cmake/cc y falla con mensaje explicito
  si falta algo (no intenta instalar nunca: nada de sudo dentro del script).
- `--cuda` aborta con exit 2: el cableado CUDA Linux no es alcance de esta
  parte; ver DEPENDENCIAS.md §4.
- VERIFICADO: sintaxis `bash -n` ok. NO ejecutado completo (regla de la
  tarea: no cargo build en este portátil de 14 GiB).

### scripts/run-linux.sh

- Envuelve el binario `target/release/silb` con
  `WINIT_UNIX_BACKEND=wayland` exportado y `exec`.
- Falla explícito si no hay binario (mensaje: «Ejecuta antes
  build-linux.sh»).
- OJO: el binario se llama **`silb`** desde el renombrado (`Cargo.toml` → `[[bin]]
  name = "silb"`); los scripts apuntaban a `whispr-open` y ejecutaban un binario
  viejo. Corregido en build-linux.sh y run-linux.sh.

### scripts/linux-check.sh

- Verificación completa en una pasada, pensada para lanzarse por SSH en el
  portátil: `ssh omarchy 'bash ~/silb/scripts/linux-check.sh'`.
- Pasos: `cargo check` → `cargo check --tests` → `cargo test` →
  `cargo build --release` → medidas (tamaño del binario + `ldd`).
- Escribe `docs/VERIFICACION-LINUX-<fecha>.txt` y sale con el nº de fallos.
  `SKIP_BUILD=1` salta el build (para iterar solo con los tests).
- El `cargo test` es el dato que faltaba: en WSL con rustc 1.94 daba ICE del
  compilador; en el portátil (Arch) se verá si el toolchain lo pasa.


## Pendientes declarados

- Ejecutar ambos contra un toolchain realcargo de Linux (requiere instalar
  cargo/cmake, fuera de alcance: regla sudo contradicha con el usuario del
  agente).
- Packaging nativo Linux (PKGBUILD AUR / flatpak / AppImage) NO emprendido
  en esta parte: packaging/whispr-open.iss es solo informativo aqui y queda
  intacto.
