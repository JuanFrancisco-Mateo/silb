#!/usr/bin/env bash
# Copia el server LLM (llama.cpp) al paquete: <repo>/vendor/llama.
#
#   bash packaging/vendor-llama.sh [--cuda] [SRC] [DEST]
#
#   SRC   instalación de llama.cpp (default: $LLAMA_SRC o D:/LlamaCPP)
#   DEST  destino (default: <repo>/vendor/llama, junto a vendor/README.md)
#
# Sin --cuda copia el pack BASE (CPU, ~44 MB): llama-server.exe + sus DLL
# hermanas. Con --cuda copia además el runtime CUDA a DEST/cuda (~656 MB:
# ggml-cuda.dll + cudart + cublas), que la app usa solo si hay driver NVIDIA.
#
# Idempotente: un fichero ya copiado con el MISMO tamaño y sha256 no se vuelve
# a copiar. Cada copia se verifica (tamaño + sha256) y si no cuadra, el script
# falla con código 1: un pack a medias produce un arranque roto del server.
#
# Uso: si DEST no es el definitivo, borra el fichero antes (un rename DEST no
# limpia restos de un pack anterior).
set -u

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC="${LLAMA_SRC:-D:/LlamaCPP}"
DEST="$REPO/vendor/llama"
CUDA=0
args=()
for a in "$@"; do
  case "$a" in
    --cuda) CUDA=1 ;;
    *) args+=("$a") ;;
  esac
done
[ "${#args[@]}" -ge 1 ] && SRC="${args[0]}"
[ "${#args[@]}" -ge 2 ] && DEST="${args[1]}"

if [ ! -f "$SRC/llama-server.exe" ]; then
  echo "ERROR: no hay llama-server.exe en «$SRC» (pasa la ruta del build de llama.cpp)" >&2
  exit 1
fi

# Pack base: el stub, sus DLL hermanas y los backends ggml (CPU).
BASE="llama-server.exe llama-server-impl.dll llama-common.dll llama.dll mtmd.dll
ggml.dll ggml-base.dll ggml-cpu.dll
ggml-cpu-alderlake.dll ggml-cpu-cannonlake.dll ggml-cpu-cascadelake.dll
ggml-cpu-cooperlake.dll ggml-cpu-haswell.dll ggml-cpu-icelake.dll
ggml-cpu-ivybridge.dll ggml-cpu-piledriver.dll ggml-cpu-sandybridge.dll
ggml-cpu-sapphirerapids.dll ggml-cpu-skylakex.dll ggml-cpu-sse42.dll
ggml-cpu-x64.dll ggml-cpu-zen4.dll ggml-rpc.dll
libomp140.x86_64.dll"
# Pack CUDA opcional: el backend GPU y su runtime (va en DEST/cuda).
CUDA_FILES="ggml-cuda.dll cudart64_13.dll cublas64_13.dll cublasLt64_13.dll"

fail=0
bytes=0

# hash() imprime el sha256 (vacio si no hay sha256sum en el sistema). Se lee del
# stdin a propósito: con la ruta como argumento, sha256sum escapa con «\» las
# rutas de Windows y el hash saldría con un prefijo. Verificado: `sha256sum < f`.
hash() { command -v sha256sum >/dev/null 2>&1 && sha256sum < "$1" | cut -d' ' -f1 || echo ""; }

copy_one() { # $1 = fichero origen, $2 = directorio destino
  local name dest size_a size_b
  name="$(basename "$1")"
  dest="$2/$name"
  if [ ! -f "$1" ]; then
    echo "FALTA  $name (no está en el origen)"
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
    local ha hb
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

echo "origen : $SRC"
echo "destino: $DEST"
mkdir -p "$DEST"
for f in $BASE; do copy_one "$SRC/$f" "$DEST"; done

if [ "$CUDA" = 1 ]; then
  mkdir -p "$DEST/cuda"
  for f in $CUDA_FILES; do copy_one "$SRC/$f" "$DEST/cuda"; done
fi

echo "---------------------------------------------"
echo "pack base : $(du -sm --exclude=cuda "$DEST" 2>/dev/null | cut -f1) MB en $DEST"
[ -d "$DEST/cuda" ] && echo "pack cuda : $(du -sm "$DEST/cuda" | cut -f1) MB en $DEST/cuda"
echo "total     : $(du -sm "$DEST" 2>/dev/null | cut -f1) MB"
if [ "$fail" != 0 ]; then
  echo "FALLO: el pack quedó incompleto; corrige el origen y repite" >&2
  exit 1
fi
echo "OK: el server viaja en el paquete (la app lo encuentra sola)"
