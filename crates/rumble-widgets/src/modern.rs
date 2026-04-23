//! "Modern" theme — the flat/pill aesthetic from the design handoff.
//!
//! Serves as a reference impl of the `Theme` trait.

use eframe::egui::{
    Color32, CornerRadius, FontFamily, FontId, Rect, Shape, Stroke, StrokeKind, Visuals,
    epaint::{RectShape, Shadow},
};

use crate::{
    theme::Theme,
    tokens::{Axis, PressableRole, PressableState, SurfaceKind, TextRole, Tokens},
};

pub struct ModernTheme {
    tokens: Tokens,
}

impl Default for ModernTheme {
    fn default() -> Self {
        Self {
            tokens: Tokens {
                accent: Color32::from_rgb(0x3b, 0x82, 0xf6),
                danger: Color32::from_rgb(0xd6, 0x45, 0x45),
                talking: Color32::from_rgb(0xff, 0x6a, 0x3d),

                surface: Color32::from_rgb(0xff, 0xff, 0xff),
                surface_alt: Color32::from_rgb(0xfa, 0xfa, 0xf9),
                surface_sunken: Color32::from_rgb(0xf2, 0xf2, 0xf0),

                text: Color32::from_rgb(0x1d, 0x1d, 0x1f),
                text_muted: Color32::from_rgb(0x86, 0x86, 0x8b),
                text_on_accent: Color32::WHITE,
                text_on_danger: Color32::WHITE,

                line: Color32::from_rgb(0x2a, 0x2a, 0x2d),
                line_soft: Color32::from_rgb(0xec, 0xec, 0xec),

                radius_sm: 3.0,
                radius_md: 6.0,
                radius_pill: 999.0,

                pad_sm: 4.0,
                pad_md: 8.0,

                bevel_inset: 2.0,

                font_body: FontId::new(13.0, FontFamily::Proportional),
                font_label: FontId::new(11.0, FontFamily::Proportional),
                font_heading: FontId::new(15.0, FontFamily::Proportional),
                font_mono: FontId::new(12.0, FontFamily::Monospace),
            },
        }
    }
}

impl Theme for ModernTheme {
    fn name(&self) -> &'static str {
        "modern"
    }

    fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    fn surface(&self, rect: Rect, kind: SurfaceKind) -> Shape {
        let t = &self.tokens;
        let mut shapes: Vec<Shape> = Vec::new();
        match kind {
            SurfaceKind::Panel => {
                shapes.push(Shape::rect_filled(rect, 0.0, t.surface));
            }
            SurfaceKind::Pane => {
                shapes.push(Shape::rect_filled(rect, 0.0, t.surface_alt));
                shapes.push(Shape::line_segment(
                    [rect.right_top(), rect.right_bottom()],
                    Stroke::new(1.0, t.line_soft),
                ));
            }
            SurfaceKind::Group => {
                shapes.push(Shape::Rect(RectShape::new(
                    rect,
                    CornerRadius::from(t.radius_md),
                    t.surface,
                    Stroke::new(1.0, t.line_soft),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Titlebar | SurfaceKind::Toolbar => {
                shapes.push(Shape::rect_filled(rect, 0.0, t.surface));
                shapes.push(Shape::line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    Stroke::new(1.0, t.line_soft),
                ));
            }
            SurfaceKind::Statusbar => {
                shapes.push(Shape::rect_filled(rect, 0.0, t.surface_alt));
                shapes.push(Shape::line_segment(
                    [rect.left_top(), rect.right_top()],
                    Stroke::new(1.0, t.line_soft),
                ));
            }
            SurfaceKind::Tooltip | SurfaceKind::Popup => {
                shapes.push(Shape::Rect(RectShape::new(
                    rect,
                    CornerRadius::from(t.radius_md),
                    t.surface,
                    Stroke::new(1.0, t.line_soft),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Field => {
                shapes.push(Shape::Rect(RectShape::new(
                    rect,
                    CornerRadius::from(t.radius_sm),
                    t.surface_sunken,
                    Stroke::new(1.0, t.line_soft),
                    StrokeKind::Inside,
                )));
            }
        }
        Shape::Vec(shapes)
    }

    fn pressable(&self, rect: Rect, role: PressableRole, state: PressableState) -> Shape {
        let t = &self.tokens;

        let (fill, stroke, radius) = match (role, state.active) {
            (PressableRole::Default, false) => (t.surface, Some(t.line_soft), t.radius_sm),
            (PressableRole::Default, true) => (t.surface_sunken, Some(t.line_soft), t.radius_sm),

            (PressableRole::Primary, _) => (t.accent, None, t.radius_sm),

            (PressableRole::Danger, false) => (t.surface, Some(t.line_soft), t.radius_sm),
            (PressableRole::Danger, true) => (t.danger, None, t.radius_sm),

            (PressableRole::Accent, false) => (t.surface, Some(t.line_soft), t.radius_pill),
            (PressableRole::Accent, true) => (t.text, None, t.radius_pill),

            (PressableRole::Ghost, false) => (Color32::TRANSPARENT, None, t.radius_pill),
            (PressableRole::Ghost, true) => (t.surface_sunken, None, t.radius_pill),
        };

        let fill = if state.disabled {
            blend(fill, t.surface, 0.6)
        } else if state.pressed {
            darken(fill, 0.08)
        } else if state.hovered {
            if fill == Color32::TRANSPARENT {
                t.surface_sunken
            } else {
                lighten(fill, 0.04)
            }
        } else {
            fill
        };

        let mut shapes: Vec<Shape> = Vec::new();
        if fill != Color32::TRANSPARENT {
            shapes.push(Shape::rect_filled(rect, CornerRadius::from(radius), fill));
        }
        if let Some(stroke_col) = stroke {
            shapes.push(Shape::Rect(RectShape::stroke(
                rect,
                CornerRadius::from(radius),
                Stroke::new(1.0, stroke_col),
                StrokeKind::Inside,
            )));
        }
        if state.focused && !state.disabled {
            shapes.push(Shape::Rect(RectShape::stroke(
                rect.expand(1.5),
                CornerRadius::from(radius + 1.5),
                Stroke::new(1.5, t.accent),
                StrokeKind::Outside,
            )));
        }
        Shape::Vec(shapes)
    }

    fn selection(&self, rect: Rect) -> Shape {
        Shape::rect_filled(
            rect,
            CornerRadius::from(self.tokens.radius_sm),
            Color32::from_rgba_unmultiplied(0x3b, 0x82, 0xf6, 0x22),
        )
    }

    fn separator(&self, rect: Rect, axis: Axis) -> Shape {
        let stroke = Stroke::new(1.0, self.tokens.line_soft);
        match axis {
            Axis::Horizontal => Shape::line_segment([rect.left_center(), rect.right_center()], stroke),
            Axis::Vertical => Shape::line_segment([rect.center_top(), rect.center_bottom()], stroke),
        }
    }

    fn text_color(
        &self,
        _role: TextRole,
        _on: SurfaceKind,
        pressable_role: Option<PressableRole>,
        state: PressableState,
    ) -> Color32 {
        let t = &self.tokens;
        let base = match (pressable_role, state.active) {
            (Some(PressableRole::Primary), _) => t.text_on_accent,
            (Some(PressableRole::Danger), true) => t.text_on_danger,
            (Some(PressableRole::Accent), true) => t.surface,
            _ => t.text,
        };
        if state.disabled {
            blend(base, t.surface, 0.55)
        } else {
            base
        }
    }

    fn font(&self, role: TextRole) -> FontId {
        match role {
            TextRole::Body => self.tokens.font_body.clone(),
            TextRole::Label | TextRole::Caption => self.tokens.font_label.clone(),
            TextRole::Heading => self.tokens.font_heading.clone(),
            TextRole::Mono => self.tokens.font_mono.clone(),
        }
    }

    fn apply_egui_visuals(&self, visuals: &mut Visuals) {
        let t = &self.tokens;
        visuals.window_fill = t.surface;
        visuals.panel_fill = t.surface;
        visuals.window_stroke = Stroke::new(1.0, t.line_soft);
        visuals.window_shadow = Shadow::NONE;
        visuals.override_text_color = Some(t.text);
        visuals.hyperlink_color = t.accent;
        visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(0x3b, 0x82, 0xf6, 0x44);
        visuals.selection.stroke = Stroke::new(1.0, t.accent);
    }
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

fn lighten(c: Color32, t: f32) -> Color32 {
    blend(c, Color32::WHITE, t)
}

fn darken(c: Color32, t: f32) -> Color32 {
    blend(c, Color32::BLACK, t)
}
