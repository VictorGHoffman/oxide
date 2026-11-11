use std::fmt;

use iced::theme::Palette;
use iced::{color, Theme};

fn make(
    name: &str,
    background: iced::Color,
    text: iced::Color,
    primary: iced::Color,
    success: iced::Color,
    warning: iced::Color,
    danger: iced::Color,
) -> Theme {
    Theme::custom(
        name.to_string(),
        Palette {
            background,
            text,
            primary,
            success,
            warning,
            danger,
        },
    )
}

/// Oxide's dark theme: near-black neutral surfaces with a rust-orange accent.
pub fn oxide() -> Theme {
    make("Oxide", color!(0x131417), color!(0xe8e8ec), color!(0xee6a37), color!(0x4cb782), color!(0xe5a93c), color!(0xe5534b))
}

/// The light sibling of [`oxide`]: clean off-white surfaces, same accent.
pub fn oxide_light() -> Theme {
    make("Oxide Light", color!(0xfcfbfa), color!(0x1f2025), color!(0xdc5a26), color!(0x2f9e68), color!(0xc98a14), color!(0xd0443c))
}

fn dracula() -> Theme {
    make("Dracula", color!(0x1d1e28), color!(0xf0f0f5), color!(0xbd93f9), color!(0x50fa7b), color!(0xf1c40f), color!(0xff5555))
}

fn nord() -> Theme {
    make("Nord", color!(0x242933), color!(0xe5e9f0), color!(0x88c0d0), color!(0xa3be8c), color!(0xebcb8b), color!(0xbf616a))
}

fn tokyo_night() -> Theme {
    make("Tokyo Night", color!(0x15161e), color!(0xc0caf5), color!(0x7aa2f7), color!(0x9ece6a), color!(0xe0af68), color!(0xf7768e))
}

fn tokyo_night_light() -> Theme {
    make("Tokyo Night Light", color!(0xf4f5f9), color!(0x343b58), color!(0x34548a), color!(0x33635c), color!(0x8f5e15), color!(0x8c4351))
}

fn catppuccin_mocha() -> Theme {
    make("Catppuccin Mocha", color!(0x181825), color!(0xcdd6f4), color!(0xcba6f7), color!(0xa6e3a1), color!(0xf9e2af), color!(0xf38ba8))
}

fn catppuccin_latte() -> Theme {
    make("Catppuccin Latte", color!(0xf8f9fc), color!(0x4c4f69), color!(0x8839ef), color!(0x40a02b), color!(0xdf8e1d), color!(0xd20f39))
}

fn gruvbox_dark() -> Theme {
    make("Gruvbox Dark", color!(0x1b1d1e), color!(0xebdbb2), color!(0xfe8019), color!(0xb8bb26), color!(0xfabd2f), color!(0xfb4934))
}

fn gruvbox_light() -> Theme {
    make("Gruvbox Light", color!(0xfaf5dc), color!(0x3c3836), color!(0xaf3a03), color!(0x79740e), color!(0xb57614), color!(0x9d0006))
}

fn solarized_dark() -> Theme {
    make("Solarized Dark", color!(0x00232b), color!(0x93a1a1), color!(0x268bd2), color!(0x859900), color!(0xb58900), color!(0xdc322f))
}

fn solarized_light() -> Theme {
    make("Solarized Light", color!(0xfdf8ea), color!(0x586e75), color!(0x268bd2), color!(0x859900), color!(0xb58900), color!(0xdc322f))
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
        make: dracula,
    },
    Entry {
        name: "Nord",
        opposite: "Oxide Light",
        make: nord,
    },
    Entry {
        name: "Tokyo Night",
        opposite: "Tokyo Night Light",
        make: tokyo_night,
    },
    Entry {
        name: "Tokyo Night Light",
        opposite: "Tokyo Night",
        make: tokyo_night_light,
    },
    Entry {
        name: "Catppuccin Mocha",
        opposite: "Catppuccin Latte",
        make: catppuccin_mocha,
    },
    Entry {
        name: "Catppuccin Latte",
        opposite: "Catppuccin Mocha",
        make: catppuccin_latte,
    },
    Entry {
        name: "Gruvbox Dark",
        opposite: "Gruvbox Light",
        make: gruvbox_dark,
    },
    Entry {
        name: "Gruvbox Light",
        opposite: "Gruvbox Dark",
        make: gruvbox_light,
    },
    Entry {
        name: "Solarized Dark",
        opposite: "Solarized Light",
        make: solarized_dark,
    },
    Entry {
        name: "Solarized Light",
        opposite: "Solarized Dark",
        make: solarized_light,
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
