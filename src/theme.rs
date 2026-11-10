use std::fmt;

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

/// The light sibling of [`oxide`]: warm paper background, same rust-orange accent.
pub fn oxide_light() -> Theme {
    Theme::custom(
        "Oxide Light".to_string(),
        Palette {
            background: color!(0xf5f3f0),
            text: color!(0x25262b),
            primary: color!(0xd9541e),
            success: color!(0x2f9e68),
            warning: color!(0xc98a14),
            danger: color!(0xd0443c),
        },
    )
}

/// One selectable theme.
pub struct Entry {
    pub name: &'static str,
    /// The theme that "toggle light/dark" switches to.
    pub opposite: &'static str,
    pub make: fn() -> Theme,
}

pub const ENTRIES: &[Entry] = &[
    Entry {
        name: "Oxide",
        opposite: "Oxide Light",
        make: oxide,
    },
    Entry {
        name: "Oxide Light",
        opposite: "Oxide",
        make: oxide_light,
    },
    Entry {
        name: "Dracula",
        opposite: "Oxide Light",
        make: || Theme::Dracula,
    },
    Entry {
        name: "Nord",
        opposite: "Oxide Light",
        make: || Theme::Nord,
    },
    Entry {
        name: "Tokyo Night",
        opposite: "Tokyo Night Light",
        make: || Theme::TokyoNight,
    },
    Entry {
        name: "Tokyo Night Light",
        opposite: "Tokyo Night",
        make: || Theme::TokyoNightLight,
    },
    Entry {
        name: "Catppuccin Mocha",
        opposite: "Catppuccin Latte",
        make: || Theme::CatppuccinMocha,
    },
    Entry {
        name: "Catppuccin Latte",
        opposite: "Catppuccin Mocha",
        make: || Theme::CatppuccinLatte,
    },
    Entry {
        name: "Gruvbox Dark",
        opposite: "Gruvbox Light",
        make: || Theme::GruvboxDark,
    },
    Entry {
        name: "Gruvbox Light",
        opposite: "Gruvbox Dark",
        make: || Theme::GruvboxLight,
    },
    Entry {
        name: "Solarized Dark",
        opposite: "Solarized Light",
        make: || Theme::SolarizedDark,
    },
    Entry {
        name: "Solarized Light",
        opposite: "Solarized Dark",
        make: || Theme::SolarizedLight,
    },
];

pub fn index_of(name: &str) -> Option<usize> {
    ENTRIES.iter().position(|e| e.name == name)
}

/// Index of the theme that "toggle light/dark" switches to.
pub fn opposite_of(index: usize) -> usize {
    ENTRIES
        .get(index)
        .and_then(|e| index_of(e.opposite))
        .unwrap_or(0)
}

/// A theme as shown in the picker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice(pub usize);

impl Choice {
    pub fn all() -> Vec<Choice> {
        (0..ENTRIES.len()).map(Choice).collect()
    }
}

impl fmt::Display for Choice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(ENTRIES.get(self.0).map_or("Oxide", |e| e.name))
    }
}
