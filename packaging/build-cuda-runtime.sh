#!/usr/bin/env bash
# Copia el runtime mínimo de CUDA 12 que enlaza silb-cuda.exe: packaging/build/cuda12.
#
#   bash packaging/build-cuda-runtime.sh [BIN] [DEST]
#
#   BIN   directorio con las DLL (default: $CUDA_PATH/bin, si no, el kit 12.9)
#   DEST  destino (default: <repo>/packaging/build/cuda12)
#
# SILB-CUDA.EXE IMPORTA SOLO DOS DLL (cudart64_12.dll 0,6 MB y cublas64_12.dll
# 102,5 MB), pero cublas64_12.dll importa ESTÁTICAMENTE cublasLt64_12.dll
# (668,7 MB). Comprobado con
#   llvm-objdump -p cublas64_12.dll | grep "DLL Name"
#     DLL Name: cublasLt64_12.dll
#     DLL Name: KERNEL32.dll
# y en vivo: sin cublasLt el proceso no llega a arrancar si no hay toolkit CUDA
# en el PATH. Las DLL van junto al .exe (el instalador offline las mete en
# {app}), así que el equipo que instala NO necesita el runtime CUDA 12.
#
# Idempotente: un fichero ya copiado con el MISMO tamaño y sha256 no se vuelve
# a copiar. Cada copia se verifica; si no cuadra o falta alguna DLL, sale 1.
# No va a git: /packaging/build/ está en .gitignore.
set -u

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${1:-}"
DEST="${2:-$REPO/packaging/build/cuda12}"

if [ -z "$BIN" ]; then
  if [ -n "${CUDA_PATH:-}" ] && [ -d "$CUDA_PATH/bin" ]; then
    BIN="$CUDA_PATH/bin"
  else
    BIN="C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.9/bin"
  fi
fi

# Las tres: las dos que pide silb-cuda.exe y cublasLt, que es import de cublas.
DLLS="cudart64_12.dll cublas64_12.dll cublasLt64_12.dll"

if [ ! -d "$BIN" ]; then
  echo "ERROR: no existe el directorio del kit CUDA «$BIN»" >&2
  exit 1
fi

fail=0
bytes=0

# hash() imprime el sha256 leyendo del stdin (con la ruta como argumento,
# sha256sum escapa los «\» de Windows y el hash sale con prefijo).
hash() { command -v sha256sum >/dev/null 2>&1 && sha256sum < "$1" | cut -d' ' -f1 || echo ""; }

copy_one() { # $1 = origen, $2 = directorio destino
  local name dest size_a size_b ha hb
  name="$(basename "$1")"
  dest="$2/$name"
  if [ ! -f "$1" ]; then
    echo "FALTA  $name (no está en $BIN)"
    fail=1
    return
  fi
  size_a=$(stat -c %s "$1")
  size_b=$(stat -c %s "$dest" 2>/dev/null || echo 0)
  if [ "$size_a" = "$size_b" ] && [ "$(hash "$1")" = "$(hash "$dest")" ]; then
    echo "igual  $name ($size_a B)"
  else
    cp -f "$1" "$dest" || { echo "ERROR  copiando $name"; fail=1; return; }
    size_b=$(stat -c %s "$dest")
    ha=$(hash "$1"); hb=$(hash "$dest")
    if [ "$size_a" != "$size_b" ] || { [ -n "$ha" ] && [ "$ha" != "$hb" ]; }; then
      echo "ERROR  $name copiado distinto del origen"
      fail=1
      return
    fi
    echo "copia  $name ($size_a B)"
  fi
  bytes=$((bytes + size_a))
}

echo "origen : $BIN"
echo "destino: $DEST"
mkdir -p "$DEST"
for f in $DLLS; do copy_one "$BIN/$f" "$DEST"; done

echo "---------------------------------------------"
echo "runtime: $bytes B ($((bytes / 1024 / 1024)) MB) en $DEST"
if [ "$fail" != 0 ]; then
  echo "FALLO: falta el runtime CUDA 12; el instalador offline no se puede compilar" >&2
  exit 1
fi
echo "OK: runtime CUDA 12 listo para el instalador offline (/DMyOffline)"
