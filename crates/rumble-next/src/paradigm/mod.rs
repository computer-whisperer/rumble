//! Per-paradigm chrome: the top bar / toolbar / titlebar around the
//! shared shell. Each paradigm also picks its default theme.

use std::sync::Arc;

use rumble_widgets::{LunaTheme, ModernTheme, MumbleLiteTheme, Theme};

pub mod luna;
pub mod modern;
pub mod mumble;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Paradigm {
    /// Web-native top bar + pill search + avatar pill. ModernTheme.
    Modern,
    /// Classic menubar + toolbar + statusbar, Mumble-style. MumbleLiteTheme.
    MumbleClassic,
    /// Windows XP Luna — bevelled toolbar, coloured buttons. LunaTheme.
    Luna,
}

impl Paradigm {
    pub const ALL: &'static [Paradigm] = &[Self::Modern, Self::MumbleClassic, Self::Luna];

    pub fn label(self) -> &'static str {
        match self {
            Self::Modern => "Modern",
            Self::MumbleClassic => "Mumble Classic",
            Self::Luna => "Luna (XP)",
        }
    }

    /// Stable identifier used in the settings file. Must not change
    /// across releases; renaming a variant requires migration.
    pub fn as_persist_str(self) -> &'static str {
        match self {
            Self::Modern => "Modern",
            Self::MumbleClassic => "MumbleClassic",
            Self::Luna => "Luna",
        }
    }

    /// Inverse of `as_persist_str`. Unknown values fall back to the
    /// caller-supplied default.
    pub fn from_persist_str(s: &str) -> Option<Self> {
        match s {
            "Modern" => Some(Self::Modern),
            "MumbleClassic" => Some(Self::MumbleClassic),
            "Luna" => Some(Self::Luna),
            _ => None,
        }
    }

    /// Return the theme for this paradigm at the requested brightness.
    /// Every paradigm in this crate has both a light and a dark variant.
    pub fn make_theme(self, dark: bool) -> Arc<dyn Theme> {
        match (self, dark) {
            (Self::Modern, false) => Arc::new(ModernTheme::light()),
            (Self::Modern, true) => Arc::new(ModernTheme::dark()),
            (Self::MumbleClassic, false) => Arc::new(MumbleLiteTheme::light()),
            (Self::MumbleClassic, true) => Arc::new(MumbleLiteTheme::dark()),
            (Self::Luna, false) => Arc::new(LunaTheme::light()),
            (Self::Luna, true) => Arc::new(LunaTheme::dark()),
        }
    }
}
