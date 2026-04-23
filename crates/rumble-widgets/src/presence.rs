//! `UserPresence` — avatar + name + voice-state badges.
//!
//! `UserPresence` is a thin builder facade that collects args and dispatches
//! to the theme's `PresenceImpl`. Themes can swap the impl to fully re-skin
//! the widget — circular vs square avatar, badge style, talking-ring
//! treatment — not just colors. See `DefaultPresence` for the shared
//! baseline (round avatar, circular badges, breathing ring) and
//! `LunaPresence` for an XP-styled bevelled-square avatar with flat badges.
//!
//! ## Animation
//!
//! When `talking` is set, the impl animates a breathing ring on its own
//! and asks egui for a follow-up repaint. Multiple animated `UserPresence`
//! instances in one frame collapse to a single egui repaint request, so
//! the cost is flat regardless of how many are visible.

use std::time::Duration;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, epaint::CircleShape};

use crate::{theme::UiExt, tokens::TextRole};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UserState {
    pub talking: bool,
    /// Self-muted by the user.
    pub muted: bool,
    /// Self-deafened by the user.
    pub deafened: bool,
    /// Muted by the server (admin action).
    pub server_muted: bool,
    pub away: bool,
}

/// Builder arguments, passed to `PresenceImpl`.
pub struct PresenceArgs {
    pub name: Option<String>,
    pub state: UserState,
    pub size: f32,
}

/// Theme-provided implementation of the UserPresence visual + layout.
///
/// Themes override `layout` and `paint`. The default `show()` allocates,
/// senses hover, then hands off to `paint` — there is no value-mutation
/// for this widget; it's display-only.
pub trait PresenceImpl: Send + Sync + 'static {
    /// Total widget size (avatar + optional name label).
    fn layout(&self, ui: &Ui, args: &PresenceArgs) -> Vec2;

    /// Paint the widget inside `rect`. Responsible for the avatar slot,
    /// the talking ring (if `args.state.talking`), the badges, the name,
    /// and the away overlay.
    fn paint(&self, ui: &mut Ui, rect: Rect, args: &PresenceArgs);

    /// Allocate, sense hover, then hand off to `paint`.
    fn show(&self, ui: &mut Ui, args: PresenceArgs) -> Response {
        let size = self.layout(ui, &args);
        let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
        self.paint(ui, rect, &args);
        response
    }
}

/// Text-labelled builder — what callers actually use.
pub struct UserPresence {
    name: Option<String>,
    state: UserState,
    size: f32,
}

impl Default for UserPresence {
    fn default() -> Self {
        Self::new()
    }
}

impl UserPresence {
    pub fn new() -> Self {
        Self {
            name: None,
            state: UserState::default(),
            size: 32.0,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Replace the entire state at once.
    pub fn state(mut self, state: UserState) -> Self {
        self.state = state;
        self
    }

    pub fn talking(mut self, b: bool) -> Self {
        self.state.talking = b;
        self
    }
    pub fn muted(mut self, b: bool) -> Self {
        self.state.muted = b;
        self
    }
    pub fn deafened(mut self, b: bool) -> Self {
        self.state.deafened = b;
        self
    }
    pub fn server_muted(mut self, b: bool) -> Self {
        self.state.server_muted = b;
        self
    }
    pub fn away(mut self, b: bool) -> Self {
        self.state.away = b;
        self
    }

    /// Avatar diameter in points. Name (if present) sits to the right at
    /// body-text size.
    pub fn size(mut self, px: f32) -> Self {
        self.size = px;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.theme();
        let args = PresenceArgs {
            name: self.name,
            state: self.state,
            size: self.size,
        };
        theme.presence().show(ui, args)
    }
}

/// The static `PresenceImpl` every theme gets by default — preserves the
/// pre-refactor look: round avatar with a thin outline, circular badges,
/// breathing talking ring.
pub struct DefaultPresence;
pub(crate) static DEFAULT_PRESENCE: DefaultPresence = DefaultPresence;

impl PresenceImpl for DefaultPresence {
    fn layout(&self, ui: &Ui, args: &PresenceArgs) -> Vec2 {
        let theme = ui.theme();
        let tokens = theme.tokens();
        let font_body = theme.font(TextRole::Body);
        let pad_x = tokens.pad_sm;

        let name_size = args.name.as_deref().map(|n| {
            let galley = ui
                .ctx()
                .fonts_mut(|f| f.layout_no_wrap(n.to_string(), font_body.clone(), tokens.text));
            galley.rect.size()
        });

        let total_w = args.size + name_size.map(|s| pad_x + s.x).unwrap_or(0.0);
        let total_h = args.size.max(name_size.map(|s| s.y).unwrap_or(0.0));
        Vec2::new(total_w, total_h)
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, args: &PresenceArgs) {
        let theme = ui.theme();
        let tokens = theme.tokens();
        let font_body = theme.font(TextRole::Body);
        let pad_x = tokens.pad_sm;

        let avatar_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - args.size * 0.5),
            Vec2::splat(args.size),
        );
        let center = avatar_rect.center();
        let radius = args.size * 0.5;

        // Avatar: filled disk + thin outline.
        ui.painter()
            .add(CircleShape::filled(center, radius, tokens.surface_alt));
        ui.painter()
            .add(CircleShape::stroke(center, radius, Stroke::new(1.0, tokens.line_soft)));

        // Initial in the center of the avatar.
        if let Some(name) = args.name.as_deref()
            && let Some(ch) = name.chars().next()
        {
            let initial = ch.to_uppercase().to_string();
            let mut font = font_body.clone();
            font.size = (args.size * 0.45).max(1.0);
            ui.painter()
                .text(center, Align2::CENTER_CENTER, initial, font, tokens.text_muted);
        }

        // Outer ring: server-muted wins (red, solid); otherwise pulse if
        // talking.
        if args.state.server_muted {
            ui.painter().add(CircleShape::stroke(
                center,
                radius + 2.5,
                Stroke::new(2.0, tokens.danger),
            ));
        } else if args.state.talking {
            let t = ui.input(|i| i.time as f32);
            let pulse = (t * std::f32::consts::TAU * 1.6).sin() * 0.5 + 0.5;
            let alpha = 140 + (pulse * 115.0) as u8;
            let c = tokens.talking;
            let ring = Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), alpha);
            ui.painter()
                .add(CircleShape::stroke(center, radius + 2.5, Stroke::new(2.0, ring)));
            ui.ctx().request_repaint_after(Duration::from_millis(33));
        }

        // Bottom-right badges.
        let badge_size = (args.size * 0.42).max(10.0);
        let mut badge_x = avatar_rect.right() - badge_size * 0.5 + 1.0;
        let badge_y = avatar_rect.bottom() - badge_size * 0.5 + 1.0;

        let muted = args.state.muted || args.state.server_muted;
        if muted {
            paint_circular_badge(
                ui,
                Pos2::new(badge_x, badge_y),
                badge_size,
                tokens.danger,
                Color32::WHITE,
                'M',
                font_body.clone(),
            );
            badge_x -= badge_size + 1.0;
        }
        if args.state.deafened {
            paint_circular_badge(
                ui,
                Pos2::new(badge_x, badge_y),
                badge_size,
                Color32::from_rgb(0x44, 0x44, 0x88),
                Color32::WHITE,
                'D',
                font_body.clone(),
            );
        }

        // Name to the right of the avatar — placed via ui.put so it
        // registers in accesskit (kittest can find it by label).
        if let Some(name) = args.name.clone() {
            let text_color = if args.state.away {
                tokens.text_muted
            } else {
                tokens.text
            };
            let name_rect = Rect::from_min_max(Pos2::new(avatar_rect.right() + pad_x, rect.top()), rect.max);
            ui.put(
                name_rect,
                egui::Label::new(egui::RichText::new(name).font(font_body).color(text_color)).selectable(false),
            );
        }

        // Away: dim the whole row with a translucent overlay last.
        if args.state.away {
            ui.painter().rect_filled(
                rect,
                0.0,
                Color32::from_rgba_unmultiplied(tokens.surface.r(), tokens.surface.g(), tokens.surface.b(), 110),
            );
        }
    }
}

fn paint_circular_badge(
    ui: &mut Ui,
    center: Pos2,
    size: f32,
    fill: Color32,
    fg: Color32,
    glyph: char,
    mut font: FontId,
) {
    let r = size * 0.5;
    font.size = (size * 0.72).max(1.0);
    ui.painter().add(CircleShape::filled(center, r, fill));
    ui.painter().add(CircleShape::stroke(
        center,
        r,
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(0, 0, 0, 90)),
    ));
    ui.painter()
        .text(center, Align2::CENTER_CENTER, glyph.to_string(), font, fg);
}

// -----------------------------------------------------------------------
// Shared helpers (used by DefaultPresence; themes may reuse too).

/// Measure the full row size given `args` — avatar diameter + (gap +
/// name galley width) if a name is set. Themes that don't change the
/// layout shape can reuse this from their `layout`.
pub(crate) fn measure_default_layout(ui: &Ui, args: &PresenceArgs) -> Vec2 {
    let theme = ui.theme();
    let tokens = theme.tokens();
    let font_body = theme.font(TextRole::Body);
    let pad_x = tokens.pad_sm;

    let name_size = args.name.as_deref().map(|n| {
        let galley = ui
            .ctx()
            .fonts_mut(|f| f.layout_no_wrap(n.to_string(), font_body.clone(), tokens.text));
        galley.rect.size()
    });
    let total_w = args.size + name_size.map(|s| pad_x + s.x).unwrap_or(0.0);
    let total_h = args.size.max(name_size.map(|s| s.y).unwrap_or(0.0));
    Vec2::new(total_w, total_h)
}

/// Place the name label inside `name_rect` via `ui.put` so it registers
/// in accesskit. Themes share this so kittest's `get_by_label` keeps
/// working regardless of theme.
pub(crate) fn paint_name(ui: &mut Ui, name_rect: Rect, name: &str, color: Color32) {
    let theme = ui.theme();
    let font_body = theme.font(TextRole::Body);
    ui.put(
        name_rect,
        egui::Label::new(egui::RichText::new(name).font(font_body).color(color)).selectable(false),
    );
}
