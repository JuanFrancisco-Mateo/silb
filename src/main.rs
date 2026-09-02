//! Whispr Open — dictado local de escritorio nativo.

//! App de ventana pura: sin consola de Windows (la app es GUI; los logs van
//! a archivo vía `log`, no a una terminal). En debug (cargo run) los logs
//! también se ven en stderr.
#![windows_subsystem = "windows"]

use whispr_open::app::WhisprApp;

fn main() -> iced::Result {
    whispr_open::log!(
        "main",
        "Whispr Open {} iniciando",
        env!("CARGO_PKG_VERSION")
    );
    iced::application("Whispr Open — Dictado local", WhisprApp::update, WhisprApp::view)
        .theme(WhisprApp::theme)
        .subscription(WhisprApp::subscription)
        .window_size(iced::Size::new(780.0, 640.0))
        .run_with(WhisprApp::new)
}