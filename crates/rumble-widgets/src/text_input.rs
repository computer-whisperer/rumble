//! Themed wrapper around egui's `TextEdit`.
//!
//! egui already handles caret, selection, IME, clipboard, password
//! masking, and undo, so the widget is intentionally a styling wrapper:
//! it allocates the outer rect, paints a `Field` surface, places a child
//! UI inside the bevel-safe area, and runs `TextEdit::frame(false)` so
//! egui doesn't paint its own background. The focus ring uses the
//! theme's accent color.
//!
//! `submit_on_enter` captures Enter (without Shift) and clears the buffer,
//! returning the submitted string in `TextInputResponse::submitted`.
//! Shift+Enter falls through, so multiline composers can still insert
//! newlines.

use std::hash::Hash;

use eframe::egui::{
    Align, Color32, CornerRadius, Event, Id, Key, Layout, Margin, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, TextEdit, Ui, UiBuilder, Vec2, epaint::RectShape,
};

use crate::{
    theme::UiExt,
    tokens::{SurfaceKind, TextRole},
};

const PAD_X: f32 = 8.0;
const PAD_Y: f32 = 6.0;
const DEFAULT_WIDTH: f32 = 240.0;

pub struct TextInput<'a> {
    buf: &'a mut String,
    placeholder: &'static str,
    multiline: bool,
    password: bool,
    submit_on_enter: bool,
    desired_width: f32,
    rows: usize,
    id_salt: Id,
}

#[derive(Debug)]
pub struct TextInputResponse {
    pub response: Response,
    /// `Some(text)` on the frame the user pressed Enter while focused
    /// (and `submit_on_enter` is set). The buffer is cleared at the same
    /// time. `None` otherwise.
    pub submitted: Option<String>,
}

impl std::ops::Deref for TextInputResponse {
    type Target = Response;
    fn deref(&self) -> &Response {
        &self.response
    }
}

impl<'a> TextInput<'a> {
    pub fn new(buf: &'a mut String) -> Self {
        Self {
            buf,
            placeholder: "",
            multiline: false,
            password: false,
            submit_on_enter: false,
            desired_width: DEFAULT_WIDTH,
            rows: 1,
            id_salt: Id::new("rumble_widgets::text_input"),
        }
    }

    pub fn placeholder(mut self, text: &'static str) -> Self {
        self.placeholder = text;
        self
    }

    pub fn multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        if multiline && self.rows == 1 {
            self.rows = 3;
        }
        self
    }

    pub fn password(mut self, password: bool) -> Self {
        self.password = password;
        self
    }

    /// Treat unmodified Enter as "submit": capture the buffer, clear it,
    /// and return the captured value via `TextInputResponse::submitted`.
    /// Shift+Enter still inserts a newline in multiline mode.
    pub fn submit_on_enter(mut self, submit: bool) -> Self {
        self.submit_on_enter = submit;
        self
    }

    pub fn desired_width(mut self, w: f32) -> Self {
        self.desired_width = w;
        self
    }

    /// Visible row count for multiline mode. Single-line mode ignores this.
    pub fn rows(mut self, rows: usize) -> Self {
        self.rows = rows.max(1);
        self
    }

    pub fn id_salt(mut self, salt: impl Hash) -> Self {
        self.id_salt = Id::new(salt);
        self
    }

    pub fn show(self, ui: &mut Ui) -> TextInputResponse {
        let theme = ui.theme();
        let tokens = theme.tokens();
        let font = theme.font(TextRole::Body);
        let line_h = font.size + 4.0;

        // Outer rect — the visible Field area.
        let rows = if self.multiline { self.rows } else { 1 };
        let outer_h = line_h * rows as f32 + PAD_Y * 2.0;
        let outer_size = Vec2::new(self.desired_width, outer_h);
        let (outer_rect, _bg_resp) = ui.allocate_exact_size(outer_size, Sense::hover());

        // Field background goes first so the text paints on top.
        ui.painter().add(theme.surface(outer_rect, SurfaceKind::Field));

        // Inset for bevel + visual padding so the caret + glyphs don't kiss
        // the border.
        let inset_x = tokens.bevel_inset + PAD_X - 4.0;
        let inset_y = tokens.bevel_inset + PAD_Y - 2.0;
        let inner = Rect::from_min_max(
            Pos2::new(outer_rect.left() + inset_x, outer_rect.top() + inset_y),
            Pos2::new(outer_rect.right() - inset_x, outer_rect.bottom() - inset_y),
        );

        let child_builder = UiBuilder::new()
            .id_salt(self.id_salt)
            .max_rect(inner)
            .layout(Layout::left_to_right(Align::Center));

        let mut inner_resp = ui
            .scope_builder(child_builder, |ui| {
                let mut te = if self.multiline {
                    TextEdit::multiline(self.buf)
                } else {
                    TextEdit::singleline(self.buf)
                };
                te = te
                    .frame(false)
                    .background_color(Color32::TRANSPARENT)
                    .hint_text(self.placeholder)
                    .password(self.password)
                    .desired_width(inner.width())
                    .margin(Margin::ZERO)
                    .font(font.clone())
                    .text_color(tokens.text);
                if self.multiline {
                    te = te.desired_rows(rows);
                }
                ui.add(te)
            })
            .inner;

        // Submit-on-enter: detect Enter (no Shift) while the field is the
        // active widget. Singleline TextEdit drops focus on Enter — that
        // shows up as `lost_focus()` on the same frame — so check both.
        // In multiline mode TextEdit will have already inserted a '\n' on
        // this frame; strip it before capturing.
        //
        // Reading `i.modifiers.shift` is unreliable for a single synthetic
        // Key event (no preceding KeyDown to update the global modifier
        // state), so look at the event's own modifiers.
        let mut submitted = None;
        if self.submit_on_enter && (inner_resp.has_focus() || inner_resp.lost_focus()) {
            let plain_enter = ui.input(|i| {
                i.events.iter().any(|e| {
                    matches!(
                        e,
                        Event::Key {
                            key: Key::Enter,
                            pressed: true,
                            modifiers,
                            ..
                        } if !modifiers.shift
                    )
                })
            });
            if plain_enter {
                if self.multiline && self.buf.ends_with('\n') {
                    self.buf.pop();
                }
                let captured = std::mem::take(self.buf);
                submitted = Some(captured);
            }
        }

        // Focus ring on top.
        if inner_resp.has_focus() {
            ui.painter().add(Shape::Rect(RectShape::stroke(
                outer_rect.expand(1.0),
                CornerRadius::from(tokens.radius_sm + 1.0),
                Stroke::new(1.5, tokens.accent),
                StrokeKind::Outside,
            )));
        }

        // Expose the outer Field rect (not the inner TextEdit rect) so
        // callers can position popups / tooltips relative to the visible
        // field.
        inner_resp.rect = outer_rect;

        TextInputResponse {
            response: inner_resp,
            submitted,
        }
    }
}
