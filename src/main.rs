//! Silb — dictado local de escritorio nativo.

//! App de ventana pura: sin consola de Windows (la app es GUI; los logs van
//! a archivo vía `log`, no a una terminal). En debug (cargo run) los logs
//! también se ven en stderr.
#![windows_subsystem = "windows"]

use whispr_open::app::WhisprApp;

fn main() -> iced::Result {
    whispr_open::log!(
        "main",
        "Silb {} iniciando",
        env!("CARGO_PKG_VERSION")
    );
    // H2 (QA 24/09/2026): el usuario se pierde buscando el log. La ruta
    // queda dicha al arrancar (en consola y en el propio fichero) y también
    // se muestra en Ajustes → Avanzado, con botón «Abrir carpeta».
    whispr_open::log!(
        "main",
        "registro en: {} (rota a .1/.2 al pasar de 1 MB; trazas de depuración con WHISPR_DEBUG=1)",
        whispr_open::log::log_path().display()
    );

    // Guard de instancia única (requisito de robustez): la segunda
    // ejecución no registra atajo global (Windows lo denegaría a ciegas),
    // no abre una segunda bandeja ni una segunda burbuja. Avisa con un
    // cuadro nativo y sale limpia.
    if !whispr_open::single_instance::acquire() {
        whispr_open::log!(
            "main",
            "ya hay otra instancia corriendo: salgo sin tocar atajo ni bandeja"
        );
        whispr_open::single_instance::notify_second_instance();
        return Ok(());
    }

    // Multi-ventana de iced 0.13: `iced_winit::Program` renderiza una vista
    // y un tema por window::Id. La ventana principal es la app completa; la
    // burbuja flotante (whispr_open::bubble) se abre al iniciar.
    let mut main_window = iced::window::Settings::default();
    // Ventana del diseño (frame «Dictado» de Figma, 1200×820): barra lateral de
    // 204 px + espacio de trabajo. El mínimo permite encogerla sin que la
    // tarjeta de transcripción quede inservible.
    main_window.size = iced::Size::new(1200.0, 820.0);
    main_window.min_size = Some(iced::Size::new(940.0, 640.0));
    // El marco lo dibuja la app (barra de ventana propia de 40 px con
    // controles de minimizar/maximizar/cerrar): sin decoración nativa. El
    // arrastre llega por `window::drag` desde `Message::DragMainWindow` y el
    // cierre por la X propia pasa por `WindowCloseRequested` → bandeja.
    main_window.decorations = false;
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
