//! Build script: incrusta el `.ico` multirresolución en el `.exe`.
//!
//! Pasos en Windows:
//! 1. Si `assets/whispr-open.ico` no existe (clonar el repo en CI fresca),
//!    ejecuta `assets/build_icon.py` para regenerarlo. Si existe, lo usa
//!    tal cual (así el `cargo build` no invoca Python cada vez y el build
//!    sigue siendo determinista: el `.ico` versionado es siempre válido).
//! 2. Usa el crate `winresource` para incrustar el `.ico` como recurso
//!    RT_ICON (lo ven el Explorador, los accesos directos, la barra de
//!    tareas y el menú del shell).
//! 3. NO incrustamos un manifest propio: el compilador MSVC de Rust
//!    genera el manifest side-by-side del CRT (vcruntime140_app.dll,
//!    vcruntime140_1_app.dll) automáticamente. Sobreescribirlo con un
//!    manifest que omita esa dependencia rompe el arranque del binario
//!    con error SxS 14001 (verificado el 11/09/2026).
//!
//! En otros targets (cross-compile a Linux para CI rápida), no hace nada:
//! el icono solo importa en Windows.

use std::path::PathBuf;
use std::process::Command;

#[cfg(windows)]
fn maybe_regenerate_icon(repo_ico: &std::path::Path) {
    if repo_ico.exists() {
        return;
    }
    let script = PathBuf::from("assets").join("build_icon.py");
    let script_abs = script
        .canonicalize()
        .unwrap_or_else(|_| std::env::current_dir().unwrap().join(&script));

    // Probamos `python`, `py` y `python3` por orden (Windows suele tener
    // `python` o `py`, rara vez `python3`).
    for candidate in ["python", "py", "python3"] {
        let probe = Command::new(candidate).arg("-V").output();
        if let Ok(out) = probe {
            if !out.status.success() {
                continue;
            }
            let status = Command::new(candidate)
                .arg(&script_abs)
                .arg("assets")
                .status()
                .unwrap_or_else(|e| panic!("no se pudo relanzar {candidate}: {e}"));
            assert!(
                status.success(),
                "{candidate} {} terminó con código {status}",
                script_abs.display()
            );
            return;
        }
    }
    panic!(
        "no se encontró un Python ejecutable para generar el .ico (probados: \
         python, py, python3). Instálalo o genera el .ico a mano con `python \
         assets/build_icon.py assets`."
    );
}

#[cfg(windows)]
fn main() {
    let repo_ico = PathBuf::from("assets").join("whispr-open.ico");
    maybe_regenerate_icon(&repo_ico);
    assert!(
        repo_ico.exists(),
        "no se encontró {}; ejecuta `python assets/build_icon.py assets`",
        repo_ico.display()
    );

    let mut res = winresource::WindowsResource::new();
    res.set_icon(repo_ico.to_str().unwrap());
    if let Err(e) = res.compile() {
        panic!("winresource falló: {e}");
    }

    // Re-compila si cambia el script (que regenera el .ico) o el propio
    // .ico versionado.
    println!("cargo:rerun-if-changed=assets/build_icon.py");
    println!("cargo:rerun-if-changed=assets/whispr-open.ico");
}

#[cfg(not(windows))]
fn main() {
    // No hacemos nada: el icono solo importa en Windows.
    println!("cargo:rerun-if-changed=assets/build_icon.py");
}