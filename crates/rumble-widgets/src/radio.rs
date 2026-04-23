//! Radio button — select one of N.
//!
//! `Radio<'a, T>` takes a `&mut T: PartialEq + Clone` and a specific
//! `value`; the radio is "selected" when `*current == value`. Clicking
//! an unselected radio sets `*current = value`; clicking a selected one
//! does nothing.
//!
//! Painting flows through `Theme::radio_indicator`, so themes can swap
//! geometry without this file moving. See `luna.rs` for an XP override.

use eframe::egui::{
    Color32, CornerRadius, Key, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, TextWrapMode, Ui, Vec2,
    WidgetText, emath::GuiRounding, epaint::RectShape,
};

use crate::{
    theme::UiExt,
    tokens::{PressableState, TextRole, Tokens},
};

const RADIO_SIZE: f32 = 16.0;
const LABEL_GAP: f32 = 8.0;

pub struct Radio<'a, T: PartialEq + Clone> {
    current: &'a mut T,
    value: T,
    label: WidgetText,
    disabled: bool,
}

impl<'a, T: PartialEq + Clone> Radio<'a, T> {
    pub fn new(current: &'a mut T, value: T, label: impl Into<WidgetText>) -> Self {
        Self {
            current,
            value,
            label: label.into(),
            disabled: false,
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.theme();
        let font = theme.font(TextRole::Body);
        let galley = self
            .label
            .clone()
            .into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, font);

        let size = Vec2::new(
            RADIO_SIZE + LABEL_GAP + galley.rect.width(),
            RADIO_SIZE.max(galley.rect.height()),
        );
        let sense = if self.disabled { Sense::hover() } else { Sense::click() };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);

        let selected = *self.current == self.value;
        let space = response.has_focus() && ui.input(|i| i.key_pressed(Key::Space));
        if (response.clicked() || space) && !selected {
            *self.current = self.value.clone();
            response.mark_changed();
        }
        // Recompute after potential mutation.
        let selected = *self.current == self.value;

        let state = PressableState {
            active: selected,
            hovered: response.hovered() && !self.disabled,
            pressed: response.is_pointer_button_down_on(),
            focused: response.has_focus(),
            disabled: self.disabled,
        };

        // Snap the indicator to the physical-pixel grid *before* the
        // theme derives any inner geometry from it. Rect/line shapes
        // get snapped by the tessellator automatically, but circles
        // (the radio dot) do not — if the outer rect is sub-pixel,
        // `rect.center()` would be sub-pixel too and the dot would
        // render off-center relative to the snapped outer shape.
        let ppp = ui.ctx().pixels_per_point();
        let indicator_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - RADIO_SIZE * 0.5),
            Vec2::splat(RADIO_SIZE),
        )
        .round_to_pixels(ppp);
        ui.painter().add(theme.radio_indicator(indicator_rect, selected, state));

        let label_rect = Rect::from_min_max(
            Pos2::new(indicator_rect.right() + LABEL_GAP, rect.top()),
            rect.right_bottom(),
        );
        let text_color = if self.disabled {
            blend(theme.tokens().text, theme.tokens().surface, 0.5)
        } else {
            theme.tokens().text
        };
        crate::toggle::paint_label_accessible(ui, label_rect, &self.label, text_color, response.id.with("radio_label"));

        response
    }
}

/// Flat fallback paint used by `Theme::radio_indicator`'s default impl.
pub fn default_radio_indicator(tokens: &Tokens, rect: Rect, selected: bool, state: PressableState) -> Shape {
    let (fill, border) = if selected {
        (tokens.accent, tokens.accent)
    } else {
        (tokens.surface, tokens.line)
    };
    let fill = if state.disabled {
        blend(fill, tokens.surface_sunken, 0.55)
    } else if state.hovered {
        blend(fill, Color32::WHITE, 0.06)
    } else {
        fill
    };

    let mut shapes: Vec<Shape> = Vec::new();
    // Outer circle: rect with 50% radius is an ellipse; fine for square rect.
    let radius = rect.width() * 0.5;
    shapes.push(Shape::Rect(RectShape::new(
        rect,
        CornerRadius::from(radius),
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    )));
    if selected {
        let dot_r = rect.width() * 0.22;
        let dot_color = if state.disabled {
            blend(Color32::WHITE, tokens.surface, 0.5)
        } else {
            Color32::WHITE
        };
        shapes.push(Shape::circle_filled(rect.center(), dot_r, dot_color));
    }
    if state.focused && !state.disabled {
        shapes.push(Shape::Rect(RectShape::stroke(
            rect.expand(1.5),
            CornerRadius::from(radius + 1.5),
            Stroke::new(1.5, tokens.accent),
            StrokeKind::Outside,
        )));
    }
    Shape::Vec(shapes)
}

fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 { (x as f32 * (1.0 - t) + y as f32 * t).round().clamp(0.0, 255.0) as u8 };
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}
