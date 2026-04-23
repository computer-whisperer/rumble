//! "Luna" theme — Windows XP blue. Modeled after the XP.css reference
//! (reference/XP.css/themes/XP/*.scss) to get the chrome right:
//! 3-stop button gradient, 4-stop pressed gradient, asymmetric orange hover
//! and blue focus inner-glows, flat borders on fields/fieldsets, 8-stop
//! titlebar.
//!
//! Ships light and dark variants via [`LunaPalette`]. Every hardcoded
//! color and gradient stop lives on the palette struct, so a dark variant
//! is just a different `&'static LunaPalette` — no paint code duplicated.

use eframe::egui::{
    Color32, CornerRadius, FontFamily, FontId, Pos2, Rect, Shape, Stroke, StrokeKind, Style, TextWrapMode, Ui, Vec2,
    Visuals,
    emath::GuiRounding,
    epaint::{RectShape, Shadow},
    pos2,
};

use crate::{
    level_meter::{LevelMeterArgs, LevelMeterImpl, paint_marker, resolve_markers, resolve_zones},
    presence::{PresenceArgs, PresenceImpl, measure_default_layout, paint_name},
    slider::{SliderArgs, SliderImpl, SliderState, VALUE_BOX_GAP, paint_value_box},
    theme::{Theme, UiExt},
    toggle::{LABEL_GAP, ToggleArgs, ToggleImpl, ToggleState, blend as toggle_blend, paint_label_accessible},
    tokens::{Axis, PressableRole, PressableState, SurfaceKind, TextRole, Tokens},
    tree::TreeImpl,
};

/// Per-edge inset-shadow layer. Stored flat because XP buttons have
/// different top / bottom / left / right thicknesses — that asymmetry is
/// what gives the chrome its iconic "lit from top-right" look.
pub struct GlowLayer {
    pub top: f32,
    pub bottom: f32,
    pub left: f32,
    pub right: f32,
    pub color: Color32,
}

pub struct LunaPalette {
    // ----- Tokens-driving colors -----
    pub accent: Color32,
    pub danger: Color32,
    pub talking: Color32,
    pub surface: Color32,
    pub surface_alt: Color32,
    pub surface_sunken: Color32,
    pub text: Color32,
    pub text_muted: Color32,
    pub text_on_accent: Color32,
    pub text_on_danger: Color32,
    pub line: Color32,
    pub line_soft: Color32,

    // ----- Surface chrome -----
    pub pane_border: Color32,
    pub group_fill: Color32,
    pub group_border: Color32,
    pub field_fill: Color32,
    pub field_border: Color32,
    pub popup_fill: Color32,
    pub popup_border: Color32,
    pub tooltip_fill: Color32,
    pub tooltip_border: Color32,

    // ----- Titlebar / toolbars -----
    pub titlebar_stops: &'static [(f32, Color32)],
    pub titlebar_border: Color32,
    pub toolbar_stops: &'static [(f32, Color32)],
    pub statusbar_stops: &'static [(f32, Color32)],

    // ----- Button / Pressable -----
    pub button_border: Color32,
    pub button_rest_stops: &'static [(f32, Color32)],
    pub button_pressed_stops: &'static [(f32, Color32)],
    pub hover_layers: &'static [GlowLayer],
    pub focus_layers: &'static [GlowLayer],
    pub primary_stops: &'static [(f32, Color32)],
    pub primary_border: Color32,
    pub danger_stops: &'static [(f32, Color32)],
    pub danger_border: Color32,
    pub accent_active_stops: &'static [(f32, Color32)],

    // ----- Selection / separator / scrollbar -----
    pub selection_fill: Color32,
    pub separator_dark: Color32,
    pub separator_light: Color32,
    pub scroll_track: Color32,
    pub scroll_handle_rest: Color32,
    pub scroll_handle_hover: Color32,
    pub scroll_handle_active: Color32,

    // ----- Checkbox (LunaToggle) -----
    pub check_border: Color32,
    pub check_border_disabled: Color32,
    pub check_color: Color32,
    pub check_color_disabled: Color32,
    pub check_fill_top: Color32,
    pub check_fill_bottom: Color32,
    pub check_fill_pressed_top: Color32,
    pub check_fill_pressed_bottom: Color32,
    pub check_fill_disabled: Color32,
    pub check_hover_amber: Color32,
    pub check_hover_cream_gold: Color32,

    // ----- Slider -----
    pub track_fill: Color32,
    pub track_shadow: Color32,
    pub track_highlight: Color32,
    pub thumb_outline: Color32,
    pub thumb_body_fill: Color32,
    pub thumb_body_highlight: Color32,
    pub thumb_body_shadow: Color32,
    pub thumb_accent: Color32,

    // ----- Presence -----
    pub avatar_bg: Color32,
    pub avatar_highlight: Color32,
    pub avatar_shadow: Color32,
    pub badge_muted: Color32,
    pub badge_deafened: Color32,
    pub badge_text: Color32,
    pub badge_border: Color32,
    pub talking_ring: Color32,

    // ----- Tree -----
    pub caret_fill: Color32,
    pub caret_border: Color32,
    pub caret_glyph: Color32,

    // ----- Focus ring (dotted-outline substitute) -----
    pub focus_ring: Color32,

    // ----- Group-box legend text -----
    pub legend: Color32,
}

// Shared glow slices — light and dark both use the same orange-hover and
// blue-focus layers. Glow colors are mid-brightness, so they read on
// cream OR charcoal button fills without adjustment.
const XP_HOVER_LAYERS: &[GlowLayer] = &[
    GlowLayer {
        top: 1.0,
        bottom: 0.0,
        left: 0.0,
        right: 1.0,
        color: Color32::from_rgb(0xff, 0xf0, 0xcf),
    },
    GlowLayer {
        top: 2.0,
        bottom: 0.0,
        left: 1.0,
        right: 0.0,
        color: Color32::from_rgb(0xfd, 0xd8, 0x89),
    },
    GlowLayer {
        top: 2.0,
        bottom: 0.0,
        left: 0.0,
        right: 2.0,
        color: Color32::from_rgb(0xfb, 0xc7, 0x61),
    },
    GlowLayer {
        top: 0.0,
        bottom: 2.0,
        left: 2.0,
        right: 0.0,
        color: Color32::from_rgb(0xe5, 0xa0, 0x1a),
    },
];

const XP_FOCUS_LAYERS: &[GlowLayer] = &[
    GlowLayer {
        top: 1.0,
        bottom: 0.0,
        left: 0.0,
        right: 1.0,
        color: Color32::from_rgb(0xce, 0xe7, 0xff),
    },
    GlowLayer {
        top: 2.0,
        bottom: 0.0,
        left: 1.0,
        right: 0.0,
        color: Color32::from_rgb(0x98, 0xb8, 0xea),
    },
    GlowLayer {
        top: 2.0,
        bottom: 0.0,
        left: 0.0,
        right: 2.0,
        color: Color32::from_rgb(0xbc, 0xd4, 0xf6),
    },
    GlowLayer {
        top: 0.0,
        bottom: 1.0,
        left: 1.0,
        right: 0.0,
        color: Color32::from_rgb(0x89, 0xad, 0xe4),
    },
    GlowLayer {
        top: 0.0,
        bottom: 2.0,
        left: 2.0,
        right: 0.0,
        color: Color32::from_rgb(0x89, 0xad, 0xe4),
    },
];

impl LunaPalette {
    /// Authentic Windows XP Luna (blue + cream) palette.
    pub const LIGHT: Self = Self {
        accent: Color32::from_rgb(0x1e, 0x6b, 0xc6),
        danger: Color32::from_rgb(0xc4, 0x20, 0x20),
        talking: Color32::from_rgb(0xff, 0x6a, 0x3d),
        surface: Color32::from_rgb(0xff, 0xff, 0xff),
        surface_alt: Color32::from_rgb(0xec, 0xe9, 0xd8),
        surface_sunken: Color32::from_rgb(0xd4, 0xcf, 0xb7),
        text: Color32::BLACK,
        text_muted: Color32::from_rgb(0x55, 0x55, 0x55),
        text_on_accent: Color32::WHITE,
        text_on_danger: Color32::WHITE,
        line: Color32::from_rgb(0x08, 0x31, 0xd9),
        line_soft: Color32::from_rgb(0xac, 0xa8, 0x99),

        pane_border: Color32::from_rgb(0x7f, 0x9d, 0xb9),
        group_fill: Color32::from_rgb(0xff, 0xff, 0xff),
        group_border: Color32::from_rgb(0xd0, 0xd0, 0xbf),
        field_fill: Color32::from_rgb(0xff, 0xff, 0xff),
        field_border: Color32::from_rgb(0x7f, 0x9d, 0xb9),
        popup_fill: Color32::from_rgb(0xff, 0xff, 0xff),
        popup_border: Color32::from_rgb(0x7f, 0x9d, 0xb9),
        tooltip_fill: Color32::from_rgb(0xff, 0xff, 0xe1),
        tooltip_border: Color32::BLACK,

        titlebar_stops: &[
            (0.00, Color32::from_rgb(0x09, 0x97, 0xff)),
            (0.08, Color32::from_rgb(0x00, 0x53, 0xee)),
            (0.40, Color32::from_rgb(0x00, 0x50, 0xee)),
            (0.88, Color32::from_rgb(0x00, 0x66, 0xff)),
            (0.93, Color32::from_rgb(0x00, 0x66, 0xff)),
            (0.95, Color32::from_rgb(0x00, 0x5b, 0xff)),
            (0.96, Color32::from_rgb(0x00, 0x3d, 0xd7)),
            (1.00, Color32::from_rgb(0x00, 0x3d, 0xd7)),
        ],
        titlebar_border: Color32::from_rgb(0x08, 0x31, 0xd9),
        toolbar_stops: &[
            (0.0, Color32::from_rgb(0xf5, 0xf4, 0xea)),
            (1.0, Color32::from_rgb(0xec, 0xe9, 0xd8)),
        ],
        statusbar_stops: &[
            (0.0, Color32::from_rgb(0xec, 0xe9, 0xd8)),
            (1.0, Color32::from_rgb(0xd4, 0xcf, 0xb7)),
        ],

        button_border: Color32::from_rgb(0x00, 0x3c, 0x74),
        button_rest_stops: &[
            (0.00, Color32::from_rgb(0xff, 0xff, 0xff)),
            (0.86, Color32::from_rgb(0xec, 0xeb, 0xe5)),
            (1.00, Color32::from_rgb(0xd8, 0xd0, 0xc4)),
        ],
        button_pressed_stops: &[
            (0.00, Color32::from_rgb(0xcd, 0xca, 0xc3)),
            (0.08, Color32::from_rgb(0xe3, 0xe3, 0xdb)),
            (0.94, Color32::from_rgb(0xe5, 0xe5, 0xde)),
            (1.00, Color32::from_rgb(0xf2, 0xf2, 0xf1)),
        ],
        hover_layers: XP_HOVER_LAYERS,
        focus_layers: XP_FOCUS_LAYERS,
        primary_stops: &[
            (0.0, Color32::from_rgb(0x9c, 0xd6, 0x6a)),
            (1.0, Color32::from_rgb(0x4a, 0x8c, 0x1c)),
        ],
        primary_border: Color32::from_rgb(0x2e, 0x62, 0x10),
        danger_stops: &[
            (0.0, Color32::from_rgb(0xff, 0x84, 0x84)),
            (1.0, Color32::from_rgb(0xc4, 0x20, 0x20)),
        ],
        danger_border: Color32::from_rgb(0x8b, 0x1a, 0x1a),
        accent_active_stops: &[
            (0.0, Color32::from_rgb(0x86, 0xad, 0xe0)),
            (1.0, Color32::from_rgb(0xc8, 0xdb, 0xf3)),
        ],

        selection_fill: Color32::from_rgb(0x31, 0x6a, 0xc5),
        separator_dark: Color32::from_rgb(0xac, 0xa8, 0x99),
        separator_light: Color32::WHITE,
        scroll_track: Color32::from_rgb(0xd4, 0xd0, 0xc8),
        scroll_handle_rest: Color32::from_rgb(0xec, 0xeb, 0xe5),
        scroll_handle_hover: Color32::WHITE,
        scroll_handle_active: Color32::from_rgb(0xcd, 0xca, 0xc3),

        check_border: Color32::from_rgb(0x1d, 0x52, 0x81),
        check_border_disabled: Color32::from_rgb(0xca, 0xc8, 0xbb),
        check_color: Color32::from_rgb(0x21, 0xa8, 0x21),
        check_color_disabled: Color32::from_rgb(0x80, 0x80, 0x80),
        check_fill_top: Color32::from_rgb(0xdc, 0xdc, 0xd7),
        check_fill_bottom: Color32::WHITE,
        check_fill_pressed_top: Color32::from_rgb(0xb0, 0xb0, 0xa7),
        check_fill_pressed_bottom: Color32::from_rgb(0xe3, 0xe1, 0xd2),
        check_fill_disabled: Color32::WHITE,
        check_hover_amber: Color32::from_rgb(0xf8, 0xb6, 0x36),
        check_hover_cream_gold: Color32::from_rgb(0xfe, 0xdf, 0x9c),

        track_fill: Color32::from_rgb(0xec, 0xeb, 0xe4),
        track_shadow: Color32::from_rgb(0x9d, 0x9c, 0x99),
        track_highlight: Color32::WHITE,
        thumb_outline: Color32::from_rgb(0x77, 0x88, 0x92),
        thumb_body_fill: Color32::from_rgb(0xf3, 0xf3, 0xef),
        thumb_body_highlight: Color32::from_rgb(0xf7, 0xf7, 0xf4),
        thumb_body_shadow: Color32::from_rgb(0xc3, 0xc3, 0xc0),
        thumb_accent: Color32::from_rgb(0x21, 0xb8, 0x1f),

        avatar_bg: Color32::WHITE,
        avatar_highlight: Color32::WHITE,
        avatar_shadow: Color32::from_rgb(0x40, 0x40, 0x40),
        badge_muted: Color32::from_rgb(0xc4, 0x20, 0x20),
        badge_deafened: Color32::from_rgb(0x44, 0x44, 0x88),
        badge_text: Color32::WHITE,
        badge_border: Color32::BLACK,
        talking_ring: Color32::from_rgb(0x21, 0xb8, 0x1f),

        caret_fill: Color32::WHITE,
        caret_border: Color32::from_rgb(0x80, 0x80, 0x80),
        caret_glyph: Color32::BLACK,

        focus_ring: Color32::BLACK,

        // XP group-box legend blue (same as luna_compare.rs):
        // `color: #0046d5` in the reference HTML.
        legend: Color32::from_rgb(0x00, 0x46, 0xd5),
    };

    /// Dark Luna: charcoal chrome + same Luna-blue accents + the same
    /// orange-hover / blue-focus glows. Not an XP ship config — a faithful
    /// reimagining in the same idiom.
    pub const DARK: Self = Self {
        accent: Color32::from_rgb(0x3d, 0x8b, 0xe6),
        danger: Color32::from_rgb(0xe0, 0x54, 0x54),
        talking: Color32::from_rgb(0xff, 0x6a, 0x3d),
        surface: Color32::from_rgb(0x2b, 0x2b, 0x2d),
        surface_alt: Color32::from_rgb(0x3a, 0x3a, 0x3d),
        surface_sunken: Color32::from_rgb(0x1e, 0x1e, 0x20),
        text: Color32::from_rgb(0xe8, 0xe8, 0xe8),
        text_muted: Color32::from_rgb(0xa0, 0xa0, 0xa0),
        text_on_accent: Color32::WHITE,
        text_on_danger: Color32::WHITE,
        line: Color32::from_rgb(0x08, 0x31, 0xd9),
        line_soft: Color32::from_rgb(0x55, 0x55, 0x5c),

        pane_border: Color32::from_rgb(0x55, 0x55, 0x5c),
        group_fill: Color32::from_rgb(0x2b, 0x2b, 0x2d),
        group_border: Color32::from_rgb(0x55, 0x55, 0x5c),
        field_fill: Color32::from_rgb(0x1e, 0x1e, 0x20),
        field_border: Color32::from_rgb(0x55, 0x55, 0x5c),
        popup_fill: Color32::from_rgb(0x2b, 0x2b, 0x2d),
        popup_border: Color32::from_rgb(0x55, 0x55, 0x5c),
        tooltip_fill: Color32::from_rgb(0x3a, 0x38, 0x28),
        tooltip_border: Color32::from_rgb(0xff, 0xff, 0xe1),

        titlebar_stops: &[
            (0.00, Color32::from_rgb(0x05, 0x5c, 0xa0)),
            (0.08, Color32::from_rgb(0x00, 0x2e, 0x90)),
            (0.40, Color32::from_rgb(0x00, 0x2c, 0x8e)),
            (0.88, Color32::from_rgb(0x00, 0x3a, 0x98)),
            (0.93, Color32::from_rgb(0x00, 0x3a, 0x98)),
            (0.95, Color32::from_rgb(0x00, 0x34, 0x90)),
            (0.96, Color32::from_rgb(0x00, 0x1e, 0x70)),
            (1.00, Color32::from_rgb(0x00, 0x1e, 0x70)),
        ],
        titlebar_border: Color32::from_rgb(0x00, 0x14, 0x60),
        toolbar_stops: &[
            (0.0, Color32::from_rgb(0x48, 0x48, 0x4e)),
            (1.0, Color32::from_rgb(0x3a, 0x3a, 0x3d)),
        ],
        statusbar_stops: &[
            (0.0, Color32::from_rgb(0x3a, 0x3a, 0x3d)),
            (1.0, Color32::from_rgb(0x26, 0x26, 0x28)),
        ],

        button_border: Color32::from_rgb(0x14, 0x14, 0x16),
        button_rest_stops: &[
            (0.00, Color32::from_rgb(0x5c, 0x5c, 0x64)),
            (0.86, Color32::from_rgb(0x46, 0x46, 0x4e)),
            (1.00, Color32::from_rgb(0x32, 0x32, 0x3a)),
        ],
        button_pressed_stops: &[
            (0.00, Color32::from_rgb(0x22, 0x22, 0x28)),
            (0.08, Color32::from_rgb(0x2a, 0x2a, 0x32)),
            (0.94, Color32::from_rgb(0x32, 0x32, 0x3a)),
            (1.00, Color32::from_rgb(0x40, 0x40, 0x48)),
        ],
        hover_layers: XP_HOVER_LAYERS,
        focus_layers: XP_FOCUS_LAYERS,
        primary_stops: &[
            (0.0, Color32::from_rgb(0x74, 0xb0, 0x4a)),
            (1.0, Color32::from_rgb(0x30, 0x68, 0x0c)),
        ],
        primary_border: Color32::from_rgb(0x20, 0x44, 0x08),
        danger_stops: &[
            (0.0, Color32::from_rgb(0xd8, 0x60, 0x60)),
            (1.0, Color32::from_rgb(0x80, 0x18, 0x18)),
        ],
        danger_border: Color32::from_rgb(0x55, 0x10, 0x10),
        accent_active_stops: &[
            (0.0, Color32::from_rgb(0x2d, 0x56, 0x90)),
            (1.0, Color32::from_rgb(0x5a, 0x82, 0xbe)),
        ],

        selection_fill: Color32::from_rgb(0x31, 0x6a, 0xc5),
        separator_dark: Color32::from_rgb(0x10, 0x10, 0x12),
        separator_light: Color32::from_rgb(0x55, 0x55, 0x5c),
        scroll_track: Color32::from_rgb(0x26, 0x26, 0x28),
        scroll_handle_rest: Color32::from_rgb(0x48, 0x48, 0x4e),
        scroll_handle_hover: Color32::from_rgb(0x60, 0x60, 0x68),
        scroll_handle_active: Color32::from_rgb(0x38, 0x38, 0x40),

        check_border: Color32::from_rgb(0x98, 0x98, 0xa0),
        check_border_disabled: Color32::from_rgb(0x44, 0x44, 0x48),
        check_color: Color32::from_rgb(0x3e, 0xc8, 0x3e),
        check_color_disabled: Color32::from_rgb(0x60, 0x60, 0x60),
        check_fill_top: Color32::from_rgb(0x2a, 0x2a, 0x30),
        check_fill_bottom: Color32::from_rgb(0x40, 0x40, 0x48),
        check_fill_pressed_top: Color32::from_rgb(0x1c, 0x1c, 0x22),
        check_fill_pressed_bottom: Color32::from_rgb(0x2e, 0x2e, 0x36),
        check_fill_disabled: Color32::from_rgb(0x2b, 0x2b, 0x2d),
        check_hover_amber: Color32::from_rgb(0xf8, 0xb6, 0x36),
        check_hover_cream_gold: Color32::from_rgb(0xfe, 0xdf, 0x9c),

        track_fill: Color32::from_rgb(0x1e, 0x1e, 0x20),
        track_shadow: Color32::BLACK,
        track_highlight: Color32::from_rgb(0x5c, 0x5c, 0x64),
        thumb_outline: Color32::from_rgb(0x98, 0xa8, 0xb0),
        thumb_body_fill: Color32::from_rgb(0x3c, 0x3c, 0x42),
        thumb_body_highlight: Color32::from_rgb(0x4c, 0x4c, 0x54),
        thumb_body_shadow: Color32::from_rgb(0x1c, 0x1c, 0x20),
        thumb_accent: Color32::from_rgb(0x21, 0xb8, 0x1f),

        avatar_bg: Color32::from_rgb(0x2b, 0x2b, 0x2d),
        avatar_highlight: Color32::from_rgb(0x60, 0x60, 0x68),
        avatar_shadow: Color32::from_rgb(0x08, 0x08, 0x0a),
        badge_muted: Color32::from_rgb(0xc4, 0x20, 0x20),
        badge_deafened: Color32::from_rgb(0x68, 0x68, 0xb0),
        badge_text: Color32::WHITE,
        badge_border: Color32::BLACK,
        talking_ring: Color32::from_rgb(0x21, 0xb8, 0x1f),

        caret_fill: Color32::from_rgb(0x2b, 0x2b, 0x2d),
        caret_border: Color32::from_rgb(0x98, 0x98, 0xa0),
        caret_glyph: Color32::from_rgb(0xe0, 0xe0, 0xe0),

        focus_ring: Color32::WHITE,

        // Brightened XP legend blue for contrast on the charcoal group fill.
        legend: Color32::from_rgb(0x7e, 0xb5, 0xff),
    };
}

pub struct LunaTheme {
    palette: &'static LunaPalette,
    tokens: Tokens,
    toggle_impl: &'static LunaToggle,
    slider_impl: &'static LunaSlider,
    presence_impl: &'static LunaPresence,
    tree_impl: &'static LunaTree,
    level_meter_impl: &'static LunaLevelMeter,
}

impl Default for LunaTheme {
    fn default() -> Self {
        Self::light()
    }
}

impl LunaTheme {
    pub fn light() -> Self {
        Self {
            palette: &LunaPalette::LIGHT,
            tokens: tokens_from_palette(&LunaPalette::LIGHT),
            toggle_impl: &LUNA_TOGGLE_LIGHT,
            slider_impl: &LUNA_SLIDER_LIGHT,
            presence_impl: &LUNA_PRESENCE_LIGHT,
            tree_impl: &LUNA_TREE_LIGHT,
            level_meter_impl: &LUNA_LEVEL_METER_LIGHT,
        }
    }

    pub fn dark() -> Self {
        Self {
            palette: &LunaPalette::DARK,
            tokens: tokens_from_palette(&LunaPalette::DARK),
            toggle_impl: &LUNA_TOGGLE_DARK,
            slider_impl: &LUNA_SLIDER_DARK,
            presence_impl: &LUNA_PRESENCE_DARK,
            tree_impl: &LUNA_TREE_DARK,
            level_meter_impl: &LUNA_LEVEL_METER_DARK,
        }
    }
}

fn tokens_from_palette(p: &LunaPalette) -> Tokens {
    Tokens {
        accent: p.accent,
        danger: p.danger,
        talking: p.talking,
        surface: p.surface,
        surface_alt: p.surface_alt,
        surface_sunken: p.surface_sunken,
        text: p.text,
        text_muted: p.text_muted,
        text_on_accent: p.text_on_accent,
        text_on_danger: p.text_on_danger,
        line: p.line,
        line_soft: p.line_soft,
        radius_sm: 3.0,
        radius_md: 3.0,
        radius_pill: 4.0,
        pad_sm: 3.0,
        pad_md: 6.0,
        bevel_inset: 2.0,
        font_body: FontId::new(12.0, FontFamily::Proportional),
        font_label: FontId::new(11.0, FontFamily::Proportional),
        font_heading: FontId::new(12.0, FontFamily::Proportional),
        font_mono: FontId::new(11.0, FontFamily::Monospace),
    }
}

impl Theme for LunaTheme {
    fn name(&self) -> &'static str {
        "luna"
    }
    fn tokens(&self) -> &Tokens {
        &self.tokens
    }

    fn surface(&self, rect: Rect, kind: SurfaceKind) -> Shape {
        let p = self.palette;
        let mut shapes: Vec<Shape> = Vec::new();
        match kind {
            SurfaceKind::Panel => {
                shapes.push(Shape::rect_filled(rect, 0.0, p.surface_alt));
            }
            SurfaceKind::Pane => {
                shapes.push(Shape::rect_filled(rect, 0.0, p.surface));
                shapes.push(Shape::Rect(RectShape::stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, p.pane_border),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Group => {
                // Flat fieldset: white fill, light-beige 1px border, 4px radius.
                shapes.push(Shape::rect_filled(rect, CornerRadius::from(4.0), p.group_fill));
                shapes.push(Shape::Rect(RectShape::stroke(
                    rect,
                    CornerRadius::from(4.0),
                    Stroke::new(1.0, p.group_border),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Titlebar => {
                // 8-stop classic Luna blue with rounded top corners (8px).
                // Dark variant darkens the stops; the narrow band at 95-96%
                // mimics the highlight notch in both.
                let r = 8.0_f32.min(rect.height() * 0.5);
                shapes.extend(titlebar_gradient(rect, p.titlebar_stops, 32, r));
                shapes.push(Shape::Rect(RectShape::stroke(
                    rect,
                    CornerRadius {
                        nw: r.round() as u8,
                        ne: r.round() as u8,
                        sw: 0,
                        se: 0,
                    },
                    Stroke::new(1.0, p.titlebar_border),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Toolbar => {
                shapes.extend(vertical_gradient(rect, p.toolbar_stops, 12, 0.0));
                shapes.push(Shape::line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    Stroke::new(1.0, p.line_soft),
                ));
            }
            SurfaceKind::Statusbar => {
                shapes.extend(vertical_gradient(rect, p.statusbar_stops, 8, 0.0));
                shapes.push(Shape::line_segment(
                    [rect.left_top(), rect.right_top()],
                    Stroke::new(1.0, p.line_soft),
                ));
            }
            SurfaceKind::Tooltip => {
                shapes.push(Shape::rect_filled(rect, CornerRadius::ZERO, p.tooltip_fill));
                shapes.push(Shape::Rect(RectShape::stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, p.tooltip_border),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Popup => {
                // Flat menu surface: white fill + single blue border.
                shapes.push(Shape::rect_filled(rect, CornerRadius::ZERO, p.popup_fill));
                shapes.push(Shape::Rect(RectShape::stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, p.popup_border),
                    StrokeKind::Inside,
                )));
            }
            SurfaceKind::Field => {
                // Flat text input: single 1px border, no sunken bevel — XP
                // did not use 95/98 chrome on Luna.
                shapes.push(Shape::rect_filled(rect, CornerRadius::ZERO, p.field_fill));
                shapes.push(Shape::Rect(RectShape::stroke(
                    rect,
                    CornerRadius::ZERO,
                    Stroke::new(1.0, p.field_border),
                    StrokeKind::Inside,
                )));
            }
        }
        Shape::Vec(shapes)
    }

    fn pressable(&self, rect: Rect, role: PressableRole, state: PressableState) -> Shape {
        let p = self.palette;
        let t = &self.tokens;
        let radius = t.radius_md;

        // Ghost: no chrome at rest. When hovered / pressed / focused /
        // active, fall through to the standard chrome so the button has
        // something to paint. Otherwise return empty.
        let has_interaction = state.hovered || state.pressed || state.focused;
        if matches!(role, PressableRole::Ghost) && !state.active && !has_interaction {
            return Shape::Noop;
        }

        // Base gradient and border by role/active. Accent-active uses the
        // pressed gradient + accent border so "selected tab" / PTT-on reads
        // as a depressed, highlighted button — the XP idiom for a sticky
        // toggled-on state.
        let (mut stops, mut border): (Vec<(f32, Color32)>, Color32) = match role {
            PressableRole::Primary => (p.primary_stops.to_vec(), p.primary_border),
            PressableRole::Danger if state.active => (p.danger_stops.to_vec(), p.danger_border),
            PressableRole::Accent if state.active => (p.accent_active_stops.to_vec(), p.accent),
            _ => {
                let base_stops = if state.active {
                    p.button_pressed_stops.to_vec()
                } else {
                    p.button_rest_stops.to_vec()
                };
                let base_border = match role {
                    PressableRole::Accent => p.accent,
                    _ => p.button_border,
                };
                (base_stops, base_border)
            }
        };

        // Pressed (held): neutral roles flip to the pressed gradient; Primary
        // and Danger darken instead so their role color stays recognisable.
        if state.pressed && !state.disabled {
            match role {
                PressableRole::Primary | PressableRole::Danger => {
                    stops = stops.iter().map(|(t_pos, c)| (*t_pos, darken(*c, 0.08))).collect();
                }
                _ => {
                    stops = p.button_pressed_stops.to_vec();
                }
            }
        }

        // Disabled: fade toward the panel surface.
        if state.disabled {
            stops = stops
                .iter()
                .map(|(t_pos, c)| (*t_pos, blend(*c, p.surface_alt, 0.55)))
                .collect();
            border = blend(border, p.surface_alt, 0.55);
        }

        let mut shapes: Vec<Shape> = vertical_gradient(rect, &stops, 12, radius);

        // Border.
        shapes.push(Shape::Rect(RectShape::stroke(
            rect,
            CornerRadius::from(radius),
            Stroke::new(1.0, border),
            StrokeKind::Inside,
        )));

        // Hover / focus inner glows, layered bottom-up to mimic CSS's
        // first-listed-on-top box-shadow stacking. Each layer is an L-shape
        // with its own per-edge thickness — the iconic "lit from top-right"
        // look. Glow layer slices are shared between light and dark
        // palettes since the colors are mid-brightness.
        if state.hovered && !state.pressed && !state.disabled {
            shapes.extend(glow_layers(rect, p.hover_layers));
        }
        if state.focused && !state.pressed && !state.disabled {
            shapes.extend(glow_layers(rect, p.focus_layers));
        }

        Shape::Vec(shapes)
    }

    fn selection(&self, rect: Rect) -> Shape {
        Shape::rect_filled(rect, CornerRadius::ZERO, self.palette.selection_fill)
    }

    fn separator(&self, rect: Rect, axis: Axis) -> Shape {
        let c_dark = self.palette.separator_dark;
        let c_light = self.palette.separator_light;
        match axis {
            Axis::Horizontal => {
                let mid = rect.center().y;
                Shape::Vec(vec![
                    Shape::line_segment(
                        [pos2(rect.left(), mid), pos2(rect.right(), mid)],
                        Stroke::new(1.0, c_dark),
                    ),
                    Shape::line_segment(
                        [pos2(rect.left(), mid + 1.0), pos2(rect.right(), mid + 1.0)],
                        Stroke::new(1.0, c_light),
                    ),
                ])
            }
            Axis::Vertical => {
                let mid = rect.center().x;
                Shape::Vec(vec![
                    Shape::line_segment(
                        [pos2(mid, rect.top()), pos2(mid, rect.bottom())],
                        Stroke::new(1.0, c_dark),
                    ),
                    Shape::line_segment(
                        [pos2(mid + 1.0, rect.top()), pos2(mid + 1.0, rect.bottom())],
                        Stroke::new(1.0, c_light),
                    ),
                ])
            }
        }
    }

    fn text_color(
        &self,
        _role: TextRole,
        on: SurfaceKind,
        pressable_role: Option<PressableRole>,
        state: PressableState,
    ) -> Color32 {
        let t = &self.tokens;
        let base = match (on, pressable_role, state.active) {
            (SurfaceKind::Titlebar, _, _) => Color32::WHITE,
            (_, Some(PressableRole::Primary), _) => t.text_on_accent,
            (_, Some(PressableRole::Danger), true) => t.text_on_danger,
            // Accent-active paints a light-blue "depressed selected tab"
            // look; black label keeps contrast against it.
            (_, Some(PressableRole::Accent), true) => t.text,
            _ => t.text,
        };
        if state.disabled {
            blend(base, t.surface_alt, 0.55)
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
        let p = self.palette;
        visuals.window_fill = p.surface_alt;
        visuals.panel_fill = p.surface_alt;
        visuals.window_stroke = Stroke::new(1.0, p.line);
        visuals.window_shadow = Shadow::NONE;
        visuals.override_text_color = Some(p.text);
        visuals.hyperlink_color = p.accent;
        visuals.selection.bg_fill = p.selection_fill;
        visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);

        // Scrollbar colors. egui paints the track from `extreme_bg_color`
        // and the handle from `widgets.{inactive,hovered,active}.bg_fill`
        // — two flat rect_filled calls, no bevels — so pick colors that
        // read as XP. Small radius keeps the handle from looking pill-like.
        visuals.extreme_bg_color = p.scroll_track;
        for w in [
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            w.corner_radius = eframe::egui::CornerRadius::from(1);
        }
        visuals.widgets.inactive.bg_fill = p.scroll_handle_rest;
        visuals.widgets.hovered.bg_fill = p.scroll_handle_hover;
        visuals.widgets.active.bg_fill = p.scroll_handle_active;
    }

    fn apply_egui_style(&self, style: &mut Style) {
        // Solid (always-visible) scrollbars at XP's chunky width. egui's
        // default `ScrollStyle::floating()` makes bars vanish when not
        // hovered, which reads as out-of-place against everything else
        // in this theme being persistently chrome-y.
        let mut scroll = eframe::egui::style::ScrollStyle::solid();
        scroll.bar_width = 16.0;
        scroll.handle_min_length = 20.0;
        scroll.bar_inner_margin = 0.0;
        scroll.bar_outer_margin = 0.0;
        style.spacing.scroll = scroll;
    }

    fn toggle(&self) -> &dyn ToggleImpl {
        self.toggle_impl
    }

    fn slider(&self) -> &dyn SliderImpl {
        self.slider_impl
    }

    fn presence(&self) -> &dyn PresenceImpl {
        self.presence_impl
    }

    fn tree(&self) -> &dyn TreeImpl {
        self.tree_impl
    }

    fn level_meter(&self) -> &dyn LevelMeterImpl {
        self.level_meter_impl
    }

    fn group_title_color(&self) -> Color32 {
        self.palette.legend
    }

    fn radio_indicator(&self, rect: Rect, selected: bool, state: PressableState) -> Shape {
        paint_xp_radio_indicator(rect, selected, state, self.palette)
    }
}

// ---------------------------------------------------------------------
// Luna's native toggle: 13×13 XP checkbox with a gold hover glow and a
// green ✓ glyph. ToggleStyle::Switch is intentionally rendered identically
// to ToggleStyle::Checkbox — XP never shipped a sliding-pill toggle, and
// the whole point of this impl is to be XP-authentic.

const XP_CB_SIZE: f32 = 13.0;

pub struct LunaToggle {
    palette: &'static LunaPalette,
}

static LUNA_TOGGLE_LIGHT: LunaToggle = LunaToggle {
    palette: &LunaPalette::LIGHT,
};
static LUNA_TOGGLE_DARK: LunaToggle = LunaToggle {
    palette: &LunaPalette::DARK,
};

impl ToggleImpl for LunaToggle {
    fn layout(&self, ui: &Ui, args: &ToggleArgs) -> Vec2 {
        let font = ui.theme().font(TextRole::Body);
        let galley = args
            .label
            .clone()
            .into_galley(ui, Some(TextWrapMode::Extend), f32::INFINITY, font);
        Vec2::new(
            XP_CB_SIZE + LABEL_GAP + galley.rect.width(),
            XP_CB_SIZE.max(galley.rect.height()),
        )
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, args: &ToggleArgs, state: ToggleState) {
        let theme = ui.theme();
        let tokens = theme.tokens();
        let p = self.palette;

        // Indicator on the left, vertically centered. Snap to the
        // physical-pixel grid so the hover bevel L-strips and the
        // ✓ glyph line endpoints co-align with the outer box.
        let ppp = ui.ctx().pixels_per_point();
        let indicator_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - XP_CB_SIZE * 0.5),
            Vec2::splat(XP_CB_SIZE),
        )
        .round_to_pixels(ppp);
        paint_xp_checkbox_indicator(ui, indicator_rect, state, p);

        // Label, vertically centered on the full row.
        let label_rect = Rect::from_min_max(
            Pos2::new(indicator_rect.right() + LABEL_GAP, rect.top()),
            rect.right_bottom(),
        );
        let text_color = if state.disabled {
            toggle_blend(tokens.text, tokens.surface_alt, 0.5)
        } else {
            tokens.text
        };
        paint_label_accessible(ui, label_rect, &args.label, text_color, "luna_toggle_label");

        // Focus ring — XP wraps the label, not the indicator, in a dotted
        // outline. Approximated here as a thin 1-px stroke; exact dotted
        // rendering would need per-segment stippling.
        if state.focused && !state.disabled {
            ui.painter().add(Shape::Rect(RectShape::stroke(
                label_rect.shrink(1.0),
                CornerRadius::ZERO,
                Stroke::new(1.0, p.focus_ring),
                StrokeKind::Inside,
            )));
        }
    }
}

fn paint_xp_checkbox_indicator(ui: &mut Ui, rect: Rect, state: ToggleState, p: &LunaPalette) {
    // Background: subtle 135° gradient. Vertical 2-stop approximation.
    let (fill_top, fill_bottom) = if state.disabled {
        (p.check_fill_disabled, p.check_fill_disabled)
    } else if state.pressed {
        (p.check_fill_pressed_top, p.check_fill_pressed_bottom)
    } else {
        (p.check_fill_top, p.check_fill_bottom)
    };
    // Two stacked half-height strips for a cheap gradient.
    let mid_y = rect.center().y;
    ui.painter().rect_filled(
        Rect::from_min_max(rect.min, Pos2::new(rect.right(), mid_y)),
        CornerRadius::ZERO,
        fill_top,
    );
    ui.painter().rect_filled(
        Rect::from_min_max(Pos2::new(rect.left(), mid_y), rect.max),
        CornerRadius::ZERO,
        fill_bottom,
    );

    // Border.
    let border_color = if state.disabled {
        p.check_border_disabled
    } else {
        p.check_border
    };
    ui.painter().add(Shape::Rect(RectShape::stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, border_color),
        StrokeKind::Inside,
    )));

    // Hover glow: `inset -2px -2px <amber>, inset 2px 2px <cream-gold>`.
    // Bottom-right amber, top-left cream-gold (topmost).
    if state.hovered && !state.pressed && !state.disabled {
        let inner = rect.shrink(1.0);

        // Bottom-right L in amber (painted first → bottommost).
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(inner.left(), inner.bottom() - 2.0), inner.right_bottom()),
            CornerRadius::ZERO,
            p.check_hover_amber,
        );
        ui.painter().rect_filled(
            Rect::from_min_max(Pos2::new(inner.right() - 2.0, inner.top()), inner.right_bottom()),
            CornerRadius::ZERO,
            p.check_hover_amber,
        );
        // Top-left L in cream-gold (topmost).
        ui.painter().rect_filled(
            Rect::from_min_max(inner.left_top(), Pos2::new(inner.right(), inner.top() + 2.0)),
            CornerRadius::ZERO,
            p.check_hover_cream_gold,
        );
        ui.painter().rect_filled(
            Rect::from_min_max(inner.left_top(), Pos2::new(inner.left() + 2.0, inner.bottom())),
            CornerRadius::ZERO,
            p.check_hover_cream_gold,
        );
    }

    // Checkmark: bold green ✓ when on. Two line segments, thinner than
    // the default impl's because the box is 13 px instead of 16.
    if state.on {
        let check_color = if state.disabled {
            p.check_color_disabled
        } else {
            p.check_color
        };
        let stroke = Stroke::new(1.75, check_color);
        let inset = rect.size().x * 0.18;
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
}

/// XP-style radio: round well with a soft bevel border, white fill,
/// and a small gradient-green dot when selected. Returns a `Shape` so
/// the widget layer can hand it to the painter without owning a `Ui`.
fn paint_xp_radio_indicator(rect: Rect, selected: bool, state: PressableState, p: &LunaPalette) -> Shape {
    let mut shapes: Vec<Shape> = Vec::new();
    let radius = rect.width() * 0.5;
    let corner = CornerRadius::from(radius);

    let fill = if state.disabled {
        p.check_fill_disabled
    } else if state.pressed {
        p.check_fill_pressed_top
    } else {
        p.check_fill_top
    };
    let border = if state.disabled {
        p.check_border_disabled
    } else {
        p.check_border
    };

    shapes.push(Shape::Rect(RectShape::new(
        rect,
        corner,
        fill,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    )));

    // Hover bevel: amber bottom-right hint (subtle, matches checkbox hover).
    if state.hovered && !state.pressed && !state.disabled {
        shapes.push(Shape::Rect(RectShape::stroke(
            rect.shrink(1.0),
            corner,
            Stroke::new(1.0, p.check_hover_amber),
            StrokeKind::Inside,
        )));
    }

    if selected {
        let dot_r = rect.width() * 0.28;
        let color = if state.disabled {
            p.check_color_disabled
        } else {
            p.check_color
        };
        shapes.push(Shape::circle_filled(rect.center(), dot_r, color));
    }

    if state.focused && !state.disabled {
        shapes.push(Shape::Rect(RectShape::stroke(
            rect.expand(1.5),
            CornerRadius::from(radius + 1.5),
            Stroke::new(1.0, p.focus_ring),
            StrokeKind::Outside,
        )));
    }

    Shape::Vec(shapes)
}

// XP Explorer tree-line texture. Classic XP used a 1-on-1-off pattern
// with 1×1 dots; on modern hi-DPI displays a single logical pixel is
// visually negligible and the line effectively disappears. We use a
// 2-on-2-off pattern so each dash is at least 2 physical pixels at
// `pixels_per_point=1.0`, keeping the dotted look while guaranteeing
// the line is visible at the sizes users actually run the app.
const DASH_ON: f32 = 2.0;
const DASH_PERIOD: f32 = 4.0;

fn paint_dotted_v(ui: &mut Ui, x: f32, y_start: f32, y_end: f32, color: Color32) {
    let x = x.round();
    let mut y = y_start.round();
    // Phase-align to the grid so consecutive rows' dashes meet cleanly.
    let phase = (y as i32).rem_euclid(DASH_PERIOD as i32) as f32;
    y -= phase;
    while y < y_end {
        let top = y.max(y_start);
        let bottom = (y + DASH_ON).min(y_end);
        if bottom > top {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(x, top), Vec2::new(1.0, bottom - top)),
                0.0,
                color,
            );
        }
        y += DASH_PERIOD;
    }
}

fn paint_dotted_h(ui: &mut Ui, x_start: f32, x_end: f32, y: f32, color: Color32) {
    let y = y.round();
    let mut x = x_start.round();
    let phase = (x as i32).rem_euclid(DASH_PERIOD as i32) as f32;
    x -= phase;
    while x < x_end {
        let left = x.max(x_start);
        let right = (x + DASH_ON).min(x_end);
        if right > left {
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(left, y), Vec2::new(right - left, 1.0)),
                0.0,
                color,
            );
        }
        x += DASH_PERIOD;
    }
}

// ---------------------------------------------------------------------
// Luna's native slider: 11×21 pentagon arrow-pointer thumb on a 2-px
// sunken track, modeled after `reference/XP.css/themes/XP/_forms.scss`
// (input[type="range"]) and `icon/indicator-horizontal.svg`. ToggleStyle's
// philosophy applies — Luna ignores the abstract "rounded sunken track +
// circular thumb" baseline and renders an XP-authentic shape.

/// Width of the arrow-pointer thumb (px), matching the reference SVG.
const XP_THUMB_W: f32 = 11.0;
/// Total height of the thumb (px). Top 14 px are the rectangular body,
/// bottom 7 px taper to the pointer tip.
const XP_THUMB_H: f32 = 21.0;
/// Height of the body rectangle within the thumb; the remainder is the tip.
const XP_THUMB_BODY_H: f32 = 14.0;
/// Track height (px). XP draws a 2-px line with a sunken bevel frame.
const XP_TRACK_H: f32 = 2.0;
/// Total row height — tall enough to hold the thumb without clipping.
const XP_ROW_H: f32 = 22.0;

pub struct LunaSlider {
    palette: &'static LunaPalette,
}

static LUNA_SLIDER_LIGHT: LunaSlider = LunaSlider {
    palette: &LunaPalette::LIGHT,
};
static LUNA_SLIDER_DARK: LunaSlider = LunaSlider {
    palette: &LunaPalette::DARK,
};

impl SliderImpl for LunaSlider {
    fn layout(&self, _ui: &Ui, args: &SliderArgs) -> Vec2 {
        let total_w = if args.show_value {
            args.width + VALUE_BOX_GAP + args.value_box_width
        } else {
            args.width
        };
        Vec2::new(total_w, XP_ROW_H.max(XP_THUMB_H))
    }

    fn track_rect(&self, rect: Rect, args: &SliderArgs) -> Rect {
        // Inset the track by half the thumb width on each side so the
        // thumb stays inside the widget bounds, plus 1 px for the bevel
        // frame to live in.
        let track_outer_right = if args.show_value {
            rect.left() + args.width
        } else {
            rect.right()
        };
        let inset = XP_THUMB_W * 0.5 + 1.0;
        Rect::from_min_max(
            Pos2::new(rect.left() + inset, rect.center().y - XP_TRACK_H * 0.5),
            Pos2::new(track_outer_right - inset, rect.center().y + XP_TRACK_H * 0.5),
        )
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, args: &SliderArgs, state: SliderState) {
        let p = self.palette;
        let ppp = ui.ctx().pixels_per_point();
        let track = self.track_rect(rect, args).round_to_pixels(ppp);

        paint_xp_track(ui, track, state.disabled, p);

        let thumb_x = track.left() + track.width() * state.t;
        let thumb_rect = Rect::from_min_size(
            Pos2::new(thumb_x - XP_THUMB_W * 0.5, track.center().y - XP_THUMB_H * 0.5),
            Vec2::new(XP_THUMB_W, XP_THUMB_H),
        )
        .round_to_pixels(ppp);
        paint_xp_thumb(ui, thumb_rect, state, p);

        // Value readout — same right-aligned monospace box the default
        // impl uses; it picks up Luna's flat Field surface automatically.
        if args.show_value {
            let split_x = rect.left() + args.width;
            let value_rect = Rect::from_min_max(Pos2::new(split_x + VALUE_BOX_GAP, rect.top()), rect.right_bottom());
            paint_value_box(ui, value_rect, *args.value, args.decimals, args.suffix);
        }
    }
}

/// Paint the 2-px sunken track. Mimics the CSS box-shadow stack
/// (light on bottom+right outside, dark on top+left outside) by drawing
/// two 1-px frames offset by ±1 px around the fill rect.
fn paint_xp_track(ui: &mut Ui, track: Rect, disabled: bool, p: &LunaPalette) {
    let fill = if disabled {
        toggle_blend(p.track_fill, Color32::WHITE, 0.4)
    } else {
        p.track_fill
    };
    let dark = if disabled {
        toggle_blend(p.track_shadow, Color32::WHITE, 0.5)
    } else {
        p.track_shadow
    };
    let light = p.track_highlight;

    // Inner fill.
    ui.painter().rect_filled(track, CornerRadius::ZERO, fill);

    // Top edge: dark (1 px above the track, full width including corners).
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(track.left() - 1.0, track.top() - 1.0),
            Pos2::new(track.right() + 1.0, track.top()),
        ),
        CornerRadius::ZERO,
        dark,
    );
    // Left edge: dark.
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(track.left() - 1.0, track.top() - 1.0),
            Pos2::new(track.left(), track.bottom() + 1.0),
        ),
        CornerRadius::ZERO,
        dark,
    );
    // Bottom edge: light.
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(track.left() - 1.0, track.bottom()),
            Pos2::new(track.right() + 1.0, track.bottom() + 1.0),
        ),
        CornerRadius::ZERO,
        light,
    );
    // Right edge: light.
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(track.right(), track.top() - 1.0),
            Pos2::new(track.right() + 1.0, track.bottom() + 1.0),
        ),
        CornerRadius::ZERO,
        light,
    );
}

/// Paint the pentagon-shaped XP slider thumb: rectangular body up top,
/// triangular pointer tip below, with a green accent stripe across the
/// top and a 1-px outline.
fn paint_xp_thumb(ui: &mut Ui, rect: Rect, state: SliderState, p: &LunaPalette) {
    let outline = if state.disabled {
        toggle_blend(p.thumb_outline, Color32::WHITE, 0.5)
    } else {
        p.thumb_outline
    };
    let (body_fill, accent_color) = if state.disabled {
        (
            toggle_blend(p.thumb_body_fill, Color32::WHITE, 0.3),
            toggle_blend(p.thumb_accent, p.thumb_body_fill, 0.6),
        )
    } else if state.pressed {
        (toggle_blend(p.thumb_body_fill, Color32::BLACK, 0.10), p.thumb_accent)
    } else if state.hovered {
        (p.thumb_body_highlight, p.thumb_accent)
    } else {
        (p.thumb_body_fill, p.thumb_accent)
    };

    let body = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + XP_THUMB_BODY_H));

    // Pentagon polygon (cw): TL → TR → mid-right → tip → mid-left → TL.
    let tip = Pos2::new(rect.center().x.round(), rect.bottom());
    let pentagon = vec![
        rect.left_top(),
        rect.right_top(),
        Pos2::new(rect.right(), body.bottom()),
        tip,
        Pos2::new(rect.left(), body.bottom()),
    ];

    // Filled pentagon — body fill across the whole shape.
    ui.painter()
        .add(Shape::convex_polygon(pentagon.clone(), body_fill, Stroke::NONE));

    // Right-edge subtle shadow on the body, matching the SVG's
    // `#c3c3c0` gradient column. 1-px wide strip just inside the right
    // border of the body (not the pointer triangle).
    ui.painter().rect_filled(
        Rect::from_min_max(
            Pos2::new(rect.right() - 1.0, rect.top()),
            Pos2::new(rect.right(), body.bottom()),
        ),
        CornerRadius::ZERO,
        if state.disabled {
            toggle_blend(p.thumb_body_shadow, Color32::WHITE, 0.5)
        } else {
            p.thumb_body_shadow
        },
    );

    // Top accent stripe (Luna green, 3 px). Sits just under the top
    // border so the outline still reads cleanly.
    let stripe = Rect::from_min_max(
        Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, rect.top() + 4.0),
    );
    ui.painter().rect_filled(stripe, CornerRadius::ZERO, accent_color);

    // 1-px outline around the pentagon.
    ui.painter().add(Shape::convex_polygon(
        pentagon,
        Color32::TRANSPARENT,
        Stroke::new(1.0, outline),
    ));

    // Focus ring: thin dotted-style box around the body (approximated as
    // a 1-px stroke in the palette's focus-ring color, like the toggle).
    if state.focused && !state.disabled {
        ui.painter().add(Shape::Rect(RectShape::stroke(
            body.expand(1.0),
            CornerRadius::ZERO,
            Stroke::new(1.0, p.focus_ring),
            StrokeKind::Outside,
        )));
    }
}

// ---------------------------------------------------------------------
// Luna's native UserPresence: square avatar with a sunken bevel, square
// badges with raised bevel, and a non-pulsing solid green border for
// talking. XP UIs were less animation-happy than modern ones — the
// breathing ring would have looked out of place.

pub struct LunaPresence {
    palette: &'static LunaPalette,
}

static LUNA_PRESENCE_LIGHT: LunaPresence = LunaPresence {
    palette: &LunaPalette::LIGHT,
};
static LUNA_PRESENCE_DARK: LunaPresence = LunaPresence {
    palette: &LunaPalette::DARK,
};

impl PresenceImpl for LunaPresence {
    fn layout(&self, ui: &Ui, args: &PresenceArgs) -> Vec2 {
        // Same shape as the default — avatar + gap + name galley. The
        // square-vs-round distinction is purely a paint detail.
        measure_default_layout(ui, args)
    }

    fn paint(&self, ui: &mut Ui, rect: Rect, args: &PresenceArgs) {
        let theme = ui.theme();
        let tokens = theme.tokens();
        let p = self.palette;
        let pad_x = tokens.pad_sm;
        let font_body = theme.font(TextRole::Body);

        let ppp = ui.ctx().pixels_per_point();
        let avatar_rect = Rect::from_min_size(
            Pos2::new(rect.left(), rect.center().y - args.size * 0.5),
            Vec2::splat(args.size),
        )
        .round_to_pixels(ppp);

        paint_xp_avatar(ui, avatar_rect, args, &font_body, tokens, p);

        // Talking / server-muted ring. Solid (no animation) — XP convention.
        if args.state.server_muted {
            ui.painter().add(Shape::Rect(RectShape::stroke(
                avatar_rect.expand(2.0),
                CornerRadius::ZERO,
                Stroke::new(2.0, tokens.danger),
                StrokeKind::Outside,
            )));
        } else if args.state.talking {
            ui.painter().add(Shape::Rect(RectShape::stroke(
                avatar_rect.expand(2.0),
                CornerRadius::ZERO,
                Stroke::new(2.0, p.talking_ring),
                StrokeKind::Outside,
            )));
        }

        // Badges: square chips at the bottom-right.
        let badge_size = (args.size * 0.45).max(10.0).round();
        let mut badge_x = avatar_rect.right() - badge_size + 1.0;
        let badge_y = avatar_rect.bottom() - badge_size + 1.0;

        let muted = args.state.muted || args.state.server_muted;
        if muted {
            paint_xp_badge(
                ui,
                Rect::from_min_size(Pos2::new(badge_x, badge_y), Vec2::splat(badge_size)),
                p.badge_muted,
                'M',
                font_body.clone(),
                p,
            );
            badge_x -= badge_size + 1.0;
        }
        if args.state.deafened {
            paint_xp_badge(
                ui,
                Rect::from_min_size(Pos2::new(badge_x, badge_y), Vec2::splat(badge_size)),
                p.badge_deafened,
                'D',
                font_body.clone(),
                p,
            );
        }

        // Name to the right of the avatar — uses the shared accessible
        // helper so kittest's `get_by_label` keeps working.
        if let Some(name) = args.name.as_deref() {
            let text_color = if args.state.away {
                tokens.text_muted
            } else {
                tokens.text
            };
            let name_rect = Rect::from_min_max(Pos2::new(avatar_rect.right() + pad_x, rect.top()), rect.max);
            paint_name(ui, name_rect, name, text_color);
        }

        // Away: dim the whole row last using the palette's surface color
        // at low alpha — works for light (dim toward white) and dark (dim
        // toward charcoal).
        if args.state.away {
            ui.painter().rect_filled(
                rect,
                CornerRadius::ZERO,
                Color32::from_rgba_unmultiplied(p.surface.r(), p.surface.g(), p.surface.b(), 110),
            );
        }
    }
}

/// Paint the square avatar with sunken 1-px bevel: dark on top+left,
/// light on bottom+right (Win9x/XP "etched in" look). Initial in the
/// center, sized to fit the box.
fn paint_xp_avatar(
    ui: &mut Ui,
    rect: Rect,
    args: &PresenceArgs,
    font_body: &eframe::egui::FontId,
    tokens: &Tokens,
    p: &LunaPalette,
) {
    // Background fill.
    ui.painter().rect_filled(rect, CornerRadius::ZERO, p.avatar_bg);

    // Sunken bevel — dark on top+left, light on bottom+right.
    let painter = ui.painter();
    painter.line_segment(
        [rect.left_top(), Pos2::new(rect.right(), rect.top())],
        Stroke::new(1.0, p.avatar_shadow),
    );
    painter.line_segment(
        [rect.left_top(), Pos2::new(rect.left(), rect.bottom())],
        Stroke::new(1.0, p.avatar_shadow),
    );
    painter.line_segment(
        [Pos2::new(rect.left(), rect.bottom()), rect.right_bottom()],
        Stroke::new(1.0, p.avatar_highlight),
    );
    painter.line_segment(
        [Pos2::new(rect.right(), rect.top()), rect.right_bottom()],
        Stroke::new(1.0, p.avatar_highlight),
    );

    // Initial in the center.
    if let Some(name) = args.name.as_deref()
        && let Some(ch) = name.chars().next()
    {
        let initial = ch.to_uppercase().to_string();
        let mut font = font_body.clone();
        font.size = (args.size * 0.55).max(1.0);
        ui.painter().text(
            rect.center(),
            eframe::egui::Align2::CENTER_CENTER,
            initial,
            font,
            tokens.text,
        );
    }
}

/// Square badge with a 1-px raised bevel and a single-letter glyph.
fn paint_xp_badge(
    ui: &mut Ui,
    rect: Rect,
    fill: Color32,
    glyph: char,
    mut font: eframe::egui::FontId,
    p: &LunaPalette,
) {
    ui.painter().rect_filled(rect, CornerRadius::ZERO, fill);
    // 1-px outer border for crisp definition at small sizes.
    ui.painter().add(Shape::Rect(RectShape::stroke(
        rect,
        CornerRadius::ZERO,
        Stroke::new(1.0, p.badge_border),
        StrokeKind::Inside,
    )));
    font.size = (rect.width() * 0.78).max(1.0);
    ui.painter().text(
        rect.center(),
        eframe::egui::Align2::CENTER_CENTER,
        glyph.to_string(),
        font,
        p.badge_text,
    );
}

// ---------------------------------------------------------------------
// Luna's native Tree: classic Win9x/XP +/− expander box. Everything else
// (row chrome, drop indicator) inherits the trait defaults — Luna's
// `selection()` already paints the right blue highlight, and a 2 px
// accent line is the right answer for the drop indicator regardless of
// theme.

const XP_CARET_BOX: f32 = 9.0;

pub struct LunaTree {
    palette: &'static LunaPalette,
}

static LUNA_TREE_LIGHT: LunaTree = LunaTree {
    palette: &LunaPalette::LIGHT,
};
static LUNA_TREE_DARK: LunaTree = LunaTree {
    palette: &LunaPalette::DARK,
};

impl TreeImpl for LunaTree {
    fn caret_width(&self) -> f32 {
        // 9-px box + 2 px breathing room on each side.
        XP_CARET_BOX + 4.0
    }

    fn paint_row_connectors(
        &self,
        ui: &mut Ui,
        rect: Rect,
        row_left_x: f32,
        depth: usize,
        ancestor_has_next: &[bool],
        indent: f32,
        caret_w: f32,
        pad: f32,
    ) {
        if depth == 0 {
            return;
        }
        // XP tree-line grey. We pick from the palette so dark Luna gets
        // a lighter dot — the classic Explorer look is a single neutral
        // mid-grey.
        let color = self.palette.caret_border;
        let mid_y = rect.center().y;

        // Columns 0..depth: draw vertical lines per ancestor slot.
        for (i, has_next) in ancestor_has_next.iter().enumerate() {
            let col_x = row_left_x + pad + (i as f32) * indent + caret_w * 0.5;
            let (y_start, y_end) = if *has_next {
                // More siblings at this depth below — full height.
                (rect.top(), rect.bottom())
            } else if i + 1 == depth {
                // Last-child vertical: top-half only, stopping at the
                // horizontal spur.
                (rect.top(), mid_y)
            } else {
                // Subtree closed above this row — no line at this column.
                continue;
            };
            paint_dotted_v(ui, col_x, y_start, y_end, color);
        }

        // Horizontal spur from the parent column to the current row's
        // caret/content, at row mid-height.
        let spur_start = row_left_x + pad + ((depth - 1) as f32) * indent + caret_w * 0.5;
        let spur_end = row_left_x + pad + (depth as f32) * indent;
        paint_dotted_h(ui, spur_start, spur_end, mid_y, color);
    }

    fn paint_caret(&self, ui: &mut Ui, rect: Rect, expanded: bool, _color: Color32) {
        let p = self.palette;
        let ppp = ui.ctx().pixels_per_point();
        // Center a 9×9 box inside the caret column. Snap so the +/−
        // glyph line segments co-align with the box edges.
        let center = rect.center();
        let half = XP_CARET_BOX * 0.5;
        let box_rect = Rect::from_min_size(Pos2::new(center.x - half, center.y - half), Vec2::splat(XP_CARET_BOX))
            .round_to_pixels(ppp);

        // Caret fill + 1-px border.
        ui.painter().rect_filled(box_rect, CornerRadius::ZERO, p.caret_fill);
        ui.painter().add(Shape::Rect(RectShape::stroke(
            box_rect,
            CornerRadius::ZERO,
            Stroke::new(1.0, p.caret_border),
            StrokeKind::Inside,
        )));

        // Glyph: horizontal bar always (the "−" of "+/−"), plus a vertical
        // bar when collapsed (so the "+" reads as expand).
        let bar_color = p.caret_glyph;
        let inset = 2.0;
        let mid_y = box_rect.center().y.round();
        let mid_x = box_rect.center().x.round();
        ui.painter().line_segment(
            [
                Pos2::new(box_rect.left() + inset, mid_y),
                Pos2::new(box_rect.right() - inset, mid_y),
            ],
            Stroke::new(1.0, bar_color),
        );
        if !expanded {
            ui.painter().line_segment(
                [
                    Pos2::new(mid_x, box_rect.top() + inset),
                    Pos2::new(mid_x, box_rect.bottom() - inset),
                ],
                Stroke::new(1.0, bar_color),
            );
        }
    }
}

// ---------------------------------------------------------------------
// Luna's native LevelMeter: segmented LED bricks with the XP progress
// bar's 3-D vertical gradient (light at top → base color in the middle
// → white shine at the bottom) on a white sunken-bevel field. Unlit
// bricks aren't drawn — XP's progress bar fills empty space with the
// field's white background and we follow that convention.
//
// The XP CSS reference is in `reference/XP.css/themes/XP/_progressbar.scss`.
// Their gradient stops:
//   0%   #acedad  (light green top)
//   14%  #7be47d
//   28%  #4cda50
//   42%  #2ed330  (mid: deepest tone)
//   57%  #42d845
//   71%  #76e275
//   85%  #8fe791
//   100% #ffffff  (white bottom shine)
// We approximate that programmatically per zone color so yellow/red/etc.
// get the same treatment without hard-coding 8 stops × N zones.

const LUNA_SEGMENTS: usize = 20;
const LUNA_SEGMENT_GAP: f32 = 1.0;

pub struct LunaLevelMeter {
    #[allow(dead_code)]
    palette: &'static LunaPalette,
}

static LUNA_LEVEL_METER_LIGHT: LunaLevelMeter = LunaLevelMeter {
    palette: &LunaPalette::LIGHT,
};
static LUNA_LEVEL_METER_DARK: LunaLevelMeter = LunaLevelMeter {
    palette: &LunaPalette::DARK,
};

impl LevelMeterImpl for LunaLevelMeter {
    fn paint(&self, ui: &mut Ui, rect: Rect, args: &LevelMeterArgs) {
        let theme = ui.theme();
        let tokens = theme.tokens();

        // White field with the same flat sunken border a TextInput uses —
        // matches XP's `border: 1px solid #686868` on the progress bar
        // closely enough that the meter reads as the same family.
        ui.painter().add(theme.surface(rect, SurfaceKind::Field));

        let inner = rect.shrink(1.0);
        let zones = resolve_zones(args.zones.as_deref(), args.vad, tokens.talking);

        // Lit bricks only — empty space is the white field background, XP
        // style. For each brick within the lit range, paint a 5-stop
        // gradient driven by that brick's zone color.
        for i in 0..LUNA_SEGMENTS {
            let lo = i as f32 / LUNA_SEGMENTS as f32;
            let hi = (i as f32 + 1.0) / LUNA_SEGMENTS as f32;
            let mid = (lo + hi) * 0.5;
            if mid > args.level {
                break;
            }
            let zone_color = color_for_level(&zones, mid, tokens.talking);
            let brick = brick_rect(inner, lo, hi, args.orientation, LUNA_SEGMENT_GAP);
            if brick.area() > 0.5 {
                paint_xp_progress_brick(ui, brick, zone_color, args.orientation);
            }
        }

        // Peak hold: bright high-water tick at the peak position, even if
        // the current level has dropped past it.
        if let Some(peak) = args.peak {
            let peak_idx = ((peak * LUNA_SEGMENTS as f32).floor() as usize).min(LUNA_SEGMENTS - 1);
            let lo = peak_idx as f32 / LUNA_SEGMENTS as f32;
            let hi = (peak_idx as f32 + 1.0) / LUNA_SEGMENTS as f32;
            let mid = (lo + hi) * 0.5;
            let zone_color = color_for_level(&zones, mid, tokens.talking);
            let brick = brick_rect(inner, lo, hi, args.orientation, LUNA_SEGMENT_GAP);
            paint_xp_progress_brick(ui, brick, zone_color, args.orientation);
        }

        let markers = resolve_markers(&args.markers, args.threshold, args.vad, tokens.accent);
        for (level, color) in markers {
            paint_marker(ui, rect, level, args.orientation, Stroke::new(2.0, color));
        }
    }
}

/// Paint one brick with the XP progress-bar gradient. For horizontal
/// meters the gradient runs top → bottom (light highlight up top, white
/// shine at the bottom); for vertical meters we rotate the gradient to
/// run left → right so the 3-D tube look points across the bar's short
/// axis.
fn paint_xp_progress_brick(ui: &mut Ui, rect: Rect, base: Color32, axis: Axis) {
    let stops = xp_progress_stops(base);
    let strips = match axis {
        Axis::Horizontal => brick_strips_y(rect, &stops, 6),
        Axis::Vertical => brick_strips_x(rect, &stops, 6),
    };
    for strip in strips {
        ui.painter().add(strip);
    }
}

/// Compute 5 gradient stops mimicking the XP progress-bar tube look:
/// light highlight at top → base near upper-mid → slight rebound at
/// lower-mid → bright white shine at the bottom.
fn xp_progress_stops(base: Color32) -> Vec<(f32, Color32)> {
    vec![
        (0.00, blend(base, Color32::WHITE, 0.55)),
        (0.20, blend(base, Color32::WHITE, 0.25)),
        (0.50, base),
        (0.80, blend(base, Color32::WHITE, 0.20)),
        (1.00, blend(base, Color32::WHITE, 0.75)),
    ]
}

/// Brick strips along the Y axis. Six steps is enough to read as a
/// smooth gradient at 12-14 px tall without overpainting.
fn brick_strips_y(rect: Rect, stops: &[(f32, Color32)], steps: usize) -> Vec<Shape> {
    let steps = steps.max(2);
    let mut shapes = Vec::with_capacity(steps);
    let h = rect.height() / steps as f32;
    for i in 0..steps {
        let t_mid = (i as f32 + 0.5) / steps as f32;
        let color = sample_stops(stops, t_mid);
        let strip = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + h * i as f32),
            Pos2::new(rect.right(), rect.top() + h * (i + 1) as f32 + 0.5),
        );
        shapes.push(Shape::rect_filled(strip, CornerRadius::ZERO, color));
    }
    shapes
}

/// Brick strips along the X axis (vertical meters).
fn brick_strips_x(rect: Rect, stops: &[(f32, Color32)], steps: usize) -> Vec<Shape> {
    let steps = steps.max(2);
    let mut shapes = Vec::with_capacity(steps);
    let w = rect.width() / steps as f32;
    for i in 0..steps {
        let t_mid = (i as f32 + 0.5) / steps as f32;
        let color = sample_stops(stops, t_mid);
        let strip = Rect::from_min_max(
            Pos2::new(rect.left() + w * i as f32, rect.top()),
            Pos2::new(rect.left() + w * (i + 1) as f32 + 0.5, rect.bottom()),
        );
        shapes.push(Shape::rect_filled(strip, CornerRadius::ZERO, color));
    }
    shapes
}

/// Pick the color for a normalized position from the zone table — first
/// boundary that the position falls within or below wins.
fn color_for_level(zones: &[(f32, Color32)], pos: f32, fallback: Color32) -> Color32 {
    for (boundary, color) in zones {
        if pos <= *boundary {
            return *color;
        }
    }
    fallback
}

/// Compute the rect for one brick along the meter's long axis, leaving
/// a `gap` px gutter on each side along that axis.
fn brick_rect(inner: Rect, lo: f32, hi: f32, axis: Axis, gap: f32) -> Rect {
    match axis {
        Axis::Horizontal => {
            let l = inner.left() + inner.width() * lo + gap * 0.5;
            let r = inner.left() + inner.width() * hi - gap * 0.5;
            Rect::from_min_max(Pos2::new(l, inner.top()), Pos2::new(r, inner.bottom()))
        }
        Axis::Vertical => {
            // Vertical: lo=0 is bottom, hi=1 is top.
            let b = inner.bottom() - inner.height() * lo - gap * 0.5;
            let t = inner.bottom() - inner.height() * hi + gap * 0.5;
            Rect::from_min_max(Pos2::new(inner.left(), t), Pos2::new(inner.right(), b))
        }
    }
}

/// Paint a stack of CSS-style inner-glow layers, bottom-up (first entry
/// wins visually). Each layer is an L-shape: full-width horizontal strips
/// at `top` / `bottom` and full-height vertical strips at `left` / `right`.
fn glow_layers(rect: Rect, layers: &[GlowLayer]) -> Vec<Shape> {
    // Sit inside the 1px button border so the rounded border corners stay
    // visible around the glow.
    let inner = rect.shrink(1.0);
    let mut shapes = Vec::new();
    // Paint last-listed first so first-listed lands on top.
    for layer in layers.iter().rev() {
        if layer.top > 0.0 {
            shapes.push(Shape::rect_filled(
                Rect::from_min_max(inner.left_top(), Pos2::new(inner.right(), inner.top() + layer.top)),
                CornerRadius::ZERO,
                layer.color,
            ));
        }
        if layer.bottom > 0.0 {
            shapes.push(Shape::rect_filled(
                Rect::from_min_max(
                    Pos2::new(inner.left(), inner.bottom() - layer.bottom),
                    inner.right_bottom(),
                ),
                CornerRadius::ZERO,
                layer.color,
            ));
        }
        if layer.left > 0.0 {
            shapes.push(Shape::rect_filled(
                Rect::from_min_max(inner.left_top(), Pos2::new(inner.left() + layer.left, inner.bottom())),
                CornerRadius::ZERO,
                layer.color,
            ));
        }
        if layer.right > 0.0 {
            shapes.push(Shape::rect_filled(
                Rect::from_min_max(
                    Pos2::new(inner.right() - layer.right, inner.top()),
                    inner.right_bottom(),
                ),
                CornerRadius::ZERO,
                layer.color,
            ));
        }
    }
    shapes
}

/// Gradient for a titlebar-style shape: rounded top corners, square bottom.
fn titlebar_gradient(rect: Rect, stops: &[(f32, Color32)], steps: usize, top_radius: f32) -> Vec<Shape> {
    let r = top_radius.max(0.0).min(rect.height() * 0.5);
    if r <= 0.0 {
        return square_strips(rect, stops, steps);
    }
    let cap_h = 2.0 * r;
    let mut shapes = Vec::new();

    // Rounded top cap — needs to be at least 2r tall so epaint's tessellator
    // doesn't clamp the radius and leak fill past the corner curve.
    let top_cap = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + cap_h));
    let top_t = (cap_h * 0.5) / rect.height();
    shapes.push(Shape::Rect(RectShape::new(
        top_cap,
        CornerRadius {
            nw: r.round() as u8,
            ne: r.round() as u8,
            sw: 0,
            se: 0,
        },
        sample_stops(stops, top_t),
        Stroke::NONE,
        StrokeKind::Inside,
    )));

    // Body: square strips covering the remainder.
    let body = Rect::from_min_max(Pos2::new(rect.left(), rect.top() + cap_h), rect.max);
    shapes.extend(square_strips_offset(body, stops, steps, rect));
    shapes
}

/// Build a vertical gradient as stacked horizontal strips.
///
/// `radius` is the button's corner radius; pass `0.0` for a square-cornered
/// surface (titlebar, toolbar). A non-zero radius makes the top and bottom
/// strips into rounded caps so the fill doesn't bleed past the border.
fn vertical_gradient(rect: Rect, stops: &[(f32, Color32)], steps: usize, radius: f32) -> Vec<Shape> {
    assert!(stops.len() >= 2);
    let r = radius.max(0.0).min(rect.height() * 0.5);
    let h = rect.height();

    if r <= 0.0 {
        return square_strips(rect, stops, steps);
    }

    // Each cap must be at least 2r tall, otherwise epaint's tessellator
    // clamps the corner radius to half the cap height and the gradient
    // fill bleeds past the corner curve.
    let cap_h = 2.0 * r;

    // Button too short for separate caps — fall back to a single rounded fill.
    if h <= 2.0 * cap_h {
        let color = sample_stops(stops, 0.5);
        return vec![Shape::rect_filled(rect, CornerRadius::from(r), color)];
    }

    let mut shapes = Vec::new();

    // Top cap.
    let top_cap = Rect::from_min_max(rect.min, Pos2::new(rect.right(), rect.top() + cap_h));
    let top_t = (cap_h * 0.5) / h;
    shapes.push(Shape::Rect(RectShape::new(
        top_cap,
        CornerRadius {
            nw: r.round() as u8,
            ne: r.round() as u8,
            sw: 0,
            se: 0,
        },
        sample_stops(stops, top_t),
        Stroke::NONE,
        StrokeKind::Inside,
    )));

    // Middle body — square strips, safely away from the corner region.
    let body = Rect::from_min_max(
        Pos2::new(rect.left(), rect.top() + cap_h),
        Pos2::new(rect.right(), rect.bottom() - cap_h),
    );
    shapes.extend(square_strips_offset(body, stops, steps, rect));

    // Bottom cap.
    let bot_cap = Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - cap_h), rect.max);
    let bot_t = (h - cap_h * 0.5) / h;
    shapes.push(Shape::Rect(RectShape::new(
        bot_cap,
        CornerRadius {
            nw: 0,
            ne: 0,
            sw: r.round() as u8,
            se: r.round() as u8,
        },
        sample_stops(stops, bot_t),
        Stroke::NONE,
        StrokeKind::Inside,
    )));

    shapes
}

fn square_strips(rect: Rect, stops: &[(f32, Color32)], steps: usize) -> Vec<Shape> {
    let steps = steps.max(2);
    let mut shapes = Vec::with_capacity(steps);
    let h = rect.height() / steps as f32;
    for i in 0..steps {
        let t_mid = (i as f32 + 0.5) / steps as f32;
        let color = sample_stops(stops, t_mid);
        let strip = Rect::from_min_max(
            Pos2::new(rect.left(), rect.top() + h * i as f32),
            Pos2::new(rect.right(), rect.top() + h * (i + 1) as f32 + 0.5),
        );
        shapes.push(Shape::rect_filled(strip, CornerRadius::ZERO, color));
    }
    shapes
}

/// Like `square_strips`, but samples gradient stops using the strip's position
/// within an enclosing rect (used so the body strips continue the caps' t-range).
fn square_strips_offset(body: Rect, stops: &[(f32, Color32)], steps: usize, enclosing: Rect) -> Vec<Shape> {
    let steps = steps.max(1);
    let mut shapes = Vec::with_capacity(steps);
    let strip_h = body.height() / steps as f32;
    let full_h = enclosing.height().max(f32::EPSILON);
    for i in 0..steps {
        let y0 = body.top() + strip_h * i as f32;
        let y1 = y0 + strip_h + 0.5;
        let y_mid = y0 + strip_h * 0.5;
        let t = (y_mid - enclosing.top()) / full_h;
        let color = sample_stops(stops, t);
        let strip = Rect::from_min_max(Pos2::new(body.left(), y0), Pos2::new(body.right(), y1));
        shapes.push(Shape::rect_filled(strip, CornerRadius::ZERO, color));
    }
    shapes
}

fn sample_stops(stops: &[(f32, Color32)], t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    if t <= stops[0].0 {
        return stops[0].1;
    }
    if t >= stops[stops.len() - 1].0 {
        return stops[stops.len() - 1].1;
    }
    for pair in stops.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if t <= t1 {
            let local = (t - t0) / (t1 - t0);
            return blend(c0, c1, local);
        }
    }
    stops[stops.len() - 1].1
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

fn darken(c: Color32, t: f32) -> Color32 {
    blend(c, Color32::BLACK, t)
}
