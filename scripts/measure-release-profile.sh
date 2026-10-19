#!/usr/bin/env bash
# Mide el efecto de cada flag del [profile.release] sobre el tamaño del
# binario. Compara contra la base (lto=true, strip=true) y reporta el
# delta en bytes.
#
# Uso: bash scripts/measure-release-profile.sh
#
# Estrategia: aplica overrides de cargo vía --config usando rutas a
# ficheros temporales. Cada iteración borra solo el .exe para forzar
# relink (las deps sí quedan cacheadas).

cd "$(dirname "$0")/.."

CARGO="${CARGO:-cargo}"
EXE="target/release/silb.exe"

# Variables de salida.
S_BASE=""
S_CGU=""
S_OPTZ=""
S_PANIC=""
S_ALL=""

run_one() {
    local label="$1"
    local cgu="$2"
    local opt="$3"
    local pan="$4"
    local outvar="$5"
    local cfg_file
    cfg_file="$(mktemp -t cargo-profile.XXXXXX).toml"
    # Cargo se ejecuta desde Windows; necesita la ruta Windows del cfg.
    local cfg_file_win
    cfg_file_win="$(wslpath -w "$cfg_file" 2>/dev/null || echo "$cfg_file")"
    local opt_value
case "$opt" in
    z|s) opt_value="\"$opt\"";;
    *)   opt_value="$opt";;
esac
cat > "$cfg_file" <<EOF
[profile.release]
lto = true
strip = true
codegen-units = ${cgu}
opt-level = ${opt_value}
panic = "${pan}"
EOF
    echo
    echo "=== ${label} ==="
    cat "$cfg_file" | sed 's/^/    /'
    rm -f "$EXE"
    "$CARGO" build --release --config "$cfg_file_win" > /tmp/profile-build.log 2>&1
    local rc=$?
    if [ $rc -ne 0 ] || [ ! -f "$EXE" ]; then
        echo "    ERROR (rc=$rc); log tail:"
        tail -10 /tmp/profile-build.log | sed 's/^/    /'
        rm -f "$cfg_file"
        return 1
    fi
    local size
    size=$(stat -c %s "$EXE")
    rm -f "$cfg_file"
    echo "    exe: $size B"
    eval "$outvar=$size"
}

# Variante 0 (referencia, ya medida): base.
S_BASE=44854272
echo "=== base (lto=true, strip=true, cgu=16, opt-level=3, panic=unwind) ==="
echo "    exe: $S_BASE B  (medido directo en §8.1 del doc)"

# Variantes que cambian flags de compilación: requieren recompilar todo
# (~3-4 min cada una con lto=true). Medimos solo las dos más decisivas:
# opt-level=z (size) y panic=abort. cgu=1 con lto=true apenas cambia
# (lto ya hace cross-crate), se omite.
run_one "+ opt-level=z (size)" 16 z unwind S_OPTZ
run_one "+ panic=abort"         16 3 abort S_PANIC
run_one "TODO (cgu=1 + z + abort + strip)" 1 z abort S_ALL

echo
echo "=== Resumen (bytes / delta vs base) ==="
echo "  base                       : ${S_BASE} B (delta $((S_BASE - S_BASE)) B)"
echo "  +opt-level=z               : ${S_OPTZ} B (delta $((S_OPTZ - S_BASE)) B)"
echo "  +panic=abort               : ${S_PANIC} B (delta $((S_PANIC - S_BASE)) B)"
echo "  todo (cgu=1+z+abort)       : ${S_ALL} B (delta $((S_ALL - S_BASE)) B)"