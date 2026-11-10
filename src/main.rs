#![windows_subsystem = "windows"]

mod app;
mod http;
mod model;
mod storage;
mod theme;
mod view;

use app::Oxide;
use iced::{window, Size};

/// The icon is compiled into the binary, so it works wherever the app is run from.
fn window_icon() -> Option<window::Icon> {
    let bytes = include_bytes!("../assets/oxide.ico");
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Ico)
        .map_err(|e| eprintln!("could not decode assets/oxide.ico: {e}"))
        .ok()?
        .to_rgba8();
    let (width, height) = image.dimensions();
    window::icon::from_rgba(image.into_raw(), width, height)
        .map_err(|e| eprintln!("could not create window icon: {e}"))
        .ok()
}

fn main() -> iced::Result {
    iced::application(Oxide::new, Oxide::update, Oxide::view)
        .title(Oxide::title)
        .theme(Oxide::theme)
        .subscription(Oxide::subscription)
        .window(window::Settings {
            size: Size::new(1280.0, 800.0),
            position: window::Position::Centered,
            icon: window_icon(),
            ..window::Settings::default()
        })
        .run()
}
