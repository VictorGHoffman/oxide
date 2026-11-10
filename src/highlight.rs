//! Syntax highlighting for the body editor that also colours `{{variables}}`.
//!
//! It wraps iced's syntect highlighter (JSON, XML, ...) and paints the ranges of
//! every `{{name}}` in one colour when the variable is defined and another when it is not.

use std::ops::Range;

use iced::advanced::text::highlighter::{self, Format};
use iced::highlighter as syntax;
use iced::{Color, Font};

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub syntax: syntax::Settings,
    /// Names of the variables that are defined (enabled, in the collection or global).
    pub defined: Vec<String>,
    pub defined_color: Color,
    pub undefined_color: Color,
}

/// A colour and font for a range of text.
#[derive(Debug, Clone, Copy)]
pub struct Highlight {
    color: Option<Color>,
    font: Option<Font>,
}

pub fn to_format(highlight: &Highlight, _theme: &iced::Theme) -> Format<Font> {
    Format {
        color: highlight.color,
        font: highlight.font,
    }
}

pub struct Highlighter {
    inner: syntax::Highlighter,
    settings: Settings,
}

impl highlighter::Highlighter for Highlighter {
    type Settings = Settings;
    type Highlight = Highlight;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Highlight)>;

    fn new(settings: &Self::Settings) -> Self {
        Self {
            inner: syntax::Highlighter::new(&settings.syntax),
            settings: settings.clone(),
        }
    }

    fn update(&mut self, new_settings: &Self::Settings) {
        self.inner.update(&new_settings.syntax);
        self.settings = new_settings.clone();
    }

    fn change_line(&mut self, line: usize) {
        self.inner.change_line(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        let base: Vec<(Range<usize>, Highlight)> = self
            .inner
            .highlight_line(line)
            .map(|(range, h)| {
                (
                    range,
                    Highlight {
                        color: h.color(),
                        font: h.font(),
                    },
                )
            })
            .collect();

        let variables = variable_ranges(line, &self.settings);
        overlay(base, &variables).into_iter()
    }

    fn current_line(&self) -> usize {
        self.inner.current_line()
    }
}

/// The ranges of every `{{name}}` in `line`, with the colour for it.
fn variable_ranges(line: &str, settings: &Settings) -> Vec<(Range<usize>, Color)> {
    let mut out = Vec::new();
    let mut from = 0;

    while let Some(start) = line[from..].find("{{").map(|i| i + from) {
        let Some(end) = line[start + 2..].find("}}").map(|i| i + start + 2) else {
            break;
        };
        let name = line[start + 2..end].trim();
        let color = if settings.defined.iter().any(|n| n == name) {
            settings.defined_color
        } else {
            settings.undefined_color
        };
        out.push((start..end + 2, color));
        from = end + 2;
    }
    out
}

/// Puts the variable colours on top of the syntax colours.
fn overlay(
    base: Vec<(Range<usize>, Highlight)>,
    variables: &[(Range<usize>, Color)],
) -> Vec<(Range<usize>, Highlight)> {
    if variables.is_empty() {
        return base;
    }

    let mut out = Vec::with_capacity(base.len() + variables.len() * 2);
    for (range, highlight) in base {
        let mut position = range.start;

        for (var, color) in variables {
            if var.end <= position || var.start >= range.end {
                continue;
            }
            let start = var.start.max(position);
            let end = var.end.min(range.end);

            if start > position {
                out.push((position..start, highlight));
            }
            out.push((
                start..end,
                Highlight {
                    color: Some(*color),
                    font: highlight.font,
                },
            ));
            position = end;
        }
        if position < range.end {
            out.push((position..range.end, highlight));
        }
    }
    out
}
