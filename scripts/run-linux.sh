#!/usr/bin/env bash
# Lanza whispr-open ya construido en Linux (estilo run-linux) con el entorno
# de escritorio correcto. No construye: usa scripts/build-linux.sh antes.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN=target/release/whispr-open
[ -x "$BIN" ] || { echo "Falta el binario. Ejecuta antes: bash scripts/build-linux.sh" >&2; exit 1; }

# Wayland: iced/winit ya usa Wayland por defecto vía XDG_SESSION_TYPE.
export WINIT_UNIX_BACKEND=wayland

exec "$BIN" "$@"
