//! Binary on/off toggle.
//!
//! The `Toggle` public struct is a thin facade: it collects builder args
//! and dispatches to the theme's `ToggleImpl`. Themes can swap the impl to
//! fully re-skin the widget (shape, indicator glyph, focus treatment) —
//! not just colors. See `DefaultToggle` for the shared baseline and
//! `LunaToggle` (in `luna.rs`) for an XP-authentic 13×13 checkbox.

use std::hash::Hash;

use eframe::egui::{
    Align, Color32, CornerRadius, Direction, Key, Layout, Pos2, Rect, Response, RichText, Sense, Shape, Stroke,
    StrokeKind, TextWrapMode, Ui, UiBuilder, Vec2, WidgetText, emath::GuiRounding, epaint::RectShape,
};

use crate::{
    theme::UiExt,
    tokens::{TextRole, Tokens},
};

const SWITCH_W: f32 = 36.0;
const SWITCH_H: f32 = 20.0;
const CHECKBOX_SIZE: f32 = 16.0;
pub(crate) const LABEL_GAP: f32 = 8.0;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ToggleStyle {
    Switch,
    Checkbox,
}

/// Snapshot of a Toggle's interaction state, passed to `ToggleImpl::paint`.
#[derive(Copy, Clone, Debug, Default)]
pub struct ToggleState {
    pub on: bool,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
}

/// Builder arguments, passed to `ToggleImpl`.
///
/// `style` is a hint — themes may honor it (the default impl distinguishes
/// Switch vs Checkbox) or ignore it (Luna renders both as an XP checkbox,
/// matching what was actually native to XP).
pub struct ToggleArgs<'a> {
    pub value: &'a mut bool,
    pub label: WidgetText,
    pub style: ToggleStyle,
    pub disabled: bool,
}

/// Theme-provided implementation of the Toggle visual + layout.
///
/// Themes override `layout` and `paint`. The default `show()` handles
/// allocation, click/space input, and value mutation — shared behavior that
/// every theme gets for free.
pub trait ToggleImpl: Send + Sync + 'static {
    /// Total widget size (indicator + gap + label).
    fn layout(&self, ui: &Ui, args: &ToggleArgs) -> Vec2;

    /// Paint the widget inside `rect`. Responsible for both the indicator
    /// *and* the label — themes differ in how they position/style labels
    /// (e.g. XP's dotted focus outline wraps the label, not the indicator).
    fn paint(&self, ui: &mut Ui, rect: Rect, args: &ToggleArgs, state: ToggleState);

    /// Allocate, sense clicks, mutate the value, then hand off to `paint`.
    /// Themes can override for unusual interaction, but the common path is
    /// universal and lives here.
    fn show(&self, ui: &mut Ui, args: ToggleArgs<'_>) -> ToggleResponse {
        let size = self.layout(ui, &args);
        let sense = if args.disabled { Sense::hover() } else { Sense::click() };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);

        let space = response.has_focus() && ui.input(|i| i.key_pressed(Key::Space));
        if response.clicked() || space {
            *args.value = !*args.value;
            response.mark_changed();
        }

        let state = ToggleState {
            on: *args.value,
            hovered: response.hovered() && !args.disabled,
            pressed: response.is_pointer_button_down_on(),
            focused: response.has_focus(),
            disabled: args.disabled,
        };
        self.paint(ui, rect, &args, state);

        ToggleResponse { response }
    }
}

/// The static `ToggleImpl` every theme gets by default — matches the
/// pre-refactor look: iOS-style sliding pill for Switch, rounded-square
/// check box with a drawn ✓ for Checkbox.
pub struct DefaultToggle;
pub(crate) static DEFAULT_TOGGLE: DefaultToggle = DefaultToggle;

impl ToggleImpl for DefaultToggle {
    fn layout(&self, ui: &Ui, args: &ToggleArgs) -> Vec2 {
        let theme = ui.theme();
        let font = theme.font(TextRole::Body);
        let galley = args
            .label
            .clone()
            .into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, font);

        let ind = indicator_size(args.style);
        Vec2::new(ind.x + LABEL_GAP + galley.rect.width(), ind.y.max(galley.rect.height()))
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, args: &ToggleArgs, state: ToggleState) {
        let theme = ui.theme();
        let tokens = theme.tokens();

        let ind = indicator_size(args.style);
        // Snap to the physical-pixel grid so inner geometry derived
        // from this rect (the switch thumb circle in particular) lines
        // up with the tessellator's own rect snapping.
        let ppp = ui.ctx().pixels_per_point();
        let indicator_rect =
            Rect::from_min_size(Pos2::new(rect.left(), rect.center().y - ind.y * 0.5), ind).round_to_pixels(ppp);

        match args.style {
            ToggleStyle::Switch => paint_switch(ui, indicator_rect, state, tokens),
            ToggleStyle::Checkbox => paint_checkbox(ui, indicator_rect, state, tokens),
        }

        let label_rect = Rect::from_min_max(
            Pos2::new(indicator_rect.right() + LABEL_GAP, rect.top()),
            rect.right_bottom(),
        );
        let text_color = if state.disabled {
            blend(tokens.text, tokens.surface, 0.5)
        } else {
            tokens.text
        };
        paint_label_accessible(ui, label_rect, &args.label, text_color, "default_toggle_label");
    }
}

/// Text-labelled builder — what callers actually use.
pub struct Toggle<'a> {
    value: &'a mut bool,
    label: WidgetText,
    style: ToggleStyle,
    disabled: bool,
}

#[derive(Debug)]
pub struct ToggleResponse {
    pub response: Response,
}

impl std::ops::Deref for ToggleResponse {
    type Target = Response;
    fn deref(&self) -> &Response {
        &self.response
    }
}

impl<'a> Toggle<'a> {
    pub fn new(value: &'a mut bool, label: impl Into<WidgetText>) -> Self {
        Self {
            value,
            label: label.into(),
            style: ToggleStyle::Switch,
            disabled: false,
        }
    }

    pub fn checkbox(value: &'a mut bool, label: impl Into<WidgetText>) -> Self {
        Self::new(value, label).style(ToggleStyle::Checkbox)
    }

    pub fn style(mut self, style: ToggleStyle) -> Self {
        self.style = style;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> ToggleResponse {
        let theme = ui.theme();
        let args = ToggleArgs {
            value: self.value,
            label: self.label,
            style: self.style,
            disabled: self.disabled,
        };
        theme.toggle().show(ui, args)
    }
}

// -----------------------------------------------------------------------
// Shared helpers (used by DefaultToggle; themes may reuse too).

pub(crate) fn indicator_size(style: ToggleStyle) -> Vec2 {
    match style {
        ToggleStyle::Switch => Vec2::new(SWITCH_W, SWITCH_H),
        ToggleStyle::Checkbox => Vec2::splat(CHECKBOX_SIZE),
    }
}

/// Place a label inside `rect` using a scoped child `Ui` so it registers
/// with egui's accessibility tree. Kittest's `get_by_label(...)` relies on
/// this — painting via `ui.painter().galley(...)` would be invisible to
/// AccessKit. `id_salt` must be stable across frames per toggle instance.
pub(crate) fn paint_label_accessible(ui: &mut Ui, rect: Rect, label: &WidgetText, color: Color32, id_salt: impl Hash) {
    // Re-materialize the plain text so we can apply our own color.
    let font = ui.theme().font(TextRole::Body);
    let galley = label
        .clone()
        .into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, font);
    if galley.is_empty() {
        return;
    }
    let text = galley.text().to_string();

    let builder = UiBuilder::new()
        .id_salt(id_salt)
        .max_rect(rect)
        .layout(Layout::centered_and_justified(Direction::LeftToRight));
    ui.scope_builder(builder, |ui| {
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.style_mut().interaction.selectable_labels = false;
            ui.label(RichText::new(text).color(color));
        });
    });
}

fn paint_switch(ui: &mut Ui, rect: Rect, state: ToggleState, tokens: &Tokens) {
    let bg = if state.on { tokens.accent } else { tokens.surface_sunken };
    let bg = if state.disabled {
        blend(bg, tokens.surface, 0.55)
    } else if state.hovered {
        blend(bg, Color32::WHITE, 0.06)
    } else {
        bg
    };

    let radius = CornerRadius::from(rect.height() * 0.5);
    ui.painter().add(Shape::Rect(RectShape::new(
        rect,
        radius,
        bg,
        Stroke::new(1.0, tokens.line_soft),
        StrokeKind::Inside,
    )));

    // Thumb.
    let thumb_d = rect.height() - 4.0;
    let cy = rect.center().y;
    let cx = if state.on {
        rect.right() - thumb_d * 0.5 - 2.0
    } else {
        rect.left() + thumb_d * 0.5 + 2.0
    };
    let thumb_color = if state.disabled {
        blend(tokens.surface, tokens.surface_sunken, 0.5)
    } else {
        Color32::WHITE
    };
    ui.painter()
        .add(Shape::circle_filled(Pos2::new(cx, cy), thumb_d * 0.5, thumb_color));

    if state.focused && !state.disabled {
        ui.painter().add(Shape::Rect(RectShape::stroke(
            rect.expand(1.5),
            CornerRadius::from(rect.height() * 0.5 + 1.5),
            Stroke::new(1.5, tokens.accent),
            StrokeKind::Outside,
        )));
    }
}

fn paint_checkbox(ui: &mut Ui, rect: Rect, state: ToggleState, tokens: &Tokens) {
    let radius = CornerRadius::from(tokens.radius_sm);
    let (fill, stroke_color) = if state.on {
        (tokens.accent, tokens.accent)
    } else {
        (tokens.surface, tokens.line_soft)
    };
    let fill = if state.disabled {
        blend(fill, tokens.surface, 0.55)
    } else if state.hovered {
        blend(fill, Color32::WHITE, 0.05)
    } else {
        fill
    };

    ui.painter().add(Shape::Rect(RectShape::new(
        rect,
        radius,
        fill,
        Stroke::new(1.0, stroke_color),
        StrokeKind::Inside,
    )));

    if state.on {
        let glyph_color = if state.disabled {
            blend(Color32::WHITE, tokens.surface, 0.5)
        } else {
            Color32::WHITE
        };
        let stroke = Stroke::new(2.0, glyph_color);
        let inset = rect.size().x * 0.22;
        let inner = rect.shrink(inset);
        let p1 = Pos2::new(inner.left(), inner.center().y + inner.height() * 0.05);
        let p2 = Pos2::new(
            inner.left() + inner.width() * 0.4,
            inner.bottom() - inner.height() * 0.05,
        );
        let p3 = Pos2::new(inner.right(), inner.top() + inner.height() * 0.10);
        ui.painter().line_segment([p1, p2], stroke);
        ui.painter().line_segment([p2, p3], stroke);
    }

    if state.focused && !state.disabled {
        ui.painter().add(Shape::Rect(RectShape::stroke(
            rect.expand(1.5),
            CornerRadius::from(tokens.radius_sm + 1.5),
            Stroke::new(1.5, tokens.accent),
            StrokeKind::Outside,
        )));
    }
}

pub(crate) fn blend(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let mix = |x: u8, y: u8| -> u8 { (x as f32 * (1.0 - t) + y as f32 * t).round().clamp(0.0, 255.0) as u8 };
    Color32::from_rgba_unmultiplied(
        mix(a.r(), b.r()),
        mix(a.g(), b.g()),
        mix(a.b(), b.b()),
        mix(a.a(), b.a()),
    )
}
