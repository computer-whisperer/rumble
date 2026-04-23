use std::sync::Arc;

use eframe::egui::{Color32, Context, FontId, Id, Rect, Shape, Style, Ui, Visuals};

use crate::{
    level_meter::{DEFAULT_LEVEL_METER, LevelMeterImpl},
    presence::{DEFAULT_PRESENCE, PresenceImpl},
    slider::{DEFAULT_SLIDER, SliderImpl},
    toggle::{DEFAULT_TOGGLE, ToggleImpl},
    tokens::{Axis, PressableRole, PressableState, SurfaceKind, TextRole, Tokens},
    tree::{DEFAULT_TREE, TreeImpl},
};

/// A theme is a bundle of shape-building operations. It does not own widgets,
/// layout, or interaction — those live in the widgets themselves.
///
/// Methods return `Shape` instead of drawing directly, so that containers
/// (like `SurfaceFrame`) can paint a backdrop *after* their content has
/// already determined its size.
pub trait Theme: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn tokens(&self) -> &Tokens;

    fn surface(&self, rect: Rect, kind: SurfaceKind) -> Shape;

    fn pressable(&self, rect: Rect, role: PressableRole, state: PressableState) -> Shape;

    fn selection(&self, rect: Rect) -> Shape;

    fn separator(&self, rect: Rect, axis: Axis) -> Shape;

    fn text_color(
        &self,
        role: TextRole,
        on: SurfaceKind,
        pressable_role: Option<PressableRole>,
        state: PressableState,
    ) -> Color32;

    fn font(&self, role: TextRole) -> FontId;

    /// Tweak egui's built-in `Visuals` so wrapped built-ins (TextEdit,
    /// ScrollArea, etc.) blend with this theme.
    fn apply_egui_visuals(&self, visuals: &mut Visuals);

    /// Tweak egui's built-in `Style` (spacing, scroll geometry, etc.) so
    /// wrapped built-ins blend with this theme. Default is a no-op;
    /// themes that want chunky scrollbars or different paddings override.
    fn apply_egui_style(&self, _style: &mut Style) {}

    /// Widget-level implementation of Toggle. Themes override this to
    /// change more than colors — shape, focus treatment, indicator glyph,
    /// etc. The default is `DefaultToggle`, a switch-or-checkbox that uses
    /// this theme's tokens.
    fn toggle(&self) -> &dyn ToggleImpl {
        &DEFAULT_TOGGLE
    }

    /// Widget-level implementation of Slider. Themes override this to
    /// change track shape, thumb glyph, focus treatment, etc. The default
    /// is `DefaultSlider` — circular thumb on a rounded sunken track with
    /// an accent-coloured fill.
    fn slider(&self) -> &dyn SliderImpl {
        &DEFAULT_SLIDER
    }

    /// Widget-level implementation of UserPresence. Themes override this
    /// to change avatar shape, badge style, talking-ring treatment, etc.
    /// The default is `DefaultPresence` — round avatar with circular
    /// badges and a breathing talking ring.
    fn presence(&self) -> &dyn PresenceImpl {
        &DEFAULT_PRESENCE
    }

    /// Widget-level implementation of Tree. Themes override paint
    /// primitives (caret glyph, row chrome, drop indicator); the default
    /// `show()` owns flattening, hit-testing, keyboard nav, and
    /// drag/drop. The default is `DefaultTree` — rotated-triangle caret
    /// with the theme's `selection()` shape for highlight.
    fn tree(&self) -> &dyn TreeImpl {
        &DEFAULT_TREE
    }

    /// Widget-level implementation of LevelMeter. Themes override this
    /// to swap the bar style — smooth fill, segmented LEDs, needle, etc.
    /// The default is `DefaultLevelMeter` — smooth zone fill on a sunken
    /// Field surface with peak tick and marker lines.
    fn level_meter(&self) -> &dyn LevelMeterImpl {
        &DEFAULT_LEVEL_METER
    }

    /// Color of a `GroupBox`'s title text. Default is `tokens.text`;
    /// themes with a distinct legend treatment (Luna's XP blue, etc.)
    /// override.
    fn group_title_color(&self) -> Color32 {
        self.tokens().text
    }

    /// Paint a radio-button indicator (circle + optional inner dot).
    /// Default paints a flat themed circle; themes like Luna override
    /// for XP-authentic bevel geometry.
    fn radio_indicator(&self, rect: Rect, selected: bool, state: PressableState) -> Shape {
        crate::radio::default_radio_indicator(self.tokens(), rect, selected, state)
    }
}

const THEME_KEY: Id = Id::NULL;

#[derive(Clone)]
struct ThemeSlot(Arc<dyn Theme>);

/// Install a theme into the egui context. Call once on app start (and any
/// time the user picks a new theme).
pub fn install_theme(ctx: &Context, theme: Arc<dyn Theme>) {
    let mut style = (*ctx.style()).clone();
    theme.apply_egui_visuals(&mut style.visuals);
    theme.apply_egui_style(&mut style);
    ctx.set_style(style);
    ctx.data_mut(|d| d.insert_temp(THEME_KEY, ThemeSlot(theme)));
}

/// Extension trait for reading the installed theme off a `Ui`.
pub trait UiExt {
    fn theme(&self) -> Arc<dyn Theme>;
}

impl UiExt for Ui {
    fn theme(&self) -> Arc<dyn Theme> {
        self.ctx()
            .data(|d| d.get_temp::<ThemeSlot>(THEME_KEY))
            .expect("rumble_widgets: no theme installed — call install_theme()")
            .0
    }
}
