//! Continuous level / VU meter for audio I/O.
//!
//! `LevelMeter` is a thin builder facade that collects args and dispatches
//! to the theme's `LevelMeterImpl`. Themes can swap the impl to fully
//! re-skin the bar — smooth fill vs segmented LED bricks vs needle —
//! not just colors. See `DefaultLevelMeter` for the shared baseline
//! (smooth zone fill on a Field surface) and `LunaLevelMeter` for a
//! segmented LED-style meter that reads as vintage hardware.
//!
//! The widget is render-only: the caller passes a normalized `level` in
//! `[0.0, 1.0]` each frame and (optionally) a peak-hold value plus any
//! number of color zones and marker lines. To animate at audio rate, the
//! caller asks egui to repaint — the widget itself does not schedule
//! repaints, so multiple meters in one frame don't compound and a static
//! meter is free.
//!
//! With `.interactive(true)` the widget reports the `[0.0, 1.0]` position
//! under the pointer during click+drag, suitable for adjusting a VAD
//! threshold.
//!
//! ## Customizing zones and markers
//!
//! By default the bar paints in three segments — green up to 0.7, yellow
//! up to 0.9, then the theme's `talking` color — with no markers. Override
//! either piece via `.zones(...)` and `.markers(...)`. For the common
//! Mumble-style VAD case (silent + speech thresholds with matching color
//! transitions), `.vad(silent, speech)` is sugar that sets both at once
//! using theme tokens.

use eframe::egui::{Color32, CornerRadius, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};

use crate::{
    theme::UiExt,
    tokens::{Axis, SurfaceKind},
};

pub(crate) const VU_GREEN: Color32 = Color32::from_rgb(0x4a, 0xa8, 0x4a);
pub(crate) const VU_YELLOW: Color32 = Color32::from_rgb(0xe0, 0xb8, 0x40);

/// Builder arguments, passed to `LevelMeterImpl`.
pub struct LevelMeterArgs {
    pub level: f32,
    /// Override for color zones. Each entry is the upper bound of that
    /// zone in `[0.0, 1.0]` and its color; the lower bound is the previous
    /// entry's upper or 0.0 for the first.
    pub zones: Option<Vec<(f32, Color32)>>,
    /// Marker lines drawn over the bar (level, color).
    pub markers: Vec<(f32, Color32)>,
    /// Sugar: a single accent-colored marker. Resolved at paint time so
    /// the color tracks the installed theme.
    pub threshold: Option<f32>,
    /// Sugar: VAD silent + speech thresholds. Drives both the default
    /// zones and two accent-colored markers; each piece can be overridden
    /// by an explicit `zones` / `markers` value.
    pub vad: Option<(f32, f32)>,
    pub peak: Option<f32>,
    pub orientation: Axis,
    pub min_size: Vec2,
    pub interactive: bool,
}

/// Theme-provided implementation of the LevelMeter visual.
///
/// Themes override `paint`. The default `show()` allocates, senses input
/// (when interactive), computes the `threshold_drag` value, then hands
/// off to `paint` — there is no value-mutation; the meter is display-only.
pub trait LevelMeterImpl: Send + Sync + 'static {
    /// Paint the meter inside `rect`. Responsible for the background, the
    /// filled portion (up to `args.level`), peak tick, and markers.
    fn paint(&self, ui: &mut Ui, rect: Rect, args: &LevelMeterArgs);

    /// Allocate, sense pointer (when interactive), then hand off to
    /// `paint`. Themes can override for unusual interaction; the common
    /// path lives here.
    fn show(&self, ui: &mut Ui, args: LevelMeterArgs) -> LevelMeterResponse {
        let sense = if args.interactive {
            Sense::click_and_drag()
        } else {
            Sense::hover()
        };
        let (rect, response) = ui.allocate_exact_size(args.min_size, sense);

        self.paint(ui, rect, &args);

        let threshold_drag = if args.interactive {
            response
                .interact_pointer_pos()
                .map(|p| pos_to_level(rect, p, args.orientation))
        } else {
            None
        };

        LevelMeterResponse {
            response,
            threshold_drag,
        }
    }
}

/// The static `LevelMeterImpl` every theme gets by default — preserves
/// the pre-refactor look: smooth zone fill on a sunken Field surface
/// with peak tick and marker lines.
pub struct DefaultLevelMeter;
pub(crate) static DEFAULT_LEVEL_METER: DefaultLevelMeter = DefaultLevelMeter;

impl LevelMeterImpl for DefaultLevelMeter {
    fn paint(&self, ui: &mut Ui, rect: Rect, args: &LevelMeterArgs) {
        let theme = ui.theme();
        let tokens = theme.tokens();

        // Sunken Field background — themed.
        ui.painter().add(theme.surface(rect, SurfaceKind::Field));

        let inner = rect.shrink(tokens.bevel_inset);
        let zones = resolve_zones(args.zones.as_deref(), args.vad, tokens.talking);

        // Paint zones in segments only up to `level`.
        let mut prev = 0.0f32;
        for (boundary, color) in &zones {
            let boundary = boundary.clamp(0.0, 1.0);
            if args.level <= prev {
                break;
            }
            let zone_end = args.level.min(boundary);
            let seg = zone_rect(inner, prev, zone_end, args.orientation);
            if seg.area() > 0.0 {
                ui.painter().add(Shape::rect_filled(seg, CornerRadius::ZERO, *color));
            }
            prev = boundary;
        }

        if let Some(peak) = args.peak {
            paint_marker(
                ui,
                inner,
                peak,
                args.orientation,
                Stroke::new(1.5, Color32::from_rgba_unmultiplied(255, 255, 255, 220)),
            );
        }

        let markers = resolve_markers(&args.markers, args.threshold, args.vad, tokens.accent);
        for (level, color) in markers {
            paint_marker(ui, rect, level, args.orientation, Stroke::new(2.0, color));
        }
    }
}

pub struct LevelMeter {
    level: f32,
    zones: Option<Vec<(f32, Color32)>>,
    markers: Vec<(f32, Color32)>,
    threshold: Option<f32>,
    vad: Option<(f32, f32)>,
    peak: Option<f32>,
    orientation: Axis,
    min_size: Vec2,
    interactive: bool,
}

#[derive(Debug)]
pub struct LevelMeterResponse {
    pub response: Response,
    /// If the user is dragging or clicking the meter (and `interactive` was
    /// set), the `[0.0, 1.0]` position under the pointer along the meter's
    /// axis. `None` otherwise.
    pub threshold_drag: Option<f32>,
}

/// Forward `Response` methods (`.clicked()`, `.hovered()`, `.rect`, ...)
/// to the inner response so call sites don't need to write
/// `resp.response.clicked()`.
impl std::ops::Deref for LevelMeterResponse {
    type Target = Response;
    fn deref(&self) -> &Response {
        &self.response
    }
}

impl LevelMeter {
    pub fn new(level: f32) -> Self {
        Self {
            level: level.clamp(0.0, 1.0),
            zones: None,
            markers: Vec::new(),
            threshold: None,
            vad: None,
            peak: None,
            orientation: Axis::Horizontal,
            min_size: Vec2::new(120.0, 12.0),
            interactive: false,
        }
    }

    /// Replace the default zones with custom ones. Each entry is
    /// `(upper_bound, color)`; the first zone starts at 0.0 and each
    /// subsequent zone starts where the previous one ended. Boundaries
    /// must be ascending; values outside `[0.0, 1.0]` are clamped at
    /// render time.
    pub fn zones(mut self, zones: Vec<(f32, Color32)>) -> Self {
        self.zones = Some(zones);
        self
    }

    /// Replace the marker list. Each entry is `(level, color)` for a line
    /// drawn across the full meter width at that level.
    pub fn markers(mut self, markers: Vec<(f32, Color32)>) -> Self {
        self.markers = markers;
        self
    }

    /// Sugar for adding a single accent-colored marker line.
    pub fn threshold(mut self, t: f32) -> Self {
        self.threshold = Some(t.clamp(0.0, 1.0));
        self
    }

    /// VAD-style three-zone meter: green below `silent`, yellow between
    /// `silent` and `speech`, theme's `talking` color above. Also adds two
    /// accent-colored marker lines at the thresholds. Either piece can be
    /// overridden by a later `.zones()` or `.markers()` call.
    pub fn vad(mut self, silent: f32, speech: f32) -> Self {
        let silent = silent.clamp(0.0, 1.0);
        let speech = speech.clamp(silent, 1.0);
        self.vad = Some((silent, speech));
        self
    }

    pub fn peak(mut self, p: f32) -> Self {
        self.peak = Some(p.clamp(0.0, 1.0));
        self
    }

    pub fn orientation(mut self, axis: Axis) -> Self {
        if axis == Axis::Vertical && self.min_size == Vec2::new(120.0, 12.0) {
            self.min_size = Vec2::new(12.0, 120.0);
        }
        self.orientation = axis;
        self
    }

    pub fn min_size(mut self, size: Vec2) -> Self {
        self.min_size = size;
        self
    }

    pub fn interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    pub fn show(self, ui: &mut Ui) -> LevelMeterResponse {
        let theme = ui.theme();
        let args = LevelMeterArgs {
            level: self.level,
            zones: self.zones,
            markers: self.markers,
            threshold: self.threshold,
            vad: self.vad,
            peak: self.peak,
            orientation: self.orientation,
            min_size: self.min_size,
            interactive: self.interactive,
        };
        theme.level_meter().show(ui, args)
    }
}

// -----------------------------------------------------------------------
// Shared helpers (used by DefaultLevelMeter; themes may reuse too).

pub(crate) fn resolve_zones(
    explicit: Option<&[(f32, Color32)]>,
    vad: Option<(f32, f32)>,
    talking: Color32,
) -> Vec<(f32, Color32)> {
    if let Some(z) = explicit {
        return z.to_vec();
    }
    if let Some((silent, speech)) = vad {
        return vec![(silent, VU_GREEN), (speech, VU_YELLOW), (1.0, talking)];
    }
    vec![(0.70, VU_GREEN), (0.90, VU_YELLOW), (1.0, talking)]
}

pub(crate) fn resolve_markers(
    explicit: &[(f32, Color32)],
    threshold: Option<f32>,
    vad: Option<(f32, f32)>,
    accent: Color32,
) -> Vec<(f32, Color32)> {
    let mut out = explicit.to_vec();
    if let Some(t) = threshold {
        out.push((t, accent));
    }
    if let Some((silent, speech)) = vad {
        out.push((silent, accent));
        out.push((speech, accent));
    }
    out
}

pub(crate) fn zone_rect(inner: Rect, lo: f32, hi: f32, axis: Axis) -> Rect {
    match axis {
        Axis::Horizontal => Rect::from_min_max(
            Pos2::new(inner.left() + inner.width() * lo, inner.top()),
            Pos2::new(inner.left() + inner.width() * hi, inner.bottom()),
        ),
        Axis::Vertical => Rect::from_min_max(
            Pos2::new(inner.left(), inner.bottom() - inner.height() * hi),
            Pos2::new(inner.right(), inner.bottom() - inner.height() * lo),
        ),
    }
}

pub(crate) fn paint_marker(ui: &mut Ui, rect: Rect, level: f32, axis: Axis, stroke: Stroke) {
    let level = level.clamp(0.0, 1.0);
    match axis {
        Axis::Horizontal => {
            let x = rect.left() + rect.width() * level;
            ui.painter()
                .line_segment([Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())], stroke);
        }
        Axis::Vertical => {
            let y = rect.bottom() - rect.height() * level;
            ui.painter()
                .line_segment([Pos2::new(rect.left(), y), Pos2::new(rect.right(), y)], stroke);
        }
    }
}

pub(crate) fn pos_to_level(inner: Rect, p: Pos2, axis: Axis) -> f32 {
    match axis {
        Axis::Horizontal => ((p.x - inner.left()) / inner.width()).clamp(0.0, 1.0),
        Axis::Vertical => ((inner.bottom() - p.y) / inner.height()).clamp(0.0, 1.0),
    }
}

#[cfg(test)]
mod inner_tests {
    use super::*;
    use eframe::egui::pos2;

    #[test]
    fn pos_to_level_horizontal() {
        let inner = Rect::from_min_max(pos2(10.0, 10.0), pos2(110.0, 30.0));
        assert!((pos_to_level(inner, pos2(60.0, 20.0), Axis::Horizontal) - 0.5).abs() < 1e-4);
        assert_eq!(pos_to_level(inner, pos2(0.0, 20.0), Axis::Horizontal), 0.0);
        assert_eq!(pos_to_level(inner, pos2(200.0, 20.0), Axis::Horizontal), 1.0);
    }

    #[test]
    fn pos_to_level_vertical_inverts_y() {
        let inner = Rect::from_min_max(pos2(0.0, 0.0), pos2(20.0, 100.0));
        let top = pos_to_level(inner, pos2(10.0, 0.0), Axis::Vertical);
        let bot = pos_to_level(inner, pos2(10.0, 100.0), Axis::Vertical);
        assert!((top - 1.0).abs() < 1e-4, "top of bar = level 1.0, got {top}");
        assert!(bot.abs() < 1e-4, "bottom of bar = level 0.0, got {bot}");
    }

    #[test]
    fn zone_rect_horizontal_slices() {
        let inner = Rect::from_min_max(pos2(0.0, 0.0), pos2(100.0, 10.0));
        let r = zone_rect(inner, 0.7, 0.9, Axis::Horizontal);
        assert_eq!(r.left(), 70.0);
        assert_eq!(r.right(), 90.0);
        assert_eq!(r.top(), 0.0);
        assert_eq!(r.bottom(), 10.0);
    }

    #[test]
    fn zone_rect_vertical_grows_upward() {
        let inner = Rect::from_min_max(pos2(0.0, 0.0), pos2(10.0, 100.0));
        let r = zone_rect(inner, 0.0, 0.5, Axis::Vertical);
        assert_eq!(r.bottom(), 100.0);
        assert_eq!(r.top(), 50.0);
    }

    #[test]
    fn resolve_zones_default() {
        let z = resolve_zones(None, None, Color32::RED);
        assert_eq!(z.len(), 3);
        assert_eq!(z[0].0, 0.70);
        assert_eq!(z[1].0, 0.90);
        assert_eq!(z[2], (1.0, Color32::RED));
    }

    #[test]
    fn resolve_zones_vad_drives_boundaries() {
        let z = resolve_zones(None, Some((0.3, 0.65)), Color32::from_rgb(1, 2, 3));
        assert_eq!(z.len(), 3);
        assert_eq!(z[0], (0.3, VU_GREEN));
        assert_eq!(z[1], (0.65, VU_YELLOW));
        assert_eq!(z[2], (1.0, Color32::from_rgb(1, 2, 3)));
    }

    #[test]
    fn resolve_zones_explicit_overrides_vad() {
        let custom = vec![(0.5, Color32::BLUE), (1.0, Color32::WHITE)];
        let z = resolve_zones(Some(&custom), Some((0.3, 0.65)), Color32::RED);
        assert_eq!(z, custom, "explicit zones must win over VAD sugar");
    }

    #[test]
    fn resolve_markers_combines_explicit_threshold_vad() {
        let explicit = vec![(0.1, Color32::BLACK)];
        let m = resolve_markers(&explicit, Some(0.4), Some((0.5, 0.8)), Color32::GREEN);
        assert_eq!(m.len(), 4);
        assert_eq!(m[0], (0.1, Color32::BLACK));
        assert_eq!(m[1], (0.4, Color32::GREEN));
        assert_eq!(m[2], (0.5, Color32::GREEN));
        assert_eq!(m[3], (0.8, Color32::GREEN));
    }

    #[test]
    fn vad_clamps_speech_above_silent() {
        let m = LevelMeter::new(0.5).vad(0.6, 0.2);
        assert_eq!(m.vad, Some((0.6, 0.6)));
    }
}
