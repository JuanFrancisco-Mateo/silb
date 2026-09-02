//! Whispr Open — dictado local de escritorio nativo.

use whispr_open::app::WhisprApp;

fn main() -> iced::Result {
    iced::application("Whispr Open — Dictado local", WhisprApp::update, WhisprApp::view)
        .theme(|_| iced::Theme::Dark)
        .subscription(WhisprApp::subscription)
        .window_size(iced::Size::new(720.0, 600.0))
        .run_with(WhisprApp::new)
}