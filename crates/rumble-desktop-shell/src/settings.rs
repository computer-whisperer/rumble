//! Settings schema + JSON persistence shared by `rumble-egui` and
//! `rumble-next`. See `docs/rumble-next-bringup.md` §7.
//!
//! The schema is small on purpose. Each field lands as the matching
//! feature lands; the only fields that exist today are the ones rumble-
//! next needs to fix the cert-reprompt and remember the user's UI
//! choices (`paradigm`, `dark`, `recent_servers`,
//! `accepted_certificates`).
//!
//! Forward / backward compatibility:
//! - `#[serde(default)]` on the struct → missing fields use defaults,
//!   so older config files load cleanly when we add new fields.
//! - The `_extra` map captures unknown fields and round-trips them on
//!   save, so when a *newer* client (or `rumble-egui`) writes fields
//!   this version doesn't know about, an older `rumble-next` won't
//!   silently delete them.
//!
//! The store path defaults to `<ProjectDirs>/desktop-shell.json`,
//! distinct from rumble-egui's existing `settings.json` so the two
//! schemas don't fight during the transition. They unify when
//! rumble-egui adopts this store (see bringup doc).

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hotkeys::KeyboardSettings;

/// How chat timestamps are rendered in the UI.
///
/// Variant set + ordering match `rumble-egui`'s `TimestampFormat` so
/// settings files port cleanly between clients. Default is `Time24h`,
/// which matches Mumble's longstanding behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TimestampFormat {
    #[default]
    Time24h,
    Time12h,
    DateTime24h,
    DateTime12h,
    Relative,
}

impl TimestampFormat {
    pub const ALL: &'static [TimestampFormat] = &[
        TimestampFormat::Time24h,
        TimestampFormat::Time12h,
        TimestampFormat::DateTime24h,
        TimestampFormat::DateTime12h,
        TimestampFormat::Relative,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TimestampFormat::Time24h => "24-hour (14:30)",
            TimestampFormat::Time12h => "12-hour (2:30 PM)",
            TimestampFormat::DateTime24h => "Date + 24h",
            TimestampFormat::DateTime12h => "Date + 12h",
            TimestampFormat::Relative => "Relative (5m ago)",
        }
    }
}

/// Sound-effect playback preferences. Volume applies on top of the
/// per-call `Command::PlaySfx { volume }` parameter, so `1.0` means
/// "play at the volume the caller asked for"; `0.0` mutes everything
/// without disabling the toggle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SfxSettings {
    pub enabled: bool,
    pub volume: f32,
}

impl Default for SfxSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.6,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatSettings {
    /// Whether to render `[hh:mm]` style timestamps next to messages.
    pub show_timestamps: bool,
    /// Format used when timestamps are visible.
    pub timestamp_format: TimestampFormat,
    /// On joining a room, ask peers for their backlog so the user
    /// sees what was said before they arrived.
    pub auto_sync_history: bool,
}

/// Default location for the shared shell settings file.
///
/// Returns `None` only on platforms where `directories` cannot resolve
/// a config dir; callers should treat that as "settings disabled" and
/// run with defaults.
pub fn default_settings_path() -> Option<PathBuf> {
    ProjectDirs::from("com", "rumble", "Rumble").map(|dirs| dirs.config_dir().join("desktop-shell.json"))
}

/// Server the user has connected to before. Populated as the connect
/// flow learns about new servers; consumed by the upcoming recent-
/// servers UI (bringup doc §2).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RecentServer {
    /// Address used to dial (e.g. `127.0.0.1:5000`).
    pub addr: String,
    /// User-chosen display name for the server, or empty if none.
    #[serde(default)]
    pub label: String,
    /// Username last used against this server.
    #[serde(default)]
    pub username: String,
    /// Last connect timestamp (unix seconds), used to sort the list.
    #[serde(default)]
    pub last_used_unix: u64,
}

/// A server certificate the user has approved.
///
/// Field names match `rumble-egui`'s `AcceptedCertificate` so the
/// schemas line up when rumble-egui migrates to this store.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AcceptedCertificate {
    /// Server address/name the certificate was accepted for.
    pub server_name: String,
    /// SHA256 fingerprint as hex string.
    pub fingerprint_hex: String,
    /// DER-encoded certificate as base64 string.
    pub certificate_der_base64: String,
}

impl AcceptedCertificate {
    /// Build an entry from raw DER bytes. Base64-encodes for storage.
    pub fn from_der(server_name: impl Into<String>, fingerprint_hex: impl Into<String>, der: &[u8]) -> Self {
        use base64::{Engine, engine::general_purpose::STANDARD};
        Self {
            server_name: server_name.into(),
            fingerprint_hex: fingerprint_hex.into(),
            certificate_der_base64: STANDARD.encode(der),
        }
    }

    /// Decode the stored cert back to DER bytes. Returns `None` if the
    /// stored base64 is malformed.
    pub fn der_bytes(&self) -> Option<Vec<u8>> {
        use base64::{Engine, engine::general_purpose::STANDARD};
        STANDARD.decode(&self.certificate_der_base64).ok()
    }
}

/// Persistent settings shared by all desktop GUI clients.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Active paradigm (e.g. `"Modern"`). Stored as a string so this
    /// crate doesn't need to know the enum's variants.
    pub paradigm: Option<String>,
    /// Dark mode toggle.
    pub dark: bool,
    /// Servers the user has connected to before, newest last.
    pub recent_servers: Vec<RecentServer>,
    /// Approved server certificates, keyed implicitly by `server_name`.
    pub accepted_certificates: Vec<AcceptedCertificate>,
    /// Server address to auto-connect on launch. Must match a
    /// `RecentServer.addr` to take effect; clearing this disables
    /// auto-connect.
    pub auto_connect_addr: Option<String>,
    /// Hotkey bindings (PTT, mute, deafen) and the global-hotkey
    /// enable flag. Defaults to PTT=Space.
    pub keyboard: KeyboardSettings,
    /// Chat display preferences.
    pub chat: ChatSettings,
    /// Sound effect playback preferences.
    pub sfx: SfxSettings,

    /// Catch-all for fields written by a newer client. Round-tripped
    /// on save so we don't silently delete unknown settings.
    #[serde(flatten)]
    _extra: HashMap<String, Value>,
}

/// In-memory wrapper that loads `Settings` from disk and saves changes
/// after each mutation.
///
/// Save is synchronous and runs on every `modify()`. The file is small
/// (< 4 KB once populated), so debouncing buys us nothing yet — we can
/// add it later if a settings-heavy UI starts pegging the disk.
pub struct SettingsStore {
    settings: Settings,
    path: Option<PathBuf>,
}

impl SettingsStore {
    /// Load from the default path (`<config>/desktop-shell.json`).
    /// Always returns a usable store: parse errors and missing files
    /// fall through to defaults, with a warning logged.
    pub fn load_default() -> Self {
        Self::load_from_path(default_settings_path())
    }

    /// Load from an explicit path. Pass `None` to run in-memory only
    /// (useful for tests / headless renders).
    pub fn load_from_path(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            tracing::debug!("settings: no path configured, running in-memory");
            return Self {
                settings: Settings::default(),
                path: None,
            };
        };

        let settings = match fs::read_to_string(&path) {
            Ok(text) => match serde_json::from_str::<Settings>(&text) {
                Ok(s) => {
                    tracing::info!("settings: loaded {}", path.display());
                    s
                }
                Err(err) => {
                    tracing::warn!("settings: failed to parse {}: {err} — using defaults", path.display());
                    Settings::default()
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                tracing::info!("settings: {} not found, using defaults", path.display());
                Settings::default()
            }
            Err(err) => {
                tracing::warn!("settings: could not read {}: {err} — using defaults", path.display());
                Settings::default()
            }
        };

        Self {
            settings,
            path: Some(path),
        }
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Run `f` against the inner `Settings`, then persist if anything
    /// changed. Returns `f`'s value so callers can chain.
    ///
    /// Save errors are logged but not surfaced — settings persistence
    /// is best-effort, and we'd rather drop a save than break the UI.
    pub fn modify<R>(&mut self, f: impl FnOnce(&mut Settings) -> R) -> R {
        let result = f(&mut self.settings);
        self.save();
        result
    }

    /// Force a save without modifying anything (e.g. during shutdown).
    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(parent) = path.parent()
            && let Err(err) = fs::create_dir_all(parent)
        {
            tracing::warn!("settings: could not create {}: {err}", parent.display());
            return;
        }
        let json = match serde_json::to_string_pretty(&self.settings) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!("settings: serialize failed: {err}");
                return;
            }
        };
        if let Err(err) = fs::write(path, json) {
            tracing::warn!("settings: write to {} failed: {err}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_unknown_fields() {
        // A newer client wrote `future_field`. Loading it into our
        // schema should preserve it on the next save.
        let json = r#"{ "dark": true, "future_field": {"nested": 1} }"#;
        let parsed: Settings = serde_json::from_str(json).unwrap();
        assert!(parsed.dark);

        let reserialized = serde_json::to_string(&parsed).unwrap();
        assert!(reserialized.contains("future_field"));
        assert!(reserialized.contains("nested"));
    }

    #[test]
    fn defaults_when_file_missing() {
        let store = SettingsStore::load_from_path(Some(PathBuf::from(
            "/tmp/rumble-desktop-shell-nonexistent-test-file.json",
        )));
        assert!(store.settings().paradigm.is_none());
        assert!(!store.settings().dark);
        assert!(store.settings().recent_servers.is_empty());
    }

    #[test]
    fn modify_saves() {
        let dir = tempdir_for_test();
        let path = dir.join("settings.json");
        {
            let mut store = SettingsStore::load_from_path(Some(path.clone()));
            store.modify(|s| {
                s.dark = true;
                s.paradigm = Some("Luna".into());
            });
        }
        let reread = SettingsStore::load_from_path(Some(path));
        assert!(reread.settings().dark);
        assert_eq!(reread.settings().paradigm.as_deref(), Some("Luna"));
    }

    fn tempdir_for_test() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!(
            "rumble-desktop-shell-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }
}
