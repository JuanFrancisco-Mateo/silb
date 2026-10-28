#!/usr/bin/env bash
# linux-check.sh — verificación completa del port Linux de Silb, en una pasada.
#
# Se ejecuta EN el portátil (Arch/Omarchy: Hyprland + Wayland), sobre una copia
# del árbol de trabajo, no sobre el repo de Windows:
#
#   ssh omarchy 'bash ~/silb/scripts/linux-check.sh'
#
# Hace, en orden y parando al primer fallo de compilación:
#   1. cargo check            (código base)
#   2. cargo check --tests    (tests compilan)
#   3. cargo test             (suite; en rustc 1.94 del WSL daba ICE, aquí se ve
#                              si el toolchain del portátil lo pasa)
#   4. cargo build --release  (binario real)
#   5. Medidas: tamaño del binario + tamaño de las dependencias dinámicas
#      (ldd) para vigilar el peso del paquete Linux.
#
# Escribe el informe en docs/VERIFICACION-LINUX-<fecha>.txt y también por stdout.
# Variables: CARGO_BUILD_JOBS (default: núcleos), SKIP_BUILD=1 para saltar (4).
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

REPORT="docs/VERIFICACION-LINUX-$(date +%Y-%m-%d).txt"
FAIL=0

log() { echo "$@" | tee -a "$REPORT"; }
step() {
  local name="$1"; shift
  log ""
  log "── $name ──────────────────────────────────────────────"
  local t0=$SECONDS
  if "$@" >>"$REPORT" 2>&1; then
    log "OK   $name ($((SECONDS - t0))s)"
  else
    log "FALLO $name ($((SECONDS - t0))s) — ver detalle arriba"
    FAIL=$((FAIL + 1))
  fi
}

: >"$REPORT"
log "VERIFICACIÓN LINUX — Silb $(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
log "Fecha: $(date -Iseconds)"
log "Host : $(hostname) · $(uname -srm)"
log "Rust : $(rustc -vV | head -1)"
log "Sesión: ${XDG_SESSION_TYPE:-desconocida} · ${XDG_CURRENT_DESKTOP:-?}"
log "Commit: $(git rev-parse --short HEAD 2>/dev/null || echo '(sin git)')"

step "cargo check" cargo check --quiet
step "cargo check --tests" cargo check --tests --quiet
step "cargo test" cargo test --quiet
if [ "${SKIP_BUILD:-0}" != "1" ]; then
  step "cargo build --release" cargo build --release
fi

BIN=target/release/silb
log ""
log "── Medidas ────────────────────────────────────────────"
if [ -n "${BIN:-}" ] && [ -x "$BIN" ]; then
  log "binario        : $BIN $(du -h "$BIN" | cut -f1)"
  # Peso real del paquete = binario + las .so que NO vengan de un paquete del
  # sistema (las de /usr/lib las pone el usuario con pacman). ponytail: heurística
  # por ruta, suficiente para vigilar regresiones de peso.
  log "dependencias   :"
  ldd "$BIN" 2>/dev/null | awk '{print $1, $3}' | grep -v "^/" | sort -u | head -40 | tee -a "$REPORT"
else
  log "binario        : no encontrado (¿build saltado?)"
fi

log ""
if [ "$FAIL" -eq 0 ]; then
  log "RESULTADO: TODO OK"
else
  log "RESULTADO: $FAIL paso(s) con fallo — detalle en $REPORT"
fi
exit "$FAIL"
