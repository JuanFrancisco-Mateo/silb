#!/usr/bin/env bash
# Empaquetado de Silb en dos piezas: primero el pack CUDA (con su .sha256) y
# después el instalador online, que ya lleva dentro la URL y el hash del pack.
#
#   bash packaging/build-installers.sh [version]
#
# Requisitos (los produce el repo, no van a git):
#   packaging/build/silb-cpu.exe   cargo build --release
#   packaging/build/silb-cuda.exe  cargo build --release --features cuda
#   packaging/build/cuda12/        bash packaging/build-cuda-runtime.sh
#
# Sale en packaging/Output/:
#   silb-cuda-pack-<v>.exe + .sha256   el pack que se sube como asset del release
#   silb-<v>-setup.exe                 el instalador ligero (~16 MB)
# PACKURL=... sobreescribe la URL que se incrusta (por defecto, la del release).
set -euo pipefail

VERSION="${1:-0.8.1}"
ISCC="${ISCC:-${LOCALAPPDATA:-$HOME/AppData/Local}/Programs/Inno Setup 6/ISCC.exe}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/packaging/Output"
PACK="silb-cuda-pack-$VERSION.exe"
PACKURL="${PACKURL:-https://github.com/JuanFrancisco-Mateo/silb/releases/download/v$VERSION/$PACK}"

cd "$ROOT"

echo "==> pack CUDA (tarda unos minutos: 465 MB con lzma2/ultra64)"
"$ISCC" '/DMyPack' "/DWHISPR_VERSION=$VERSION" packaging/whispr-open.iss
sha256sum "$OUT/$PACK" > "$OUT/$PACK.sha256"
SHA="$(cut -d' ' -f1 "$OUT/$PACK.sha256")"

echo "==> instalador online con el pack incrustado"
"$ISCC" "/DWHISPR_VERSION=$VERSION" "/DPACKURL=$PACKURL" "/DPACKSHA256=$SHA" \
  packaging/whispr-open.iss

echo
echo "pack:       $(stat -c '%s' "$OUT/$PACK") B  sha256 $SHA"
echo "instalador: $(stat -c '%s' "$OUT/silb-$VERSION-setup.exe") B"
