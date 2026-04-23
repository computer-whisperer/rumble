use eframe::egui::{Color32, FontId};

#[derive(Clone, Debug)]
pub struct Tokens {
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

    pub radius_sm: f32,
    pub radius_md: f32,
    pub radius_pill: f32,

    pub pad_sm: f32,
    pub pad_md: f32,

    /// How far inside a `SurfaceKind::Field` rect a widget must inset
    /// before painting content, to avoid stepping on the field's bevel /
    /// border stroke. Themes draw their bevel inside the rect so this is
    /// the gap between the rect edge and "safe" content area.
    pub bevel_inset: f32,

    pub font_body: FontId,
    pub font_label: FontId,
    pub font_heading: FontId,
    pub font_mono: FontId,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SurfaceKind {
    Panel,
    Pane,
    Group,
    Titlebar,
    Statusbar,
    Toolbar,
    Tooltip,
    Popup,
    Field,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum PressableRole {
    /// Default button; used for most actions.
    Default,
    /// Confirm/apply/commit.
    Primary,
    /// Deafen-on, kick, destructive actions.
    Danger,
    /// PTT active, selected tab, "on" accent toggle.
    Accent,
    /// Minimal chrome, e.g. toolbar icon button.
    Ghost,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub struct PressableState {
    pub hovered: bool,
    pub pressed: bool,
    /// App-supplied: "toggle on", "held", "selected".
    pub active: bool,
    pub focused: bool,
    pub disabled: bool,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TextRole {
    Body,
    Label,
    Heading,
    Caption,
    Mono,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Axis {
    Horizontal,
    Vertical,
}
