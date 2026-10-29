//! Silb — dictado local, una app de escritorio normal.

//! Es una app de ventana y nada más: sin consola de Windows (los logs van
//! a un archivo, no a una terminal). En debug (cargo run) los logs
//! también salen por stderr.
#![windows_subsystem = "windows"]

use whispr_open::app::WhisprApp;

fn main() -> iced::Result {
    whispr_open::log!(
        "main",
        "Silb {} iniciando",
        env!("CARGO_PKG_VERSION")
    );
    // H2 (QA 24/09/2026): la gente se perdía buscando el log. La ruta
    // se dice al arrancar (en consola y en el propio fichero) y también
    // sale en Ajustes → Avanzado, con botón «Abrir carpeta».
    whispr_open::log!(
        "main",
        "registro en: {} (rota a .1/.2 al pasar de 1 MB; trazas de depuración con WHISPR_DEBUG=1)",
        whispr_open::log::log_path().display()
    );

    // Solo puede haber una copia abierta: la segunda no registra el
    // atajo global (Windows lo negaría sin avisar), no abre otra
    // bandeja ni otra burbuja. Avisa con un cuadro del sistema y se
    // cierra sin dejar nada a medias.
    if !whispr_open::single_instance::acquire() {
        whispr_open::log!(
            "main",
            "ya hay otra instancia corriendo: salgo sin tocar atajo ni bandeja"
        );
        whispr_open::single_instance::notify_second_instance();
        return Ok(());
    }

    // Varias ventanas en iced 0.13: `iced_winit::Program` pinta una vista
    // y un tema por cada ventana. La ventana principal es la app entera; la
    // burbuja flotante (whispr_open::bubble) se abre al arrancar.
    let mut main_window = iced::window::Settings::default();
    // Ventana del diseño (la pantalla «Dictado» de Figma, 1200×820): barra lateral
    // de 204 px más la zona de trabajo. El ancho mínimo no baja de 1200: la
    // rejilla de Ajustes ocupa 916 px (rótulo 606 + hueco 20 +
    // control 250 + márgenes 20/20), así que con menos ancho las filas
    // se pisarían. De alto sí se puede encoger: Ajustes hace scroll
    // y el texto dictado se comprime.
    main_window.size = iced::Size::new(1200.0, 820.0);
    main_window.min_size = Some(iced::Size::new(1200.0, 700.0));
    // El marco lo dibuja la app (barra de 40 px con botones de
    // minimizar/maximizar/cerrar): sin barra del sistema. Se arrastra
    // con `window::drag` desde `Message::DragMainWindow` y la X propia
    // cierra pasando por `WindowCloseRequested` → bandeja.
    main_window.decorations = false;
    // El cierre lo lleva la app (`WindowCloseRequested` → `iced::exit`): ver
    // la nota en `bubble::bubble_settings`.
    main_window.exit_on_close_request = false;

    type App = WhisprApp;
    type Compositor = <iced::Renderer as iced_winit::graphics::compositor::Default>::Compositor;

    // Tipografía del diseño: Inter va DENTRO del binario (Regular/Medium/
    // SemiBold, licencia OFL) y se carga al arrancar. `default_font` = Inter
    // Regular hace que todo texto sin `.font()` propio (y los desplegables,
    // campos de texto, editores y menús) salga ya en Inter: sin esto, los pesos
    // Medium/SemiBold caían a una fuente cualquiera del sistema y salían
    // cajas `□` en las tildes (ver src/fonts.rs).
    let winit_settings = iced_winit::Settings {
        fonts: whispr_open::fonts::inter_font_sources(),
        ..iced_winit::Settings::default()
    };
    let graphics_settings = iced_winit::graphics::Settings {
        default_font: whispr_open::fonts::regular(),
        ..iced_winit::graphics::Settings::default()
    };

    iced_winit::program::run::<App, Compositor>(
        winit_settings,
        graphics_settings,
        Some(main_window),
        (),
    )
    .map_err(iced::Error::from)?;

    Ok(())
}
