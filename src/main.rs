#![windows_subsystem = "windows"]
mod app;
mod http;
mod model;
mod theme;
mod view;

use app::Oxide;

fn main() -> iced::Result {
    iced::application(Oxide::new, Oxide::update, Oxide::view)
        .title(Oxide::title)
        .theme(Oxide::theme)
        .subscription(Oxide::subscription)
        .window_size((1280.0, 800.0))
        .centered()
        .run()
}
