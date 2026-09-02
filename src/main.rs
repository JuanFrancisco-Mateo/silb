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

    // Multi-ventana de iced 0.13: `iced_winit::Program` renderiza una vista y
    // un tema por window::Id. La ventana principal es la app completa; la
    // burbuja flotante (whispr_open::bubble) se abre al iniciar.
    let mut main_window = iced::window::Settings::default();
    main_window.size = iced::Size::new(780.0, 640.0);
    // El cierre lo gestiona la app (WindowCloseRequested → iced::exit): ver
    // nota en `bubble::bubble_settings`.
    main_window.exit_on_close_request = false;

    type App = WhisprApp;
    type Compositor = <iced::Renderer as iced_winit::graphics::compositor::Default>::Compositor;

    iced_winit::program::run::<App, Compositor>(
        iced_winit::Settings::default(),
        iced_winit::graphics::Settings::default(),
        Some(main_window),
        (),
    )
    .map_err(iced::Error::from)?;

    Ok(())
}