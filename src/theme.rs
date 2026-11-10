use iced::theme::Palette;
use iced::{color, Theme};

/// Oxide's dark theme: warm rust-orange accent on a neutral charcoal base.
pub fn oxide() -> Theme {
    Theme::custom(
        "Oxide".to_string(),
        Palette {
            background: color!(0x1a1b1f),
            text: color!(0xe4e4e7),
            primary: color!(0xe0652f),
            success: color!(0x4caf7d),
            warning: color!(0xe5a93c),
            danger: color!(0xe5534b),
        },
    )
}
