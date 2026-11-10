//! Autocomplete for `{{variable}}` fields.
//!
//! [`suggest`] wraps a text input. While the input is focused and the caller passes a
//! non-empty list, a small list floats under it. Up/Down choose, Enter or Tab (or a click)
//! accept, Esc closes it. Accepting does not edit the text itself: it *types* the missing
//! characters into the wrapped input, so the cursor and the app's own change messages behave
//! exactly as if the person had typed the name.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Quad};
use iced::advanced::text::Renderer as TextRenderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::Renderer as _;
use iced::advanced::{overlay, Clipboard, Shell};
use iced::keyboard::key::Named;
use iced::keyboard::{self, Key};
use iced::{
    alignment, mouse, window, Border, Color, Element, Event, Font, Length, Pixels, Point,
    Rectangle, Shadow, Size, Theme, Vector,
};

const ROW: f32 = 28.0;
const PAD: f32 = 4.0;
const MAX_ROWS: usize = 8;

/// One entry of the list.
#[derive(Debug, Clone)]
pub struct Suggestion {
    /// The variable name (typed between `{{` and `}}`).
    pub name: String,
    /// Shown muted on the right (its current value).
    pub hint: String,
}

/// Wraps `content` (a text input). `partial` is how many characters of the name were already
/// typed after the `{{`.
pub fn suggest<'a, Message: 'a>(
    content: impl Into<Element<'a, Message>>,
    items: Vec<Suggestion>,
    partial: usize,
) -> Element<'a, Message> {
    let mut items = items;
    items.truncate(MAX_ROWS);
    Element::new(Suggest {
        content: content.into(),
        items,
        partial,
    })
}

struct Suggest<'a, Message> {
    content: Element<'a, Message>,
    items: Vec<Suggestion>,
    partial: usize,
}

#[derive(Default)]
struct State {
    /// The wrapped input has the keyboard.
    focused: bool,
    /// Esc was pressed: stay closed until the next typed character.
    dismissed: bool,
    selected: usize,
    /// A row clicked in the list, waiting for the next `update` to be typed in.
    pending: Option<usize>,
}

fn key_event(key: Key, text: Option<&str>) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: key.clone(),
        modified_key: key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::empty(),
        text: text.map(Into::into),
        repeat: false,
    })
}

impl<Message> Suggest<'_, Message> {
    /// Types `name` over the characters already typed, as if the person had done it.
    #[allow(clippy::too_many_arguments)]
    fn accept(
        &mut self,
        tree: &mut widget::Tree,
        name: &str,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let mut events: Vec<Event> = (0..self.partial)
            .map(|_| key_event(Key::Named(Named::Backspace), None))
            .collect();
        for c in format!("{name}}}}}").chars() {
            let text = c.to_string();
            events.push(key_event(Key::Character(text.as_str().into()), Some(&text)));
        }
        for event in events {
            self.content.as_widget_mut().update(
                &mut tree.children[0],
                &event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Suggest<'_, Message> {
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&[self.content.as_widget()]);
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let count = self.items.len();
        {
            let state = tree.state.downcast_mut::<State>();
            if count == 0 {
                state.selected = 0;
                state.pending = None;
            } else {
                state.selected = state.selected.min(count - 1);
            }
        }

        // A row clicked in the list is typed in here, where the input can be reached.
        let pending = tree.state.downcast_mut::<State>().pending.take();
        if let Some(name) = pending.and_then(|i| self.items.get(i)).map(|i| i.name.clone()) {
            self.accept(
                tree, &name, layout, cursor, renderer, clipboard, shell, viewport,
            );
        }

        let (visible, selected) = {
            let state = tree.state.downcast_ref::<State>();
            (state.focused && count > 0 && !state.dismissed, state.selected)
        };

        if visible {
            if let Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) = event {
                match key.as_ref() {
                    Key::Named(Named::ArrowDown) | Key::Named(Named::ArrowUp) => {
                        let down = matches!(key.as_ref(), Key::Named(Named::ArrowDown));
                        let next = if down {
                            (selected + 1) % count
                        } else {
                            (selected + count - 1) % count
                        };
                        tree.state.downcast_mut::<State>().selected = next;
                        shell.capture_event();
                        shell.request_redraw();
                        return;
                    }
                    Key::Named(Named::Enter) | Key::Named(Named::Tab) if !modifiers.shift() => {
                        let name = self.items[selected].name.clone();
                        self.accept(
                            tree, &name, layout, cursor, renderer, clipboard, shell, viewport,
                        );
                        shell.capture_event();
                        shell.request_redraw();
                        return;
                    }
                    Key::Named(Named::Escape) => {
                        tree.state.downcast_mut::<State>().dismissed = true;
                        shell.request_redraw();
                    }
                    _ => {}
                }
            }
        }

        let was_captured = shell.is_event_captured();
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
        let handled_here = !was_captured && shell.is_event_captured();

        let state = tree.state.downcast_mut::<State>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let inside = cursor.is_over(layout.bounds());
                if state.focused != inside {
                    state.focused = inside;
                    shell.request_redraw();
                }
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) => {
                if matches!(key.as_ref(), Key::Named(Named::Tab)) {
                    // Focus moves on to another field.
                    state.focused = false;
                } else if handled_here {
                    state.focused = true;
                    if !matches!(key.as_ref(), Key::Named(Named::Escape)) {
                        state.dismissed = false;
                        state.selected = 0;
                    }
                    shell.request_redraw();
                }
            }
            Event::Window(window::Event::Unfocused) => state.focused = false,
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content
            .as_widget_mut()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'b>,
        renderer: &iced::Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, iced::Renderer>> {
        let (first, rest) = tree.children.split_at_mut(1);
        let _ = rest;
        let content = self.content.as_widget_mut().overlay(
            &mut first[0],
            layout,
            renderer,
            viewport,
            translation,
        );

        let state = &mut tree.state;
        let (focused, dismissed) = {
            let s = state.downcast_ref::<State>();
            (s.focused, s.dismissed)
        };
        let list = (focused && !dismissed && !self.items.is_empty()).then(|| {
            overlay::Element::new(Box::new(List {
                anchor: Rectangle::new(layout.position() + translation, layout.bounds().size()),
                items: &self.items,
                state: state.downcast_mut::<State>(),
            }))
        });

        match (content, list) {
            (None, None) => None,
            (content, list) => Some(
                overlay::Group::with_children(content.into_iter().chain(list).collect())
                    .overlay(),
            ),
        }
    }
}

struct List<'a> {
    /// The input the list hangs from.
    anchor: Rectangle,
    items: &'a [Suggestion],
    state: &'a mut State,
}

impl<Message> overlay::Overlay<Message, Theme, iced::Renderer> for List<'_> {
    fn layout(&mut self, _renderer: &iced::Renderer, bounds: Size) -> layout::Node {
        let width = self.anchor.width.clamp(240.0, 380.0).min(bounds.width);
        let height = self.items.len() as f32 * ROW + PAD * 2.0;

        let x = self.anchor.x.min(bounds.width - width).max(0.0);
        let below = self.anchor.y + self.anchor.height + 3.0;
        let y = if below + height <= bounds.height {
            below
        } else {
            (self.anchor.y - height - 3.0).max(0.0)
        };
        layout::Node::new(Size::new(width, height)).move_to(Point::new(x, y))
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        let bounds = layout.bounds();
        let Some(position) = cursor.position_in(bounds) else {
            return;
        };
        let row = (((position.y - PAD) / ROW).floor().max(0.0) as usize)
            .min(self.items.len().saturating_sub(1));

        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if self.state.selected != row {
                    self.state.selected = row;
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                self.state.selected = row;
                self.state.pending = Some(row);
                shell.capture_event();
                shell.request_redraw();
            }
            _ => {}
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
    ) {
        let palette = theme.extended_palette();
        let bounds = layout.bounds();
        let text_color = palette.background.weak.text;
        let muted = Color {
            a: 0.55,
            ..text_color
        };

        renderer.fill_quad(
            Quad {
                bounds,
                border: Border {
                    color: palette.background.strong.color,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: Shadow::default(),
                snap: false,
            },
            palette.background.weak.color,
        );

        for (index, item) in self.items.iter().enumerate() {
            let row = Rectangle {
                x: bounds.x + PAD,
                y: bounds.y + PAD + index as f32 * ROW,
                width: bounds.width - PAD * 2.0,
                height: ROW,
            };
            if index == self.state.selected {
                renderer.fill_quad(
                    Quad {
                        bounds: row,
                        border: Border {
                            radius: 6.0.into(),
                            ..Border::default()
                        },
                        ..Quad::default()
                    },
                    palette.primary.base.color.scale_alpha(0.28),
                );
            }

            let name = iced::advanced::text::Text {
                content: format!("{{{{{}}}}}", item.name),
                bounds: Size::new(row.width * 0.6 - 10.0, ROW),
                size: Pixels(13.0),
                line_height: iced::advanced::text::LineHeight::default(),
                font: Font::MONOSPACE,
                align_x: iced::advanced::text::Alignment::Left,
                align_y: alignment::Vertical::Center,
                shaping: iced::advanced::text::Shaping::Advanced,
                wrapping: iced::advanced::text::Wrapping::None,
            };
            // For centred/right alignment the position is the anchor, not the top-left corner.
            renderer.fill_text(
                name,
                Point::new(row.x + 8.0, row.center_y()),
                text_color,
                row,
            );

            if !item.hint.is_empty() {
                let hint = iced::advanced::text::Text {
                    content: item.hint.clone(),
                    bounds: Size::new(row.width * 0.4 - 12.0, ROW),
                    size: Pixels(11.0),
                    line_height: iced::advanced::text::LineHeight::default(),
                    font: Font::DEFAULT,
                    align_x: iced::advanced::text::Alignment::Right,
                    align_y: alignment::Vertical::Center,
                    shaping: iced::advanced::text::Shaping::Advanced,
                    wrapping: iced::advanced::text::Wrapping::None,
                };
                renderer.fill_text(
                    hint,
                    Point::new(row.x + row.width - 8.0, row.center_y()),
                    muted,
                    row,
                );
            }
        }
    }
}

/// The variable names that start with what was typed after an unclosed `{{`, plus how many
/// characters that was. `None` when the text does not end inside a `{{…`.
pub fn partial_at_end(text: &str) -> Option<&str> {
    let start = text.rfind("{{")?;
    let tail = &text[start + 2..];
    if tail.contains("}}") || tail.contains(['{', '}']) || tail.contains(char::is_whitespace) {
        return None;
    }
    Some(tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_name_being_typed() {
        assert_eq!(partial_at_end("{{"), Some(""));
        assert_eq!(partial_at_end("{{ba"), Some("ba"));
        assert_eq!(partial_at_end("https://{{host}}/{{us"), Some("us"));
        assert_eq!(partial_at_end("{{done}}"), None);
        assert_eq!(partial_at_end("a {{b c"), None);
        assert_eq!(partial_at_end("plain"), None);
    }
}
