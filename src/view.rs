//! All widgets. Everything here is read-only over the application state.

use iced::font::{self, Font};
use iced::widget::{
    button, checkbox, column, container, mouse_area, opaque, pick_list, radio, row, scrollable,
    space, stack, text, text_editor, text_input, Column, Row,
};
use iced::{color, Border, Color, Element, Fill, Length, Padding, Theme};

use crate::app::*;
use crate::http::{format_duration, format_size};
use crate::model::*;

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

// ───────────────────────────── container / button styles ─────────────────────────────

fn sidebar_style(theme: &Theme) -> container::Style {
    container::Style::default().background(theme.extended_palette().background.weaker.color)
}

fn divider_style(theme: &Theme) -> container::Style {
    container::Style::default().background(theme.extended_palette().background.strong.color)
}

fn row_style(selected: bool, hovered: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        let p = theme.extended_palette();
        let background = if selected {
            Some(p.primary.base.color.scale_alpha(0.28))
        } else if hovered {
            Some(p.background.weak.color)
        } else {
            None
        };
        container::Style {
            background: background.map(Into::into),
            border: Border {
                radius: 4.0.into(),
                ..Border::default()
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
            radius: 6.0.into(),
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
                    radius: 4.0.into(),
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
                radius: 4.0.into(),
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
            radius: 4.0.into(),
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
                    p.background.base.color
                } else {
                    p.background.weaker.color
                }
                .into(),
            ),
            border: Border {
                color: p.background.strong.color,
                width: 0.0,
                radius: 0.0.into(),
            },
            ..container::Style::default()
        }
    }
}

fn accent_bar(active: bool) -> impl Fn(&Theme) -> container::Style {
    move |theme| {
        container::Style::default().background(if active {
            theme.palette().primary
        } else {
            Color::TRANSPARENT
        })
    }
}

// ───────────────────────────── small builders ─────────────────────────────

fn hline<'a>() -> Element<'a, Message> {
    container(space())
        .width(Fill)
        .height(1)
        .style(divider_style)
        .into()
}

fn vline<'a>() -> Element<'a, Message> {
    container(space())
        .width(1)
        .height(Fill)
        .style(divider_style)
        .into()
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
        let body = row![self.sidebar(), vline(), self.main_area()];

        let mut layers = stack![body];
        if let Some(menu) = &self.menu {
            layers = layers.push(self.menu_overlay(menu));
        }
        if let Some(id) = self.confirm_delete {
            if let Some(dialog) = self.delete_dialog(id) {
                layers = layers.push(dialog);
            }
        }
        layers.into()
    }

    // ───────────────────────── sidebar ─────────────────────────

    fn sidebar(&self) -> Element<'_, Message> {
        let header = row![
            text("COLLECTIONS").size(11).font(BOLD).style(muted),
            space().width(Fill),
            button(text("+").size(16))
                .padding(Padding::from([0, 8]))
                .style(icon_button_style)
                .on_press(Message::NewCollection),
        ]
        .align_y(iced::Center)
        .padding(Padding::from([8, 12]));

        let mut items: Vec<Element<'_, Message>> = Vec::new();
        self.push_rows(&mut items, &self.tree.roots, 0);
        let rows = Column::with_children(items)
            .spacing(1)
            .padding(Padding::from([0, 6]));

        let list: Element<'_, Message> = if self.tree.roots.is_empty() {
            centered(
                text("No collections yet.\nClick + to create one.")
                    .size(13)
                    .align_x(iced::Center)
                    .style(muted),
            )
        } else {
            scrollable(rows).height(Fill).into()
        };

        let mut sidebar = column![
            header,
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

        container(sidebar)
            .width(280)
            .height(Fill)
            .style(sidebar_style)
            .into()
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
        let selected = self.selected == Some(id);
        let hovered = self.hovered == Some(id);

        let lead: Element<'a, Message> = match &node.kind {
            Kind::Request(req) => text(req.method.short())
                .size(10)
                .font(BOLD)
                .width(36)
                .color(method_color(req.method))
                .into(),
            _ => text(if node.expanded { "▾" } else { "▸" })
                .size(12)
                .width(14)
                .style(muted)
                .into(),
        };

        let label: Element<'a, Message> = match self.renaming.as_ref().filter(|r| r.id == id) {
            Some(r) => text_input("Name", &r.text)
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

        let content = row![space().width(depth as f32 * 14.0), lead, label, dirty]
            .spacing(6)
            .align_y(iced::Center)
            .padding(Padding::from([5, 8]));

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
                Item("Delete", Some("Del"), MenuAction::Delete, true, true),
            ],
            MenuTarget::Empty => vec![Item(
                "New Collection",
                None,
                MenuAction::NewCollection,
                true,
                false,
            )],
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

    fn delete_dialog(&self, id: Id) -> Option<Element<'_, Message>> {
        let node = self.tree.get(id)?;

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
                        if count == 1 { "" } else { "s" }
                    ),
                )
            }
        };

        let card = container(
            column![
                text(format!("Delete {what}?")).size(17).font(BOLD),
                text(format!("\"{}\" will be permanently deleted.", node.name)).size(13),
                text(detail).size(13).style(muted),
                space().height(6),
                row![
                    space().width(Fill),
                    button(text("Cancel").size(13).center())
                        .padding(Padding::from([7, 18]))
                        .style(button::secondary)
                        .on_press(Message::CancelDelete),
                    button(text("Delete").size(13).center().color(Color::WHITE))
                        .padding(Padding::from([7, 18]))
                        .style(button::danger)
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

        container(column![
            self.tab_bar(),
            hline(),
            container(
                column![
                    self.url_bar(req, rt),
                    self.section_bar(req, rt),
                    container(self.section_content(req, rt)).height(Length::FillPortion(1)),
                ]
                .spacing(10)
                .padding(12)
            )
            .height(Length::FillPortion(1)),
            hline(),
            container(self.response_panel(rt))
                .padding(12)
                .height(Length::FillPortion(1)),
        ])
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

        centered(
            column![
                text("Oxide").size(48).font(BOLD).color(color!(0xe0652f)),
                text("Select a request on the left, or create one to get started.")
                    .size(14)
                    .style(muted),
                space().height(16),
                hint("Ctrl+N", "New request"),
                hint("Ctrl+Shift+N", "New folder"),
                hint("Ctrl+S", "Save request"),
                hint("Ctrl+D", "Duplicate"),
                hint("F2", "Rename"),
                hint("Del", "Delete"),
                hint("Ctrl+1…9", "Switch tab"),
                hint("Ctrl+W", "Close tab"),
            ]
            .spacing(6)
            .align_x(iced::Center),
        )
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

            let tab_view = column![
                container(space())
                    .width(Fill)
                    .height(2)
                    .style(accent_bar(active)),
                container(content)
                    .width(190)
                    .clip(true)
                    .style(tab_style(active)),
            ];

            Some(
                mouse_area(tab_view)
                    .on_press(Message::ActivateTab(id))
                    .on_double_click(Message::PinTab(id))
                    .on_middle_press(Message::CloseTab(id))
                    .on_right_press(Message::TabContext(id))
                    .into(),
            )
        });

        mouse_area(
            scrollable(Row::with_children(tabs).spacing(1))
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

        let methods = pick_list(Method::ALL, Some(req.method), |m| {
            Message::Edit(Edit::Method(m))
        })
        .width(120)
        .padding(8);

        let url = text_input("Enter URL, e.g. https://httpbin.org/get", &req.url)
            .on_input(|s| Message::Edit(Edit::Url(s)))
            .on_submit(Message::Send)
            .padding(8);

        let action = if sending {
            button(text("Cancel").center())
                .style(button::danger)
                .on_press(Message::Cancel)
        } else {
            button(text("Send").center().color(Color::WHITE).font(SEMIBOLD))
                .style(button::primary)
                .on_press(Message::Send)
        }
        .width(90)
        .padding(8);

        row![methods, url, action].spacing(8).into()
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
        ];

        Row::with_children(sections.into_iter().map(|(section, label)| {
            button(text(label).size(13))
                .padding(Padding::from([5, 12]))
                .style(pill_style(rt.section == section))
                .on_press(Message::SectionChanged(section))
                .into()
        }))
        .spacing(4)
        .into()
    }

    fn section_content<'a>(&'a self, req: &'a Request, rt: &'a Runtime) -> Element<'a, Message> {
        match rt.section {
            Section::Params => kv_table(KvList::Params, &req.params, "Key", "Value"),
            Section::Headers => kv_table(KvList::Headers, &req.headers, "Header", "Value"),
            Section::Cookies => kv_table(KvList::Cookies, &req.cookies, "Cookie name", "Value"),
            Section::Body => self.body_section(req, rt),
            Section::Auth => self.auth_section(req),
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
                    .placeholder(placeholder)
                    .on_action(|a| Message::Edit(Edit::BodyAction(a)))
                    .font(Font::MONOSPACE)
                    .size(13)
                    .padding(10)
                    .height(Fill)
                    .into()
            }
        };

        column![kinds, content].spacing(10).height(Fill).into()
    }

    fn auth_section<'a>(&'a self, req: &'a Request) -> Element<'a, Message> {
        let kind = labeled(
            "Type",
            pick_list(AuthKind::ALL, Some(req.auth_kind), |k| {
                Message::Edit(Edit::AuthKind(k))
            })
            .width(200)
            .padding(6),
        );

        let fields: Element<'a, Message> = match req.auth_kind {
            AuthKind::None => text("This request does not use any authorization.")
                .size(13)
                .style(muted)
                .into(),
            AuthKind::Bearer => labeled(
                "Token",
                text_input("Token", &req.bearer_token)
                    .on_input(|s| Message::Edit(Edit::BearerToken(s)))
                    .padding(6),
            ),
            AuthKind::Basic => column![
                labeled(
                    "Username",
                    text_input("Username", &req.basic_user)
                        .on_input(|s| Message::Edit(Edit::BasicUser(s)))
                        .padding(6),
                ),
                labeled(
                    "Password",
                    text_input("Password", &req.basic_password)
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
                        .on_input(|s| Message::Edit(Edit::ApiKeyName(s)))
                        .padding(6),
                ),
                labeled(
                    "Value",
                    text_input("Value", &req.api_key_value)
                        .on_input(|s| Message::Edit(Edit::ApiKeyValue(s)))
                        .padding(6),
                ),
                labeled(
                    "Add to",
                    pick_list(ApiKeyLocation::ALL, Some(req.api_key_in), |l| {
                        Message::Edit(Edit::ApiKeyIn(l))
                    })
                    .width(200)
                    .padding(6),
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
                .on_input(move |s| Message::Edit(Edit::KvKey(list, i, s)))
                .size(13)
                .padding(6),
            text_input(value_placeholder, &kv.value)
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
