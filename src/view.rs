//! All widgets. Everything here is read-only over the application state.

use iced::font::{self, Font};
use iced::mouse::Interaction;
use iced::widget::{
    button, checkbox, column, container, mouse_area, opaque, pick_list, radio, row, scrollable,
    space, stack, text, text_editor, text_input, tooltip, Column, Row,
};
use iced::{color, Border, Color, Element, Fill, Length, Padding, Theme};

use crate::app::*;
use crate::http::{format_duration, format_size};
use crate::model::*;
use crate::theme;

// ───────────────────────────── palette helpers ─────────────────────────────

const BOLD: Font = Font {
    weight: font::Weight::Bold,
    ..Font::DEFAULT
};
const SEMIBOLD: Font = Font {
    weight: font::Weight::Semibold,
    ..Font::DEFAULT
};
const ITALIC: Font = Font {
    style: font::Style::Italic,
    ..Font::DEFAULT
};

fn method_color(method: Method) -> Color {
    match method {
        Method::Get => color!(0x4caf7d),
        Method::Post => color!(0xe5a93c),
        Method::Put => color!(0x4c8df6),
        Method::Patch => color!(0xa97cf0),
        Method::Delete => color!(0xe5534b),
        Method::Options => color!(0xe879b9),
        Method::Head => color!(0x3fb8c4),
    }
}

fn status_color(code: u16) -> Color {
    match code {
        200..=299 => color!(0x4caf7d),
        300..=399 => color!(0x4c8df6),
        400..=499 => color!(0xe5a93c),
        _ => color!(0xe5534b),
    }
}

fn muted(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().text.scale_alpha(0.55)),
    }
}

fn shortcut_label(keys: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    };
    keys.replace("Ctrl", modifier)
}

// ───────────────────────────── shape ─────────────────────────────

/// Corner radii used everywhere, so the whole UI stays consistently round.
const RADIUS_SM: f32 = 6.0;
const RADIUS_MD: f32 = 8.0;
const RADIUS_LG: f32 = 12.0;

fn input_style(theme: &Theme, status: text_input::Status) -> text_input::Style {
    let mut style = text_input::default(theme, status);
    style.border.radius = RADIUS_MD.into();
    style
}

fn editor_style(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let mut style = text_editor::default(theme, status);
    style.border.radius = RADIUS_MD.into();
    style
}

fn pick_style(theme: &Theme, status: pick_list::Status) -> pick_list::Style {
    let mut style = pick_list::default(theme, status);
    style.border.radius = RADIUS_MD.into();
    style
}

fn checkbox_style(theme: &Theme, status: checkbox::Status) -> checkbox::Style {
    let mut style = checkbox::primary(theme, status);
    style.border.radius = 4.0.into();
    style
}

fn primary_button(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::primary(theme, status);
    style.border.radius = RADIUS_MD.into();
    style
}

fn secondary_button(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::secondary(theme, status);
    style.border.radius = RADIUS_MD.into();
    style
}

fn danger_button(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = button::danger(theme, status);
    style.border.radius = RADIUS_MD.into();
    style
}

// ───────────────────────────── container / button styles ─────────────────────────────

/// A rounded panel floating on the window background.
fn panel_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.weaker.color.into()),
        border: Border {
            color: p.background.strong.color.scale_alpha(0.55),
            width: 1.0,
            radius: RADIUS_LG.into(),
        },
        ..container::Style::default()
    }
}

fn sidebar_style(theme: &Theme) -> container::Style {
    panel_style(theme)
}

fn divider_style(theme: &Theme) -> container::Style {
    container::Style::default().background(theme.extended_palette().background.strong.color)
}

fn row_style(selected: bool, hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = theme.extended_palette();
        let background = if selected {
            Some(p.primary.base.color.scale_alpha(0.20))
        } else if hovered {
            Some(p.background.weak.color.scale_alpha(0.7))
        } else {
            None
        };
        container::Style {
            background: background.map(Into::into),
            border: Border {
                color: if selected {
                    p.primary.base.color.scale_alpha(0.45)
                } else {
                    Color::TRANSPARENT
                },
                width: 1.0,
                radius: RADIUS_MD.into(),
            },
            ..container::Style::default()
        }
    }
}

fn card_style(theme: &Theme) -> container::Style {
    let p = theme.extended_palette();
    container::Style {
        background: Some(p.background.weak.color.into()),
        border: Border {
            color: p.background.strong.color,
            width: 1.0,
            radius: RADIUS_LG.into(),
        },
        ..container::Style::default()
    }
}

fn menu_item_style(danger: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let p = theme.extended_palette();
        let base_text = if danger {
            p.danger.base.color
        } else {
            theme.palette().text
        };
        match status {
            button::Status::Hovered | button::Status::Pressed => button::Style {
                background: Some(
                    if danger {
                        p.danger.base.color
                    } else {
                        p.primary.base.color
                    }
                    .into(),
                ),
                text_color: if danger {
                    p.danger.base.text
                } else {
                    p.primary.base.text
                },
                border: Border {
                    radius: RADIUS_SM.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            },
            button::Status::Disabled => button::Style {
                text_color: base_text.scale_alpha(0.35),
                ..button::Style::default()
            },
            button::Status::Active => button::Style {
                text_color: base_text,
                ..button::Style::default()
            },
        }
    }
}

fn pill_style(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let p = theme.extended_palette();
        let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
        let background = if active {
            Some(p.background.strong.color)
        } else if hovered {
            Some(p.background.weak.color)
        } else {
            None
        };
        button::Style {
            background: background.map(Into::into),
            text_color: if active {
                theme.palette().text
            } else {
                theme.palette().text.scale_alpha(0.65)
            },
            border: Border {
                radius: RADIUS_SM.into(),
                ..Border::default()
            },
            ..button::Style::default()
        }
    }
}

fn icon_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let p = theme.extended_palette();
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: hovered.then(|| p.background.strong.color.into()),
        text_color: theme
            .palette()
            .text
            .scale_alpha(if hovered { 1.0 } else { 0.6 }),
        border: Border {
            radius: RADIUS_SM.into(),
            ..Border::default()
        },
        ..button::Style::default()
    }
}

fn tab_style(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = theme.extended_palette();
        container::Style {
            background: Some(
                if active {
                    p.background.weaker.color
                } else {
                    p.background.base.color
                }
                .into(),
            ),
            border: Border {
                color: if active {
                    theme.palette().primary.scale_alpha(0.7)
                } else {
                    p.background.strong.color.scale_alpha(0.45)
                },
                width: 1.0,
                radius: RADIUS_MD.into(),
            },
            ..container::Style::default()
        }
    }
}

// ───────────────────────────── small builders ─────────────────────────────

/// Key bindings for a code editor: Tab indents and Shift+Tab takes the indentation back
/// (instead of moving the focus away); every other key behaves as usual.
fn tab_indents(
    indent: Edit,
    unindent: Edit,
) -> impl Fn(text_editor::KeyPress) -> Option<text_editor::Binding<Message>> {
    move |key_press| {
        let is_tab = matches!(
            key_press.key.as_ref(),
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Tab)
        );
        let focused = matches!(key_press.status, text_editor::Status::Focused { .. });

        if is_tab && focused {
            let edit = if key_press.modifiers.shift() {
                unindent.clone()
            } else {
                indent.clone()
            };
            Some(text_editor::Binding::Custom(Message::Edit(edit)))
        } else {
            text_editor::Binding::from_key_press(key_press)
        }
    }
}

/// A "+" drawn from two bars, so it is exactly centred (a text glyph sits low).
fn plus_icon<'a>() -> Element<'a, Message> {
    let bar = |width: f32, height: f32| {
        container(space())
            .width(width)
            .height(height)
            .style(|_theme: &Theme| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    radius: 1.0.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
    };
    stack![
        container(bar(10.0, 2.0)).center(Fill),
        container(bar(2.0, 10.0)).center(Fill),
    ]
    .width(12)
    .height(12)
    .into()
}

/// Which part of a little window-layout icon is filled.
#[derive(Clone, Copy)]
enum Pane {
    Left,
    Right,
    Bottom,
}

/// A small "window with one pane highlighted" icon, drawn with plain containers.
fn pane_icon<'a>(pane: Pane) -> Element<'a, Message> {
    let ink = |theme: &Theme| theme.palette().text.scale_alpha(0.8);

    let filled = |width: Length, height: Length| {
        container(space())
            .width(width)
            .height(height)
            .style(move |theme: &Theme| container::Style {
                background: Some(ink(theme).into()),
                border: Border {
                    radius: 1.5.into(),
                    ..Border::default()
                },
                ..container::Style::default()
            })
    };

    let inner: Element<'a, Message> = match pane {
        Pane::Left => row![filled(Length::Fixed(5.0), Fill), space().width(Fill)]
            .spacing(2)
            .into(),
        Pane::Right => row![space().width(Fill), filled(Length::Fixed(5.0), Fill)]
            .spacing(2)
            .into(),
        Pane::Bottom => column![space().height(Fill), filled(Fill, Length::Fixed(4.0))]
            .spacing(2)
            .into(),
    };

    container(inner)
        .padding(2)
        .width(18)
        .height(14)
        .style(move |theme: &Theme| container::Style {
            border: Border {
                color: ink(theme),
                width: 1.5,
                radius: 4.0.into(),
            },
            ..container::Style::default()
        })
        .into()
}

/// An icon button with a tooltip.
fn icon_button<'a>(pane: Pane, label: &'static str, on_press: Message) -> Element<'a, Message> {
    tooltip(
        button(pane_icon(pane))
            .padding(Padding::from([5, 7]))
            .style(icon_button_style)
            .on_press(on_press),
        container(text(label).size(12))
            .padding(Padding::from([4, 8]))
            .style(card_style),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .into()
}

/// A draggable gap between two panels (the panels' own borders are the visible edge).
fn divider<'a>(which: Divider, vertical: bool) -> Element<'a, Message> {
    let (gap, cursor): (Element<'a, Message>, _) = if vertical {
        (
            container(space()).width(GRAB).height(Fill).into(),
            Interaction::ResizingHorizontally,
        )
    } else {
        (
            container(space()).width(Fill).height(GRAB).into(),
            Interaction::ResizingVertically,
        )
    };
    mouse_area(gap)
        .on_press(Message::DragStart(which))
        .interaction(cursor)
        .into()
}

/// Width (or height) of the area where a divider can be grabbed.
const GRAB: f32 = 8.0;

/// Does the node's name (or, for a request, its URL) contain `query` (already lowercase)?
fn node_matches(node: &Node, query: &str) -> bool {
    node.name.to_lowercase().contains(query)
        || node
            .request()
            .is_some_and(|r| r.url.to_lowercase().contains(query))
}

fn has_match_inside(node: &Node, query: &str) -> bool {
    node.children
        .iter()
        .any(|c| node_matches(c, query) || has_match_inside(c, query))
}

fn centered<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content).center(Fill).into()
}

fn labeled<'a>(
    label: &'static str,
    input: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    row![text(label).size(13).width(120).style(muted), input.into()]
        .spacing(8)
        .align_y(iced::Center)
        .into()
}

// ───────────────────────────── root view ─────────────────────────────

impl Oxide {
    pub fn view(&self) -> Element<'_, Message> {
        let body: Element<'_, Message> = if self.sidebar_visible {
            row![
                self.sidebar(),
                divider(Divider::Sidebar, true),
                self.main_area()
            ]
            .into()
        } else {
            row![
                self.collapsed_sidebar(),
                space().width(GRAB),
                self.main_area()
            ]
            .into()
        };
        let body = container(body).padding(8).width(Fill).height(Fill);

        let mut layers = stack![body];
        if let Some(menu) = &self.menu {
            layers = layers.push(self.menu_overlay(menu));
        }
        if let Some(scope) = self.vars_dialog {
            if let Some(dialog) = self.variables_dialog(scope) {
                layers = layers.push(dialog);
            }
        }
        if let Some(ids) = &self.confirm_delete {
            if let Some(dialog) = self.delete_dialog(ids) {
                layers = layers.push(dialog);
            }
        }
        layers.into()
    }

    // ───────────────────────── sidebar ─────────────────────────

    fn sidebar(&self) -> Element<'_, Message> {
        let header = row![
            text("Collections").size(15).font(BOLD),
            space().width(Fill),
            button(text("Import").size(12))
                .padding(Padding::from([4, 10]))
                .style(pill_style(false))
                .on_press(Message::MenuAction(MenuAction::ImportCollection)),
            button(container(plus_icon()).center(Fill))
                .width(28)
                .height(26)
                .padding(0)
                .style(primary_button)
                .on_press(Message::NewCollection),
            icon_button(Pane::Left, "Hide sidebar (Ctrl+B)", Message::ToggleSidebar),
        ]
        .spacing(6)
        .align_y(iced::Center)
        .padding(Padding::from([12, 12]).bottom(8.0));

        let query = self.search.trim().to_lowercase();
        let search_box = {
            let input = text_input("Search requests…", &self.search)
                .style(input_style)
                .id(SEARCH_ID)
                .on_input(Message::SearchChanged)
                .size(13)
                .padding(Padding::from([6, 10]));
            let mut line = row![input].spacing(4).align_y(iced::Center);
            if !self.search.is_empty() {
                line = line.push(
                    button(text("×").size(15))
                        .padding(Padding::from([0, 6]))
                        .style(icon_button_style)
                        .on_press(Message::SearchChanged(String::new())),
                );
            }
            container(line).padding(Padding::from([0, 10]).bottom(8.0))
        };

        let mut items: Vec<Element<'_, Message>> = Vec::new();
        if query.is_empty() {
            self.push_rows(&mut items, &self.tree.roots, 0);
        } else {
            self.push_matches(&mut items, &self.tree.roots, 0, &query, false);
        }
        let searching_without_results = !query.is_empty() && items.is_empty();
        let rows = Column::with_children(items)
            .spacing(1)
            .padding(Padding::from([0, 6]));

        let list: Element<'_, Message> = if searching_without_results {
            centered(text("No matching requests.").size(13).style(muted))
        } else if self.tree.roots.is_empty() {
            centered(
                column![
                    text("No collections yet").size(14).font(SEMIBOLD),
                    text("Create one, or import a Postman collection.")
                        .size(12)
                        .align_x(iced::Center)
                        .style(muted),
                    space().height(4),
                    button(text("New Collection").size(13).center())
                        .padding(Padding::from([7, 16]))
                        .style(primary_button)
                        .on_press(Message::NewCollection),
                ]
                .spacing(6)
                .align_x(iced::Center),
            )
        } else {
            scrollable(rows).height(Fill).into()
        };

        let mut sidebar = column![
            header,
            search_box,
            mouse_area(list).on_right_press(Message::SidebarContext)
        ];
        if let Some(error) = &self.save_error {
            sidebar = sidebar.push(
                container(
                    text(format!("Could not save: {error}"))
                        .size(12)
                        .color(color!(0xe5534b)),
                )
                .padding(10)
                .width(Fill),
            );
        }

        if let Some(notice) = &self.notice {
            let tint = if notice.error {
                color!(0xe5534b)
            } else {
                color!(0x4caf7d)
            };
            sidebar = sidebar.push(
                container(
                    row![
                        text(notice.text.clone()).size(12).color(tint).width(Fill),
                        button(text("×").size(14))
                            .padding(Padding::from([0, 6]))
                            .style(icon_button_style)
                            .on_press(Message::DismissNotice),
                    ]
                    .spacing(6),
                )
                .padding(10)
                .width(Fill),
            );
        }

        sidebar = sidebar.push(
            container(
                container(space())
                    .width(Fill)
                    .height(1)
                    .style(divider_style),
            )
            .padding(Padding::from([0, 12])),
        );
        sidebar = sidebar.push(self.theme_bar());

        let width = self.sidebar_width.clamp(
            MIN_SIDEBAR,
            (self.window_size.width - 320.0).clamp(MIN_SIDEBAR, MAX_SIDEBAR),
        );
        container(sidebar)
            .width(width)
            .height(Fill)
            .style(sidebar_style)
            .into()
    }

    /// Light/dark switch: a sun while the theme is dark, a moon while it is light.
    /// Drawn in the text colour only, so it follows the theme.
    fn theme_toggle(&self) -> Element<'_, Message> {
        let dark = self.theme().extended_palette().is_dark;
        let label = if dark {
            "Switch to a light theme"
        } else {
            "Switch to a dark theme"
        };

        let ink = |theme: &Theme| theme.palette().text.scale_alpha(0.8);
        // A filled circle in the text colour, or in the panel colour (to carve the moon).
        let disc = |size: f32, carve: bool| {
            container(space())
                .width(size)
                .height(size)
                .style(move |theme: &Theme| container::Style {
                    background: Some(
                        if carve {
                            theme.extended_palette().background.weaker.color
                        } else {
                            ink(theme)
                        }
                        .into(),
                    ),
                    border: Border {
                        radius: (size / 2.0).into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
        };
        let ray = |width: f32, height: f32| {
            container(space())
                .width(width)
                .height(height)
                .style(move |theme: &Theme| container::Style {
                    background: Some(ink(theme).into()),
                    border: Border {
                        radius: 1.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
        };

        let icon: Element<'_, Message> = if dark {
            // Sun: a disc with four rays.
            column![
                ray(2.0, 3.0),
                row![ray(3.0, 2.0), disc(8.0, false), ray(3.0, 2.0)]
                    .spacing(1)
                    .align_y(iced::Center),
                ray(2.0, 3.0),
            ]
            .spacing(1)
            .align_x(iced::Center)
            .into()
        } else {
            // Moon: a disc with a bite taken out of its top-right.
            stack![
                disc(15.0, false),
                container(disc(11.0, true))
                    .width(15)
                    .height(15)
                    .align_x(iced::alignment::Horizontal::Right)
                    .align_y(iced::alignment::Vertical::Top),
            ]
            .width(15)
            .height(15)
            .into()
        };

        tooltip(
            button(container(icon).width(16).height(16).center(16))
                .padding(Padding::from([4, 6]))
                .style(|_theme: &Theme, _status| button::Style::default())
                .on_press(Message::ToggleTheme),
            container(text(label).size(12))
                .padding(Padding::from([4, 8]))
                .style(card_style),
            tooltip::Position::Top,
        )
        .gap(4)
        .into()
    }

    /// Theme picker and light/dark switch at the bottom of the sidebar.
    fn theme_bar(&self) -> Element<'_, Message> {
        container(
            row![
                button(text("Globals").size(12))
                    .padding(Padding::from([5, 10]))
                    .style(pill_style(false))
                    .on_press(Message::OpenVariables(VarScope::Global)),
                pick_list(
                    theme::Choice::all(),
                    Some(theme::Choice(self.theme_index)),
                    Message::SetTheme
                )
                .width(Fill)
                .padding(Padding::from([5, 10]))
                .text_size(12)
                .style(pick_style),
                self.theme_toggle(),
            ]
            .spacing(6)
            .align_y(iced::Center),
        )
        .padding(Padding::from([8, 10]))
        .width(Fill)
        .into()
    }

    /// The thin strip left when the sidebar is hidden, with a button to bring it back.
    fn collapsed_sidebar(&self) -> Element<'_, Message> {
        container(
            column![
                icon_button(Pane::Left, "Show sidebar (Ctrl+B)", Message::ToggleSidebar,),
                space().height(Fill),
                self.theme_toggle(),
            ]
            .align_x(iced::Center),
        )
        .padding(Padding::from([10, 4]))
        .width(34)
        .height(Fill)
        .style(sidebar_style)
        .into()
    }

    /// The rows left by the search: matching nodes plus the folders/collections leading to them.
    /// A container whose own name matches is shown with everything inside it.
    fn push_matches<'a>(
        &'a self,
        out: &mut Vec<Element<'a, Message>>,
        nodes: &'a [Node],
        depth: usize,
        query: &str,
        parent_matched: bool,
    ) {
        for node in nodes {
            let own = node_matches(node, query);
            if !(parent_matched || own || has_match_inside(node, query)) {
                continue;
            }
            out.push(self.node_row(node, depth));
            if node.is_container() {
                self.push_matches(out, &node.children, depth + 1, query, parent_matched || own);
            }
        }
    }

    fn push_rows<'a>(
        &'a self,
        out: &mut Vec<Element<'a, Message>>,
        nodes: &'a [Node],
        depth: usize,
    ) {
        for node in nodes {
            out.push(self.node_row(node, depth));
            if node.is_container() && node.expanded {
                self.push_rows(out, &node.children, depth + 1);
            }
        }
    }

    fn node_row<'a>(&'a self, node: &'a Node, depth: usize) -> Element<'a, Message> {
        let id = node.id;
        let selected = self.is_selected(id);
        let hovered = self.hovered == Some(id);

        let lead: Element<'a, Message> = match &node.kind {
            Kind::Request(req) => {
                let tint = method_color(req.method);
                container(
                    text(req.method.short())
                        .size(9)
                        .font(BOLD)
                        .color(tint)
                        .width(Fill)
                        .align_x(iced::Center),
                )
                .width(38)
                .padding(Padding::from([2, 0]))
                .style(move |_theme: &Theme| container::Style {
                    background: Some(tint.scale_alpha(0.14).into()),
                    border: Border {
                        radius: 5.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                })
                .into()
            }
            _ => text(if node.expanded { "▾" } else { "▸" })
                .size(12)
                .width(14)
                .style(muted)
                .into(),
        };

        let label: Element<'a, Message> = match self.renaming.as_ref().filter(|r| r.id == id) {
            Some(r) => text_input("Name", &r.text)
                .style(input_style)
                .id(RENAME_ID)
                .size(13)
                .padding(2)
                .on_input(Message::RenameChanged)
                .on_submit(Message::RenameSubmit)
                .into(),
            None => {
                let font = if matches!(node.kind, Kind::Collection) {
                    SEMIBOLD
                } else {
                    Font::DEFAULT
                };
                text(&node.name)
                    .size(13)
                    .font(font)
                    .width(Fill)
                    .wrapping(text::Wrapping::None)
                    .into()
            }
        };

        let dirty: Element<'a, Message> = if self.is_dirty(id) {
            text("●").size(9).color(color!(0xe0652f)).into()
        } else {
            space().into()
        };

        // Containers show how many requests they hold.
        let count: Element<'a, Message> = if node.is_container() {
            text(node.request_ids().len().to_string())
                .size(11)
                .style(muted)
                .into()
        } else {
            space().into()
        };

        let content = row![
            space().width(depth as f32 * 16.0),
            lead,
            label,
            count,
            dirty
        ]
        .spacing(8)
        .align_y(iced::Center)
        .padding(Padding::from([6, 10]));

        mouse_area(
            container(content)
                .width(Fill)
                .clip(true)
                .style(row_style(selected, hovered)),
        )
        .on_press(Message::SelectNode(id))
        .on_double_click(Message::OpenPinned(id))
        .on_right_press(Message::NodeContext(id))
        .on_enter(Message::HoverNode(Some(id)))
        .on_exit(Message::HoverNode(None))
        .into()
    }

    // ───────────────────────── context menu ─────────────────────────

    fn menu_overlay(&self, menu: &ContextMenu) -> Element<'_, Message> {
        enum Entry {
            Item(&'static str, Option<&'static str>, MenuAction, bool, bool),
            Separator,
        }
        use Entry::*;

        let entries: Vec<Entry> = match menu.target {
            MenuTarget::Node(_) => vec![
                Item(
                    "New Request",
                    Some("Ctrl+N"),
                    MenuAction::NewRequest,
                    true,
                    false,
                ),
                Item(
                    "New Folder",
                    Some("Ctrl+Shift+N"),
                    MenuAction::NewFolder,
                    true,
                    false,
                ),
                Separator,
                Item("Rename", Some("F2"), MenuAction::Rename, true, false),
                Item(
                    "Duplicate",
                    Some("Ctrl+D"),
                    MenuAction::Duplicate,
                    true,
                    false,
                ),
                Separator,
                Item(
                    "Import Collection…",
                    None,
                    MenuAction::ImportCollection,
                    true,
                    false,
                ),
                Item(
                    "Export Collection…",
                    None,
                    MenuAction::ExportCollection,
                    true,
                    false,
                ),
                Item("Variables…", None, MenuAction::EditVariables, true, false),
                Separator,
                Item("Delete", Some("Del"), MenuAction::Delete, true, true),
            ],
            MenuTarget::Empty => vec![
                Item(
                    "New Collection",
                    None,
                    MenuAction::NewCollection,
                    true,
                    false,
                ),
                Item(
                    "Import Collection…",
                    None,
                    MenuAction::ImportCollection,
                    true,
                    false,
                ),
            ],
            MenuTarget::Tab(_) => vec![
                Item("Close", Some("Ctrl+W"), MenuAction::CloseTab, true, false),
                Item(
                    "Close Others",
                    None,
                    MenuAction::CloseOthers,
                    self.tabs.len() > 1,
                    false,
                ),
                Item("Close All", None, MenuAction::CloseAll, true, false),
            ],
        };

        let width = 230.0_f32;
        let height: f32 = entries
            .iter()
            .map(|e| match e {
                Item(..) => 30.0,
                Separator => 9.0,
            })
            .sum::<f32>()
            + 12.0;

        let x = menu
            .position
            .x
            .min(self.window_size.width - width - 6.0)
            .max(0.0);
        let y = menu
            .position
            .y
            .min(self.window_size.height - height - 6.0)
            .max(0.0);

        let items = entries.into_iter().map(|entry| match entry {
            Item(label, shortcut, action, enabled, danger) => {
                let shortcut_text = shortcut.map(shortcut_label).unwrap_or_default();
                button(
                    row![
                        text(label).size(13).width(Fill),
                        text(shortcut_text).size(11).style(muted),
                    ]
                    .spacing(16)
                    .align_y(iced::Center),
                )
                .width(Fill)
                .padding(Padding::from([6, 10]))
                .style(menu_item_style(danger))
                .on_press_maybe(enabled.then_some(Message::MenuAction(action)))
                .into()
            }
            Separator => container(
                container(space())
                    .width(Fill)
                    .height(1)
                    .style(divider_style),
            )
            .padding(Padding::from([4, 6]))
            .into(),
        });

        let menu_box = opaque(
            container(Column::with_children(items).spacing(0))
                .padding(4)
                .width(width)
                .style(card_style),
        );

        let backdrop = opaque(
            mouse_area(space().width(Fill).height(Fill))
                .on_press(Message::CloseMenu)
                .on_right_press(Message::CloseMenu),
        );

        let positioned = column![space().height(y), row![space().width(x), menu_box]];

        stack![backdrop, positioned].into()
    }

    // ───────────────────────── delete confirmation ─────────────────────────

    fn delete_dialog(&self, ids: &[Id]) -> Option<Element<'_, Message>> {
        let plural = |n: usize| if n == 1 { "" } else { "s" };

        let (title, headline, detail) = if let [id] = ids {
            let node = self.tree.get(*id)?;
            let (what, detail) = match node.kind {
                Kind::Request(_) => ("request", String::from("This action cannot be undone.")),
                Kind::Folder | Kind::Collection => {
                    let count = node.request_ids().len();
                    let what = if matches!(node.kind, Kind::Folder) {
                        "folder"
                    } else {
                        "collection"
                    };
                    (
                        what,
                        format!(
                            "It contains {count} request{}. Everything inside will be deleted. This action cannot be undone.",
                            plural(count)
                        ),
                    )
                }
            };
            (
                format!("Delete {what}?"),
                format!("\"{}\" will be permanently deleted.", node.name),
                detail,
            )
        } else {
            let requests: usize = ids
                .iter()
                .filter_map(|id| self.tree.get(*id))
                .map(|n| n.request_ids().len())
                .sum();
            (
                format!("Delete {} items?", ids.len()),
                format!(
                    "The {} selected items will be permanently deleted.",
                    ids.len()
                ),
                format!(
                    "They contain {requests} request{} in total. This action cannot be undone.",
                    plural(requests)
                ),
            )
        };

        let card = container(
            column![
                text(title).size(17).font(BOLD),
                text(headline).size(13),
                text(detail).size(13).style(muted),
                space().height(6),
                row![
                    space().width(Fill),
                    button(text("Cancel").size(13).center())
                        .padding(Padding::from([7, 18]))
                        .style(secondary_button)
                        .on_press(Message::CancelDelete),
                    button(text("Delete").size(13).center().color(Color::WHITE))
                        .padding(Padding::from([7, 18]))
                        .style(danger_button)
                        .on_press(Message::ConfirmDelete),
                ]
                .spacing(8),
            ]
            .spacing(8),
        )
        .padding(20)
        .width(400)
        .style(card_style);

        let dim = opaque(
            mouse_area(container(space()).width(Fill).height(Fill).style(|_| {
                container::Style::default().background(Color::from_rgba(0.0, 0.0, 0.0, 0.55))
            }))
            .on_press(Message::CancelDelete)
            .on_right_press(Message::CancelDelete),
        );

        Some(stack![dim, container(opaque(card)).center(Fill)].into())
    }

    // ───────────────────────── main area ─────────────────────────

    fn main_area(&self) -> Element<'_, Message> {
        let Some(id) = self.active else {
            return self.empty_state();
        };
        let (Some(req), Some(rt)) = (self.tree.request(id), self.runtime.get(&id)) else {
            return self.empty_state();
        };

        let editor = container(
            column![
                self.url_bar(req, rt),
                self.section_bar(req, rt),
                container(self.section_content(req, rt)).height(Length::FillPortion(1)),
            ]
            .spacing(10)
            .padding(12),
        )
        .width(Fill)
        .height(Fill)
        .style(panel_style);

        // The icon shows where the response will go when clicked.
        let (pane, label) = if self.response_right {
            (Pane::Bottom, "Dock response at the bottom")
        } else {
            (Pane::Right, "Dock response on the right")
        };
        let response_bar = row![
            space().width(Fill),
            icon_button(pane, label, Message::ToggleResponseLayout),
        ]
        .padding(Padding::from([4, 12]));
        let response = column![
            response_bar,
            container(self.response_panel(rt))
                .padding(Padding::from([0, 12]).bottom(12.0))
                .height(Fill),
        ];
        let response = |width: Length, height: Length| {
            container(response)
                .width(width)
                .height(height)
                .style(panel_style)
        };

        let content: Element<'_, Message> = if self.response_right {
            let sidebar = if self.sidebar_visible {
                self.sidebar_width
            } else {
                34.0
            };
            let width = self.response_width.clamp(
                MIN_RESPONSE_WIDTH,
                (self.window_size.width - sidebar - 320.0).max(MIN_RESPONSE_WIDTH),
            );
            row![
                editor,
                divider(Divider::Response, true),
                response(Length::Fixed(width), Fill),
            ]
            .height(Fill)
            .into()
        } else {
            let height = self.response_height.clamp(
                MIN_RESPONSE_HEIGHT,
                (self.window_size.height - 280.0).max(MIN_RESPONSE_HEIGHT),
            );
            column![
                editor,
                divider(Divider::Response, false),
                response(Fill, Length::Fixed(height)),
            ]
            .height(Fill)
            .into()
        };

        column![self.tab_bar(), content]
            .spacing(8)
            .width(Fill)
            .height(Fill)
            .into()
    }

    fn empty_state(&self) -> Element<'_, Message> {
        let hint = |keys: &str, what: &'static str| -> Element<'_, Message> {
            row![
                text(shortcut_label(keys))
                    .size(13)
                    .width(130)
                    .font(SEMIBOLD),
                text(what).size(13).style(muted),
            ]
            .spacing(8)
            .width(260)
            .into()
        };

        container(centered(
            column![
                text("Oxide").size(48).font(BOLD).color(color!(0xe0652f)),
                text("Select a request on the left, or create one to get started.")
                    .size(14)
                    .style(muted),
                space().height(16),
                hint("Shift+Click", "Select a range"),
                hint("Ctrl+Click", "Add to selection"),
                hint("Ctrl+N", "New request"),
                hint("Ctrl+Shift+N", "New folder"),
                hint("Ctrl+S", "Save request"),
                hint("Ctrl+B", "Show / hide sidebar"),
                hint("Ctrl+F", "Search requests"),
                hint("Ctrl+D", "Duplicate"),
                hint("F2", "Rename"),
                hint("Del", "Delete"),
                hint("Ctrl+1…9", "Switch tab"),
                hint("Ctrl+W", "Close tab"),
            ]
            .spacing(6)
            .align_x(iced::Center),
        ))
        .width(Fill)
        .height(Fill)
        .style(panel_style)
        .into()
    }

    // ───────────────────────── tabs ─────────────────────────

    fn tab_bar(&self) -> Element<'_, Message> {
        let tabs = self.tabs.iter().filter_map(|tab| {
            let node = self.tree.get(tab.request_id)?;
            let req = node.request()?;
            let id = tab.request_id;
            let active = self.active == Some(id);

            let content = row![
                text(req.method.short())
                    .size(10)
                    .font(BOLD)
                    .color(method_color(req.method)),
                text(&node.name)
                    .size(13)
                    .width(Fill)
                    .font(if tab.pinned { Font::DEFAULT } else { ITALIC })
                    .wrapping(text::Wrapping::None),
                if self.is_dirty(id) {
                    Element::from(text("●").size(9).color(color!(0xe0652f)))
                } else {
                    Element::from(space())
                },
                button(text("×").size(15))
                    .padding(Padding::from([0, 5]))
                    .style(icon_button_style)
                    .on_press(Message::CloseTab(id)),
            ]
            .spacing(8)
            .align_y(iced::Center)
            .padding(Padding::from([5, 8]));

            let tab_view = container(content)
                .width(190)
                .clip(true)
                .style(tab_style(active));

            let dragging = self
                .tab_drag
                .as_ref()
                .is_some_and(|d| d.moved && d.id == id);
            let area = mouse_area(tab_view)
                .on_press(Message::TabPressed(id))
                .on_double_click(Message::PinTab(id))
                .on_middle_press(Message::CloseTab(id))
                .on_right_press(Message::TabContext(id));
            Some(if dragging {
                area.interaction(Interaction::Grabbing).into()
            } else {
                area.into()
            })
        });

        mouse_area(
            scrollable(Row::with_children(tabs).spacing(6))
                .id(TABS_ID)
                .direction(scrollable::Direction::Horizontal(
                    scrollable::Scrollbar::new().width(3).scroller_width(3),
                ))
                .width(Fill),
        )
        .on_scroll(Message::TabsScrolled)
        .into()
    }

    // ───────────────────────── request editor ─────────────────────────

    fn url_bar<'a>(&'a self, req: &'a Request, rt: &'a Runtime) -> Element<'a, Message> {
        let sending = matches!(rt.state, ResponseState::Sending);

        // The dropdown is tinted with the method's colour, like in the sidebar.
        let tint = method_color(req.method);
        let methods = pick_list(Method::ALL, Some(req.method), |m| {
            Message::Edit(Edit::Method(m))
        })
        .width(125)
        .padding(Padding::from([8, 12]))
        .font(BOLD)
        .style(move |theme: &Theme, status| {
            let mut style = pick_style(theme, status);
            style.text_color = tint;
            style.handle_color = tint;
            style.background = tint.scale_alpha(0.12).into();
            style.border.color = tint.scale_alpha(0.55);
            style
        })
        .menu_style(|theme: &Theme| {
            let mut style = iced::overlay::menu::default(theme);
            style.border.radius = RADIUS_MD.into();
            style
        });

        let url = text_input("Enter URL", &req.url)
            .style(input_style)
            .on_input(|s| Message::Edit(Edit::Url(s)))
            .on_submit(Message::Send)
            .padding(8);

        let action = if sending {
            button(text("Cancel").center())
                .style(danger_button)
                .on_press(Message::Cancel)
        } else {
            button(text("Send").center().color(Color::WHITE).font(SEMIBOLD))
                .style(primary_button)
                .on_press(Message::Send)
        }
        .width(90)
        .padding(8);

        row![methods, url, action]
            .spacing(8)
            .align_y(iced::Center)
            .into()
    }

    fn section_bar<'a>(&'a self, req: &'a Request, rt: &'a Runtime) -> Element<'a, Message> {
        let badge = |n: usize| {
            if n > 0 {
                format!(" ({n})")
            } else {
                String::new()
            }
        };

        let sections = [
            (
                Section::Params,
                format!("Query Params{}", badge(active_count(&req.params))),
            ),
            (
                Section::Body,
                format!(
                    "Body{}",
                    if req.body_kind == BodyKind::None {
                        ""
                    } else {
                        " •"
                    }
                ),
            ),
            (
                Section::Headers,
                format!("Headers{}", badge(active_count(&req.headers))),
            ),
            (
                Section::Cookies,
                format!("Cookies{}", badge(active_count(&req.cookies))),
            ),
            (
                Section::Auth,
                format!(
                    "Auth{}",
                    if req.auth_kind == AuthKind::None {
                        ""
                    } else {
                        " •"
                    }
                ),
            ),
            (
                Section::Script,
                format!(
                    "Script{}",
                    if req.script.trim().is_empty() {
                        ""
                    } else {
                        " •"
                    }
                ),
            ),
        ];

        let mut bar = Row::with_children(sections.into_iter().map(|(section, label)| {
            button(text(label).size(13))
                .padding(Padding::from([5, 12]))
                .style(pill_style(rt.section == section))
                .on_press(Message::SectionChanged(section))
                .into()
        }))
        .spacing(4)
        .align_y(iced::Center);

        // Shortcut to the variables of this request's collection.
        if let Some(root) = self
            .active
            .map(|id| self.tree.ancestors(id).last().copied().unwrap_or(id))
        {
            bar = bar.push(space().width(Fill)).push(
                button(text("Variables").size(12))
                    .padding(Padding::from([4, 10]))
                    .style(icon_button_style)
                    .on_press(Message::OpenVariables(VarScope::Collection(root))),
            );
        }
        bar.into()
    }

    fn section_content<'a>(&'a self, req: &'a Request, rt: &'a Runtime) -> Element<'a, Message> {
        match rt.section {
            Section::Params => kv_table(KvList::Params, &req.params, "Key", "Value"),
            Section::Headers => kv_table(KvList::Headers, &req.headers, "Header", "Value"),
            Section::Cookies => kv_table(KvList::Cookies, &req.cookies, "Cookie name", "Value"),
            Section::Body => self.body_section(req, rt),
            Section::Auth => self.auth_section(req),
            Section::Script => self.script_section(rt),
        }
    }

    fn body_section<'a>(&'a self, req: &'a Request, rt: &'a Runtime) -> Element<'a, Message> {
        let kinds = Row::with_children(BodyKind::ALL.iter().map(|&kind| {
            radio(kind.label(), kind, Some(req.body_kind), |k| {
                Message::Edit(Edit::BodyKind(k))
            })
            .size(14)
            .text_size(13)
            .spacing(6)
            .into()
        }))
        .spacing(18);

        let content: Element<'a, Message> = match req.body_kind {
            BodyKind::None => centered(
                text("This request does not have a body")
                    .size(13)
                    .style(muted),
            ),
            BodyKind::FormData => kv_table(KvList::FormData, &req.form_data, "Key", "Value"),
            BodyKind::UrlEncoded => kv_table(KvList::UrlEncoded, &req.url_encoded, "Key", "Value"),
            BodyKind::Json | BodyKind::Xml | BodyKind::Text => {
                let placeholder = match req.body_kind {
                    BodyKind::Json => "{\n  \"hello\": \"world\"\n}",
                    BodyKind::Xml => "<root>\n  <hello>world</hello>\n</root>",
                    _ => "Plain text body",
                };
                text_editor(&rt.body_editor)
                    .style(editor_style)
                    .placeholder(placeholder)
                    .on_action(|a| Message::Edit(Edit::BodyAction(a)))
                    .key_binding(tab_indents(Edit::BodyTab, Edit::BodyUntab))
                    .font(Font::MONOSPACE)
                    .size(13)
                    .padding(10)
                    .height(Fill)
                    .into()
            }
        };

        column![kinds, content].spacing(10).height(Fill).into()
    }

    fn script_section<'a>(&'a self, rt: &'a Runtime) -> Element<'a, Message> {
        let mono = |line: &'static str| text(line).size(12).font(Font::MONOSPACE).style(muted);

        let help = column![
            text("Runs after every response. One assignment per line; the values become variables.")
                .size(13)
                .style(muted),
            mono("token = body.data.access_token       (collection variable)"),
            mono("global.user = header.X-User-Id       (global variable)"),
            mono("auth = \"Bearer \" + body.token        (join text with +)"),
            mono("sources: body, body.a.b[0].c, header.Name, cookie.Name, status, time, \"text\", {{var}}"),
        ]
        .spacing(2);

        let editor = text_editor(&rt.script_editor)
            .style(editor_style)
            .placeholder("token = body.access_token")
            .on_action(|a| Message::Edit(Edit::ScriptAction(a)))
            .key_binding(tab_indents(Edit::ScriptTab, Edit::ScriptUntab))
            .font(Font::MONOSPACE)
            .size(13)
            .padding(10)
            .height(Fill);

        let mut content = column![help, editor].spacing(8).height(Fill);
        if !rt.script_output.is_empty() {
            let lines = rt.script_output.iter().map(|(is_error, line)| {
                if *is_error {
                    text(format!("error: {line}"))
                        .size(12)
                        .font(Font::MONOSPACE)
                        .color(color!(0xe5534b))
                        .into()
                } else {
                    text(format!("set {line}"))
                        .size(12)
                        .font(Font::MONOSPACE)
                        .color(color!(0x4caf7d))
                        .into()
                }
            });
            content = content.push(Column::with_children(lines).spacing(2));
        }
        content.into()
    }

    /// The editor for global or collection variables.
    fn variables_dialog(&self, scope: VarScope) -> Option<Element<'_, Message>> {
        let rows = self.variables(scope)?;
        let (title, hint) = match scope {
            VarScope::Global => (
                "Global variables".to_string(),
                "Available in every collection. A collection variable with the same name wins.",
            ),
            VarScope::Collection(id) => (
                format!("Variables — {}", self.tree.get(id)?.name),
                "Only for the requests of this collection. They win over global variables with the same name.",
            ),
        };

        let card = container(
            column![
                text(title).size(17).font(BOLD),
                text(hint).size(13).style(muted),
                text("Use {{name}} in the URL, params, headers, cookies, body and auth. A request's Script tab can fill them from a response.")
                    .size(12)
                    .style(muted),
                container(kv_table(KvList::Vars, rows, "Variable", "Value")).height(300),
                row![
                    space().width(Fill),
                    button(text("Close").size(13).center())
                        .padding(Padding::from([7, 18]))
                        .style(primary_button)
                        .on_press(Message::CloseVariables),
                ],
            ]
            .spacing(10),
        )
        .padding(20)
        .width(580)
        .style(card_style);

        let dim = opaque(
            mouse_area(container(space()).width(Fill).height(Fill).style(|_| {
                container::Style::default().background(Color::from_rgba(0.0, 0.0, 0.0, 0.55))
            }))
            .on_press(Message::CloseVariables)
            .on_right_press(Message::CloseVariables),
        );

        Some(stack![dim, container(opaque(card)).center(Fill)].into())
    }

    fn auth_section<'a>(&'a self, req: &'a Request) -> Element<'a, Message> {
        let kind = labeled(
            "Type",
            pick_list(AuthKind::ALL, Some(req.auth_kind), |k| {
                Message::Edit(Edit::AuthKind(k))
            })
            .width(200)
            .padding(6)
            .style(pick_style),
        );

        let fields: Element<'a, Message> = match req.auth_kind {
            AuthKind::None => text("This request does not use any authorization.")
                .size(13)
                .style(muted)
                .into(),
            AuthKind::Bearer => labeled(
                "Token",
                text_input("Token", &req.bearer_token)
                    .style(input_style)
                    .on_input(|s| Message::Edit(Edit::BearerToken(s)))
                    .padding(6),
            ),
            AuthKind::Basic => column![
                labeled(
                    "Username",
                    text_input("Username", &req.basic_user)
                        .style(input_style)
                        .on_input(|s| Message::Edit(Edit::BasicUser(s)))
                        .padding(6),
                ),
                labeled(
                    "Password",
                    text_input("Password", &req.basic_password)
                        .style(input_style)
                        .on_input(|s| Message::Edit(Edit::BasicPassword(s)))
                        .secure(true)
                        .padding(6),
                ),
            ]
            .spacing(8)
            .into(),
            AuthKind::ApiKey => column![
                labeled(
                    "Key",
                    text_input("Key", &req.api_key_name)
                        .style(input_style)
                        .on_input(|s| Message::Edit(Edit::ApiKeyName(s)))
                        .padding(6),
                ),
                labeled(
                    "Value",
                    text_input("Value", &req.api_key_value)
                        .style(input_style)
                        .on_input(|s| Message::Edit(Edit::ApiKeyValue(s)))
                        .padding(6),
                ),
                labeled(
                    "Add to",
                    pick_list(ApiKeyLocation::ALL, Some(req.api_key_in), |l| {
                        Message::Edit(Edit::ApiKeyIn(l))
                    })
                    .width(200)
                    .padding(6)
                    .style(pick_style),
                ),
            ]
            .spacing(8)
            .into(),
        };

        scrollable(
            column![kind, fields]
                .spacing(14)
                .padding(Padding::from([4, 0]))
                .max_width(640),
        )
        .height(Fill)
        .into()
    }

    // ───────────────────────── response ─────────────────────────

    fn response_panel<'a>(&'a self, rt: &'a Runtime) -> Element<'a, Message> {
        match &rt.state {
            ResponseState::Idle => centered(
                text("Enter the URL and click Send to get a response")
                    .size(13)
                    .style(muted),
            ),
            ResponseState::Sending => centered(text("Sending request…").size(14).style(muted)),
            ResponseState::Cancelled => centered(text("Request cancelled").size(14).style(muted)),
            ResponseState::Failed(error) => centered(
                column![
                    text("Could not send request")
                        .size(15)
                        .font(SEMIBOLD)
                        .color(color!(0xe5534b)),
                    text(error).size(13).style(muted),
                ]
                .spacing(6)
                .align_x(iced::Center),
            ),
            ResponseState::Done(resp) => self.response_done(rt, resp),
        }
    }

    fn response_done<'a>(
        &'a self,
        rt: &'a Runtime,
        resp: &'a crate::http::Response,
    ) -> Element<'a, Message> {
        let stat =
            |label: &'static str, value: String, color: Option<Color>| -> Element<'a, Message> {
                let value = text(value).size(13).font(SEMIBOLD);
                let value = match color {
                    Some(c) => value.color(c),
                    None => value,
                };
                row![text(label).size(13).style(muted), value]
                    .spacing(6)
                    .into()
            };

        let summary = row![
            text("Response").size(13).font(BOLD),
            space().width(8),
            stat(
                "Status:",
                format!("{} {}", resp.status, resp.status_text)
                    .trim()
                    .to_string(),
                Some(status_color(resp.status)),
            ),
            stat("Time:", format_duration(resp.elapsed), None),
            stat("Size:", format_size(resp.size), None),
        ]
        .spacing(18)
        .align_y(iced::Center);

        let tabs = [
            (ResponseTab::Body, "Body".to_string()),
            (
                ResponseTab::Headers,
                format!("Headers ({})", resp.headers.len()),
            ),
            (
                ResponseTab::Cookies,
                format!("Cookies ({})", resp.cookies.len()),
            ),
            (ResponseTab::Info, "Info".to_string()),
        ];
        let tab_row = Row::with_children(tabs.into_iter().map(|(tab, label)| {
            button(text(label).size(13))
                .padding(Padding::from([4, 10]))
                .style(pill_style(rt.response_tab == tab))
                .on_press(Message::ResponseTabChanged(tab))
                .into()
        }))
        .spacing(4);

        let tools: Element<'a, Message> = if rt.response_tab == ResponseTab::Body {
            row![
                button(text("Pretty").size(12))
                    .padding(Padding::from([3, 8]))
                    .style(pill_style(rt.body_view == BodyView::Pretty))
                    .on_press(Message::BodyViewChanged(BodyView::Pretty)),
                button(text("Raw").size(12))
                    .padding(Padding::from([3, 8]))
                    .style(pill_style(rt.body_view == BodyView::Raw))
                    .on_press(Message::BodyViewChanged(BodyView::Raw)),
                button(text("Copy").size(12))
                    .padding(Padding::from([3, 8]))
                    .style(pill_style(false))
                    .on_press(Message::CopyBody),
            ]
            .spacing(4)
            .into()
        } else {
            space().into()
        };

        let content: Element<'a, Message> = match rt.response_tab {
            ResponseTab::Body => {
                if resp.body_raw.is_empty() {
                    centered(text("This response has no body").size(13).style(muted))
                } else {
                    text_editor(&rt.response_editor)
                        .style(editor_style)
                        .on_action(Message::ResponseAction)
                        .font(Font::MONOSPACE)
                        .size(13)
                        .padding(10)
                        .height(Fill)
                        .into()
                }
            }
            ResponseTab::Headers => {
                let rows = resp.headers.iter().map(|(k, v)| info_row(k, v));
                scrollable(
                    Column::with_children(rows)
                        .spacing(6)
                        .padding(Padding::from([4, 8])),
                )
                .width(Fill)
                .height(Fill)
                .into()
            }
            ResponseTab::Cookies => {
                if resp.cookies.is_empty() {
                    centered(
                        text("This response did not set any cookies")
                            .size(13)
                            .style(muted),
                    )
                } else {
                    let rows = resp.cookies.iter().map(|c| {
                        column![
                            row![
                                text(&c.name).size(13).font(SEMIBOLD),
                                text(&c.value).size(13),
                            ]
                            .spacing(10),
                            text(&c.attributes).size(12).style(muted),
                        ]
                        .spacing(2)
                        .into()
                    });
                    scrollable(
                        Column::with_children(rows)
                            .spacing(10)
                            .padding(Padding::from([4, 8])),
                    )
                    .width(Fill)
                    .height(Fill)
                    .into()
                }
            }
            ResponseTab::Info => {
                let mut rows: Vec<Element<'a, Message>> = vec![
                    info_row("Request URL", &resp.final_url),
                    info_row("HTTP Version", &resp.version),
                    info_row(
                        "Remote Address",
                        resp.remote_addr.as_deref().unwrap_or("unknown"),
                    ),
                    info_row("Body Size", &format_size(resp.size)),
                ];
                for name in ["content-type", "content-encoding", "server", "date"] {
                    if let Some(value) = resp.header(name) {
                        rows.push(info_row(name, value));
                    }
                }
                if resp.is_binary {
                    rows.push(info_row("Note", "Body is not valid UTF-8 text"));
                }
                scrollable(
                    Column::with_children(rows)
                        .spacing(6)
                        .padding(Padding::from([4, 8])),
                )
                .width(Fill)
                .height(Fill)
                .into()
            }
        };

        column![
            summary,
            row![tab_row, space().width(Fill), tools].align_y(iced::Center),
            content,
        ]
        .spacing(8)
        .into()
    }
}

/// A `name   value` line used by the response headers/info tabs.
fn info_row<'a>(name: &str, value: &str) -> Element<'a, Message> {
    row![
        text(name.to_string()).size(13).width(170).style(muted),
        text(value.to_string()).size(13).width(Fill),
    ]
    .spacing(10)
    .into()
}

// ───────────────────────────── key/value table ─────────────────────────────

fn kv_table<'a>(
    list: KvList,
    rows: &'a [KeyValue],
    key_placeholder: &'static str,
    value_placeholder: &'static str,
) -> Element<'a, Message> {
    let header = row![
        space().width(24),
        text(key_placeholder)
            .size(12)
            .font(SEMIBOLD)
            .style(muted)
            .width(Fill),
        text(value_placeholder)
            .size(12)
            .font(SEMIBOLD)
            .style(muted)
            .width(Fill),
        space().width(30),
    ]
    .spacing(8);

    let body = rows.iter().enumerate().map(|(i, kv)| {
        let is_last = i + 1 == rows.len();

        let toggle: Element<'a, Message> = if is_last && kv.is_blank() {
            space().width(18).into()
        } else {
            checkbox(kv.enabled)
                .style(checkbox_style)
                .on_toggle(move |on| Message::Edit(Edit::KvToggle(list, i, on)))
                .into()
        };

        let remove: Element<'a, Message> = if is_last && kv.is_blank() {
            space().width(30).into()
        } else {
            button(text("×").size(15).center())
                .width(30)
                .padding(Padding::from([2, 0]))
                .style(icon_button_style)
                .on_press(Message::Edit(Edit::KvRemove(list, i)))
                .into()
        };

        row![
            toggle,
            text_input(key_placeholder, &kv.key)
                .style(input_style)
                .on_input(move |s| Message::Edit(Edit::KvKey(list, i, s)))
                .size(13)
                .padding(6),
            text_input(value_placeholder, &kv.value)
                .style(input_style)
                .on_input(move |s| Message::Edit(Edit::KvValue(list, i, s)))
                .size(13)
                .padding(6),
            remove,
        ]
        .spacing(8)
        .align_y(iced::Center)
        .into()
    });

    scrollable(
        column![header, Column::with_children(body).spacing(4)]
            .spacing(6)
            .padding(Padding::from([2, 4]).right(10)),
    )
    .height(Fill)
    .into()
}
