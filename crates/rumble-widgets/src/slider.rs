//! Horizontal slider with optional value readout.
//!
//! `Slider` is a thin builder facade that collects args and dispatches to
//! the theme's `SliderImpl`. Themes can swap the impl to fully re-skin the
//! widget — track shape, thumb glyph, focus treatment — not just colors.
//! See `DefaultSlider` for the shared baseline (circular thumb, rounded
//! track with accent fill) and `LunaSlider` for the XP-authentic
//! arrow-pointer thumb on a 2-px sunken track.
//!
//! Mutates `&mut f32` in place (egui's idiom) and returns a `Response`
//! whose `.changed()` flag fires the frame the value moves. Click-to-jump,
//! drag, and keyboard steps (arrows, PageUp/Down, Home/End while focused)
//! are handled by the default `show()` so every theme inherits them.

use std::ops::RangeInclusive;

use eframe::egui::{
    Align2, Color32, CornerRadius, Key, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2, emath::GuiRounding,
};

use crate::{
    theme::UiExt,
    tokens::{PressableState, SurfaceKind, TextRole},
};

const DEFAULT_THUMB_DIAMETER: f32 = 14.0;
const DEFAULT_TRACK_HEIGHT: f32 = 6.0;
const DEFAULT_ROW_HEIGHT: f32 = 18.0;
const DEFAULT_WIDTH: f32 = 200.0;
pub(crate) const VALUE_BOX_WIDTH: f32 = 52.0;
pub(crate) const VALUE_BOX_GAP: f32 = 8.0;

/// Snapshot of a Slider's interaction state, passed to `SliderImpl::paint`.
#[derive(Copy, Clone, Debug, Default)]
pub struct SliderState {
    /// Normalized position in `[0, 1]`.
    pub t: f32,
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
    pub disabled: bool,
}

/// Builder arguments, passed to `SliderImpl`.
pub struct SliderArgs<'a> {
    pub value: &'a mut f32,
    pub range: RangeInclusive<f32>,
    pub step: Option<f32>,
    pub suffix: &'static str,
    pub decimals: usize,
    pub show_value: bool,
    pub width: f32,
    pub value_box_width: f32,
    pub disabled: bool,
}

/// Theme-provided implementation of the Slider visual + layout.
///
/// Themes override `layout`, `track_rect`, and `paint`. The default
/// `show()` handles allocation, click/drag/keyboard input, and value
/// mutation — shared behavior that every theme gets for free.
pub trait SliderImpl: Send + Sync + 'static {
    /// Total widget size (track + optional gap + value box).
    fn layout(&self, ui: &Ui, args: &SliderArgs) -> Vec2;

    /// Active track region — pointer x within this rect maps linearly to
    /// the value's normalized position. Used by the default `show()` to
    /// translate clicks/drags into values.
    fn track_rect(&self, rect: Rect, args: &SliderArgs) -> Rect;

    /// Paint the widget inside `rect`. Responsible for the track, the
    /// thumb, and (when `args.show_value`) the value readout.
    fn paint(&self, ui: &mut Ui, rect: Rect, args: &SliderArgs, state: SliderState);

    /// Allocate, sense pointer/keys, mutate the value, then hand off to
    /// `paint`. Themes can override for unusual interaction, but the common
    /// path is universal and lives here.
    fn show(&self, ui: &mut Ui, args: SliderArgs<'_>) -> SliderResponse {
        let size = self.layout(ui, &args);
        let sense = if args.disabled {
            Sense::hover()
        } else {
            Sense::click_and_drag()
        };
        let (rect, mut response) = ui.allocate_exact_size(size, sense);

        let track = self.track_rect(rect, &args);
        let (lo, hi) = (*args.range.start(), *args.range.end());
        let span = (hi - lo).max(f32::EPSILON);

        // Pointer interaction: clicked or dragged → snap to cursor position.
        let mut new_value = *args.value;
        if !args.disabled && response.is_pointer_button_down_on() {
            response.request_focus();
            if let Some(p) = response.interact_pointer_pos() {
                let t = pos_to_norm(p.x, track.left(), track.width());
                new_value = lo + t * span;
            }
        }

        // Keyboard interaction (when focused).
        if !args.disabled && response.has_focus() {
            ui.input(|i| {
                let step = args.step.unwrap_or(span / 100.0).max(f32::EPSILON);
                let big = step * 10.0;
                if i.key_pressed(Key::ArrowRight) || i.key_pressed(Key::ArrowUp) {
                    new_value += step;
                }
                if i.key_pressed(Key::ArrowLeft) || i.key_pressed(Key::ArrowDown) {
                    new_value -= step;
                }
                if i.key_pressed(Key::PageUp) {
                    new_value += big;
                }
                if i.key_pressed(Key::PageDown) {
                    new_value -= big;
                }
                if i.key_pressed(Key::Home) {
                    new_value = lo;
                }
                if i.key_pressed(Key::End) {
                    new_value = hi;
                }
            });
        }

        new_value = snap(new_value, args.step, lo, hi);
        if (new_value - *args.value).abs() > f32::EPSILON {
            *args.value = new_value;
            response.mark_changed();
        }

        let state = SliderState {
            t: ((*args.value - lo) / span).clamp(0.0, 1.0),
            hovered: response.hovered() && !args.disabled,
            pressed: response.is_pointer_button_down_on(),
            focused: response.has_focus(),
            disabled: args.disabled,
        };
        self.paint(ui, rect, &args, state);

        SliderResponse { response }
    }
}

/// The static `SliderImpl` every theme gets by default — preserves the
/// pre-refactor look: rounded sunken track, accent fill from start to
/// thumb, circular thumb with accent stroke.
pub struct DefaultSlider;
pub(crate) static DEFAULT_SLIDER: DefaultSlider = DefaultSlider;

impl SliderImpl for DefaultSlider {
    fn layout(&self, _ui: &Ui, args: &SliderArgs) -> Vec2 {
        let total_w = if args.show_value {
            args.width + VALUE_BOX_GAP + args.value_box_width
        } else {
            args.width
        };
        Vec2::new(total_w, DEFAULT_ROW_HEIGHT.max(DEFAULT_THUMB_DIAMETER))
    }

    fn track_rect(&self, rect: Rect, args: &SliderArgs) -> Rect {
        let track_outer_right = if args.show_value {
            rect.left() + args.width
        } else {
            rect.right()
        };
        let inset = DEFAULT_THUMB_DIAMETER * 0.5;
        Rect::from_min_max(
            Pos2::new(rect.left() + inset, rect.center().y - DEFAULT_TRACK_HEIGHT * 0.5),
            Pos2::new(track_outer_right - inset, rect.center().y + DEFAULT_TRACK_HEIGHT * 0.5),
        )
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, args: &SliderArgs, state: SliderState) {
        let theme = ui.theme();
        let tokens = theme.tokens();

        // Snap the track and thumb to physical pixels so the thumb
        // circle's center lines up with the tessellator-snapped track.
        let ppp = ui.ctx().pixels_per_point();
        let active_track = self.track_rect(rect, args).round_to_pixels(ppp);
        let thumb_x = active_track.left() + active_track.width() * state.t;
        let thumb_center = Pos2::new(thumb_x, active_track.center().y).round_to_pixels(ppp);
        let fill_rect = Rect::from_min_max(active_track.left_top(), Pos2::new(thumb_x, active_track.bottom()));

        // Track background as a Field surface.
        ui.painter().add(theme.surface(active_track, SurfaceKind::Field));

        // Accent fill from start up to the thumb.
        if fill_rect.width() > 0.5 {
            let inner_fill = fill_rect.shrink2(Vec2::new(0.0, tokens.bevel_inset.min(1.5)));
            ui.painter().add(Shape::rect_filled(
                inner_fill,
                CornerRadius::from(tokens.radius_sm),
                tokens.accent,
            ));
        }

        // Thumb.
        let thumb_fill = if state.pressed {
            blend(tokens.surface, Color32::BLACK, 0.08)
        } else if state.hovered {
            blend(tokens.surface, tokens.accent, 0.1)
        } else {
            tokens.surface
        };
        ui.painter().add(Shape::circle_filled(
            thumb_center,
            DEFAULT_THUMB_DIAMETER * 0.5,
            thumb_fill,
        ));
        ui.painter().add(Shape::circle_stroke(
            thumb_center,
            DEFAULT_THUMB_DIAMETER * 0.5,
            Stroke::new(1.5, tokens.accent),
        ));
        if state.focused {
            ui.painter().add(Shape::circle_stroke(
                thumb_center,
                DEFAULT_THUMB_DIAMETER * 0.5 + 2.0,
                Stroke::new(1.5, tokens.accent),
            ));
        }

        // Value readout.
        if args.show_value {
            let split_x = rect.left() + args.width;
            let value_rect = Rect::from_min_max(Pos2::new(split_x + VALUE_BOX_GAP, rect.top()), rect.right_bottom());
            paint_value_box(ui, value_rect, *args.value, args.decimals, args.suffix);
        }
    }
}

/// Shared value-box painter. Themes can call this from their `paint` to
/// inherit the standard right-aligned monospace readout, or roll their own.
pub(crate) fn paint_value_box(ui: &mut Ui, rect: Rect, value: f32, decimals: usize, suffix: &str) {
    let theme = ui.theme();
    let tokens = theme.tokens();
    ui.painter().add(theme.surface(rect, SurfaceKind::Field));
    let text = format_value(value, decimals, suffix);
    let font = theme.font(TextRole::Mono);
    let color = theme.text_color(TextRole::Mono, SurfaceKind::Field, None, PressableState::default());
    let inset_pad = tokens.bevel_inset + 4.0;
    ui.painter().text(
        Pos2::new(rect.right() - inset_pad, rect.center().y),
        Align2::RIGHT_CENTER,
        text,
        font,
        color,
    );
}

/// Text-labelled builder — what callers actually use.
pub struct Slider<'a> {
    value: &'a mut f32,
    range: RangeInclusive<f32>,
    step: Option<f32>,
    suffix: &'static str,
    decimals: usize,
    show_value: bool,
    width: f32,
    value_box_width: f32,
    disabled: bool,
}

#[derive(Debug)]
pub struct SliderResponse {
    pub response: Response,
}

impl std::ops::Deref for SliderResponse {
    type Target = Response;
    fn deref(&self) -> &Response {
        &self.response
    }
}

impl<'a> Slider<'a> {
    pub fn new(value: &'a mut f32, range: RangeInclusive<f32>) -> Self {
        Self {
            value,
            range,
            step: None,
            suffix: "",
            decimals: 0,
            show_value: true,
            width: DEFAULT_WIDTH,
            value_box_width: VALUE_BOX_WIDTH,
            disabled: false,
        }
    }

    /// Snap value to multiples of `step` (in value units, not pixels).
    /// Setting step=0 disables snapping.
    pub fn step(mut self, step: f32) -> Self {
        self.step = if step > 0.0 { Some(step) } else { None };
        self
    }

    pub fn suffix(mut self, suffix: &'static str) -> Self {
        self.suffix = suffix;
        self
    }

    pub fn decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    pub fn show_value(mut self, show: bool) -> Self {
        self.show_value = show;
        self
    }

    /// Width of the track region. Total widget width adds the value box and
    /// gap when `.show_value(true)`.
    pub fn width(mut self, width: f32) -> Self {
        self.width = width.max(DEFAULT_THUMB_DIAMETER * 2.0);
        self
    }

    pub fn value_box_width(mut self, w: f32) -> Self {
        self.value_box_width = w;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> SliderResponse {
        let theme = ui.theme();
        let args = SliderArgs {
            value: self.value,
            range: self.range,
            step: self.step,
            suffix: self.suffix,
            decimals: self.decimals,
            show_value: self.show_value,
            width: self.width,
            value_box_width: self.value_box_width,
            disabled: self.disabled,
        };
        theme.slider().show(ui, args)
    }
}

// -----------------------------------------------------------------------
// Shared helpers (used by DefaultSlider; themes may reuse too).

pub(crate) fn pos_to_norm(x: f32, left: f32, width: f32) -> f32 {
    if width <= 0.0 {
        0.0
    } else {
        ((x - left) / width).clamp(0.0, 1.0)
    }
}

pub(crate) fn snap(value: f32, step: Option<f32>, lo: f32, hi: f32) -> f32 {
    let v = match step {
        Some(s) if s > 0.0 => lo + ((value - lo) / s).round() * s,
        _ => value,
    };
    v.clamp(lo.min(hi), lo.max(hi))
}

pub(crate) fn format_value(value: f32, decimals: usize, suffix: &str) -> String {
    format!("{value:.*}{suffix}", decimals)
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

#[cfg(test)]
mod inner_tests {
    use super::*;

    #[test]
    fn snap_no_step_returns_clamped_value() {
        assert_eq!(snap(5.5, None, 0.0, 10.0), 5.5);
        assert_eq!(snap(-1.0, None, 0.0, 10.0), 0.0);
        assert_eq!(snap(15.0, None, 0.0, 10.0), 10.0);
    }

    #[test]
    fn snap_with_step_rounds_to_nearest() {
        // Step = 1, so 5.4 → 5, 5.6 → 6.
        assert_eq!(snap(5.4, Some(1.0), 0.0, 10.0), 5.0);
        assert_eq!(snap(5.6, Some(1.0), 0.0, 10.0), 6.0);
        // Step = 0.5, lo = -1.0: snaps relative to lo.
        assert_eq!(snap(-0.4, Some(0.5), -1.0, 1.0), -0.5);
    }

    #[test]
    fn snap_preserves_negative_lo() {
        // Range is -20..=20 step 1; -3.4 should snap to -3.
        assert_eq!(snap(-3.4, Some(1.0), -20.0, 20.0), -3.0);
    }

    #[test]
    fn pos_to_norm_clamps_outside_range() {
        assert_eq!(pos_to_norm(5.0, 10.0, 100.0), 0.0);
        assert_eq!(pos_to_norm(60.0, 10.0, 100.0), 0.5);
        assert_eq!(pos_to_norm(200.0, 10.0, 100.0), 1.0);
    }

    #[test]
    fn format_value_uses_decimals_and_suffix() {
        assert_eq!(format_value(3.0, 0, " dB"), "3 dB");
        assert_eq!(format_value(3.0, 2, "%"), "3.00%");
        assert_eq!(format_value(-12.345, 1, ""), "-12.3");
    }
}
