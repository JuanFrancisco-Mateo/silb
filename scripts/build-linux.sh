#!/usr/bin/env bash
# Construye Whispr Open en Linux (rama port-linux, Omarchy: Arch + Hyprland/Wayland).
#
# Sin CUDA por defecto: este build es CPU (whisper.cpp sin cublas). Para GPU
# NVIDIA (el equivalente a la config Windows de producción) hace falta el
# paquete oficial de CUDA de Arch extra + cmake; ver docs/PORT-LINUX/DEPENDENCIAS.md.
#
# Uso:  bash scripts/build-linux.sh [--cuda]
set -euo pipefail
cd "$(dirname "$0")/.."

CUDA=${1:-}
echo "── build-linux.sh: build release de whispr-open (CPU por defecto) ──"

# Comprobación barata de toolchain (mismo espíritu que la de Windows en AGENTS.md)
for tool in cargo rustc cmake cc; do
  command -v "$tool" >/dev/null || {
    echo "FALTA: $tool. Instálalo con: sudo pacman -S --needed $tool" >&2
    exit 1
  }
done
rustc -vV | head -3

# Cargo deudas del port: CUDA solo se toma si el usuario lo pide explícito.
if [ "$CUDA" = "--cuda" ]; then
  # whisper-rs still compiles whisper.cpp with CUDA when cublas is found.
  # This project pins CPU-by-default; CUDA build requires manual CMAKE flags:
  # cargo build --release --features cuda  (feature still TODO, ver doc)
  echo "AVISO: build con CUDA en Linux aún no está cableada con feature de cargo;" >&2
  echo "        sigue docs/PORT-LINUX/DEPENDENCIAS.md. Aborto para no crear un" >&2
  echo "        binario CPU con label engañoso." >&2
  exit 2
fi

cargo build --release
BIN=target/release/silb
ls -lh "$BIN"
echo "── binario listo: $BIN ──"
