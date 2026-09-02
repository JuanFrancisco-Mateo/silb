//! Whispr Open — dictado local de escritorio nativo.

use whispr_open::app::WhisprApp;

fn main() -> iced::Result {
    iced::application("Whispr Open — Dictado local", WhisprApp::update, WhisprApp::view)
        .theme(WhisprApp::theme)
        .subscription(WhisprApp::subscription)
        .window_size(iced::Size::new(780.0, 640.0))
        .run_with(WhisprApp::new)
}