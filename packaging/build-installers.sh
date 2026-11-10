#!/usr/bin/env bash
# Empaquetado de Silb en tres piezas: runtime CUDA (asset FIJO, se sube una sola
# vez), pack CUDA (solo el exe, ~50 MB por versión) y el instalador online, que ya
# lleva dentro las dos URLs y los dos hashes.
#
#   bash packaging/build-installers.sh [version]
#
# Requisitos (los produce el repo, no van a git):
#   packaging/build/silb-cpu.exe      cargo build --release
#   packaging/build/silb-vulkan.exe   cargo build --release --features vulkan
#   packaging/build/silb-cuda.exe     cargo build --release --features cuda
#   packaging/build/cuda12/           bash packaging/build-cuda-runtime.sh
#
# Sale en packaging/Output/:
#   silb-cuda-runtime-12.exe + .sha256  las 772 MB de DLL (asset fijo: solo se
#                                       recompila si falta; no cambia por versión)
#   silb-cuda-pack-<v>.exe + .sha256    solo el exe CUDA, lo que cambia por versión
#   silb-<v>-setup.exe                  el instalador normal (~21 MB: lleva CPU y
#                                       Vulkan dentro; CUDA llega como descarga)
# PACKURL=... sobreescribe la URL que se incrusta (por defecto, la del release).
set -euo pipefail

VERSION="${1:-0.9.0}"
ISCC="${ISCC:-${LOCALAPPDATA:-$HOME/AppData/Local}/Programs/Inno Setup 6/ISCC.exe}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/packaging/Output"
PACK="silb-cuda-pack-$VERSION.exe"
PACKURL="${PACKURL:-https://github.com/JuanFrancisco-Mateo/silb/releases/download/v$VERSION/$PACK}"

cd "$ROOT"

# El .iss también lo comprueba, pero mejor decirlo aquí que a mitad de ISCC.
for f in build/silb-cpu.exe build/silb-vulkan.exe build/silb-cuda.exe \
         build/cuda12/cudart64_12.dll build/cuda12/cublas64_12.dll build/cuda12/cublasLt64_12.dll; do
  [[ -f "packaging/$f" ]] || { echo "falta packaging/$f (ver requisitos en la cabecera)" >&2; exit 1; }
done
echo "==> insumos: cpu $(stat -c '%s' packaging/build/silb-cpu.exe) B" \
  "· vulkan $(stat -c '%s' packaging/build/silb-vulkan.exe) B" \
  "· cuda $(stat -c '%s' packaging/build/silb-cuda.exe) B"

# El runtime va en su propio release (cuda-runtime-12) y se sube UNA vez: las DLL no
# cambian entre versiones de Silb. Si ya está compilado se reutiliza tal cual.
RT="$OUT/silb-cuda-runtime-12.exe"
if [[ -f "$RT" ]]; then
  echo "==> runtime CUDA ya compilado, se reutiliza ($(stat -c '%s' "$RT") B)"
else
  echo "==> runtime CUDA (tarda unos minutos: 772 MB con lzma2/ultra64)"
  "$ISCC" '/DMyRuntime' packaging/silb.iss
fi
sha256sum "$RT" > "$RT.sha256"
RTSHA="$(cut -d' ' -f1 "$RT.sha256")"

echo "==> pack CUDA (solo el exe: las DLL las pone el runtime)"
"$ISCC" '/DMyPack' "/DWHISPR_VERSION=$VERSION" packaging/silb.iss
sha256sum "$OUT/$PACK" > "$OUT/$PACK.sha256"
SHA="$(cut -d' ' -f1 "$OUT/$PACK.sha256")"

echo "==> instalador online con los dos hashes incrustados"
"$ISCC" "/DWHISPR_VERSION=$VERSION" "/DPACKURL=$PACKURL" "/DPACKSHA256=$SHA" \
  "/DRUNTIMESHA256=$RTSHA" packaging/silb.iss

echo
echo "runtime:    $(stat -c '%s' "$RT") B  sha256 $RTSHA"
echo "pack:       $(stat -c '%s' "$OUT/$PACK") B  sha256 $SHA"
echo "instalador: $(stat -c '%s' "$OUT/silb-$VERSION-setup.exe") B"
