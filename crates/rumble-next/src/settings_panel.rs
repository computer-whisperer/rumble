//! Settings overlay — a modal window reachable from each paradigm's
//! chrome. Organised as categories (Connection, Voice, Devices, About)
//! that read the live state and emit backend `Command`s on change.
//!
//! Kept small on purpose: this is the first-pass parity with
//! `rumble-egui`'s multi-page settings dialog. Categories that require
//! deeper UI (pipelines, ACL admin) can grow here incrementally.

use eframe::egui::{self, Align, Layout, Margin, RichText, Ui};
use rumble_client::{PipelineConfig, ProcessorRegistry, handle::BackendHandle};
use rumble_client_traits::Platform;
use rumble_desktop_shell::{SettingsStore, TimestampFormat};
use rumble_protocol::{Command, ConnectionState, State, VoiceMode};
use rumble_widgets::{
    ButtonArgs, ComboBox, GroupBox, PressableRole, Radio, SurfaceFrame, SurfaceKind, TextRole, UiExt,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsCategory {
    Connection,
    Voice,
    Devices,
    Processing,
    Chat,
    Statistics,
    About,
}

impl SettingsCategory {
    pub const ALL: &'static [SettingsCategory] = &[
        Self::Connection,
        Self::Voice,
        Self::Devices,
        Self::Processing,
        Self::Chat,
        Self::Statistics,
        Self::About,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Connection => "Connection",
            Self::Voice => "Voice",
            Self::Devices => "Devices",
            Self::Processing => "Processing",
            Self::Chat => "Chat",
            Self::Statistics => "Statistics",
            Self::About => "About",
        }
    }
}

#[derive(Debug)]
pub struct SettingsState {
    pub category: SettingsCategory,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            category: SettingsCategory::Connection,
        }
    }
}

pub fn render<P: Platform + 'static>(
    ctx: &egui::Context,
    open: &mut bool,
    settings: &mut SettingsState,
    store: &mut SettingsStore,
    state: &State,
    backend: &BackendHandle<P>,
    processor_registry: &ProcessorRegistry,
    identity_public_key_hex: &str,
) {
    if !*open {
        return;
    }

    let mut should_close = false;

    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(true)
        .default_size([640.0, 420.0])
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                sidebar(ui, settings);
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_min_width(420.0);
                    match settings.category {
                        SettingsCategory::Connection => connection_page(ui, state, backend),
                        SettingsCategory::Voice => voice_page(ui, state, backend, store),
                        SettingsCategory::Devices => devices_page(ui, state, backend),
                        SettingsCategory::Processing => processing_page(ui, state, backend, processor_registry),
                        SettingsCategory::Chat => chat_page(ui, store),
                        SettingsCategory::Statistics => statistics_page(ui, state),
                        SettingsCategory::About => about_page(ui, identity_public_key_hex),
                    }
                });
            });

            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ButtonArgs::new("Close")
                        .role(PressableRole::Primary)
                        .min_width(90.0)
                        .show(ui)
                        .clicked()
                    {
                        should_close = true;
                    }
                });
            });
        });

    if should_close {
        *open = false;
    }
}

fn sidebar(ui: &mut Ui, settings: &mut SettingsState) {
    ui.vertical(|ui| {
        ui.set_min_width(140.0);
        for c in SettingsCategory::ALL {
            let active = settings.category == *c;
            if ButtonArgs::new(c.label())
                .role(PressableRole::Ghost)
                .active(active)
                .min_width(130.0)
                .show(ui)
                .clicked()
            {
                settings.category = *c;
            }
        }
    });
}

fn connection_page<P: Platform + 'static>(ui: &mut Ui, state: &State, backend: &BackendHandle<P>) {
    ui.label(
        RichText::new("Connection")
            .font(ui.theme().font(TextRole::Heading))
            .strong(),
    );
    ui.add_space(6.0);

    GroupBox::new("Status")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let tokens = ui.theme().tokens().clone();
            let line = match &state.connection {
                ConnectionState::Disconnected => "Not connected".to_string(),
                ConnectionState::Connecting { server_addr } => format!("Connecting to {server_addr}…"),
                ConnectionState::Connected { server_name, user_id } => {
                    format!("Connected to {server_name} as user #{user_id}")
                }
                ConnectionState::ConnectionLost { error } => format!("Lost: {error}"),
                ConnectionState::CertificatePending { cert_info } => {
                    format!("Cert pending · {}", cert_info.fingerprint_short())
                }
            };
            ui.label(RichText::new(line).color(tokens.text).font(tokens.font_body.clone()));
        });

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let connected = state.connection.is_connected();
        if ButtonArgs::new("Disconnect")
            .role(PressableRole::Danger)
            .disabled(!connected)
            .min_width(120.0)
            .show(ui)
            .clicked()
        {
            backend.send(Command::Disconnect);
        }
    });
}

fn voice_page<P: Platform + 'static>(
    ui: &mut Ui,
    state: &State,
    backend: &BackendHandle<P>,
    store: &mut SettingsStore,
) {
    ui.label(RichText::new("Voice").font(ui.theme().font(TextRole::Heading)).strong());
    ui.add_space(6.0);

    GroupBox::new("Activation mode")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let mut mode = state.audio.voice_mode;
            let before = mode;
            ui.vertical(|ui| {
                Radio::new(&mut mode, VoiceMode::PushToTalk, "Push-to-talk").show(ui);
                Radio::new(&mut mode, VoiceMode::Continuous, "Continuous / voice activity").show(ui);
            });
            if mode != before {
                backend.send(Command::SetVoiceMode { mode });
            }
        });

    ui.add_space(8.0);

    GroupBox::new("Self")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let muted = state.audio.self_muted;
            let deafened = state.audio.self_deafened;
            ui.horizontal(|ui| {
                if ButtonArgs::new(if muted { "Unmute microphone" } else { "Mute microphone" })
                    .role(PressableRole::Default)
                    .active(muted)
                    .show(ui)
                    .clicked()
                {
                    backend.send(Command::SetMuted { muted: !muted });
                }
                if ButtonArgs::new(if deafened { "Undeafen" } else { "Deafen" })
                    .role(PressableRole::Danger)
                    .active(deafened)
                    .show(ui)
                    .clicked()
                {
                    backend.send(Command::SetDeafened { deafened: !deafened });
                }
            });
        });

    ui.add_space(8.0);

    GroupBox::new("Sound effects")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let mut enabled = store.settings().sfx.enabled;
            if ui
                .checkbox(&mut enabled, "Play sounds for connect/disconnect/mute toggles")
                .changed()
            {
                store.modify(|s| s.sfx.enabled = enabled);
            }
            ui.add_space(4.0);
            ui.add_enabled_ui(enabled, |ui| {
                let mut volume = store.settings().sfx.volume;
                let before = volume;
                ui.label(
                    RichText::new("Volume")
                        .color(ui.theme().tokens().text_muted)
                        .font(ui.theme().font(TextRole::Label)),
                );
                rumble_widgets::Slider::new(&mut volume, 0.0..=1.0).show(ui);
                if (volume - before).abs() > 0.001 {
                    store.modify(|s| s.sfx.volume = volume.clamp(0.0, 1.0));
                }
            });
        });
}

fn devices_page<P: Platform + 'static>(ui: &mut Ui, state: &State, backend: &BackendHandle<P>) {
    ui.label(
        RichText::new("Audio devices")
            .font(ui.theme().font(TextRole::Heading))
            .strong(),
    );
    ui.add_space(6.0);

    device_picker(
        ui,
        "Input (microphone)",
        "input_device",
        &state.audio.input_devices,
        state.audio.selected_input.as_deref(),
        |new_id| backend.send(Command::SetInputDevice { device_id: new_id }),
    );
    ui.add_space(8.0);
    device_picker(
        ui,
        "Output (speakers)",
        "output_device",
        &state.audio.output_devices,
        state.audio.selected_output.as_deref(),
        |new_id| backend.send(Command::SetOutputDevice { device_id: new_id }),
    );

    ui.add_space(10.0);
    if ButtonArgs::new("Refresh device list")
        .role(PressableRole::Default)
        .show(ui)
        .clicked()
    {
        backend.send(Command::RefreshAudioDevices);
    }
}

fn device_picker(
    ui: &mut Ui,
    title: &str,
    id_salt: &str,
    devices: &[rumble_protocol::AudioDeviceInfo],
    current_id: Option<&str>,
    on_change: impl FnOnce(Option<String>),
) {
    GroupBox::new(title)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            if devices.is_empty() {
                ui.label(
                    RichText::new("No devices detected. Try Refresh below.").color(ui.theme().tokens().text_muted),
                );
                return;
            }
            let mut labels: Vec<String> = Vec::with_capacity(devices.len() + 1);
            labels.push("System default".to_string());
            for d in devices {
                let suffix = if d.is_default { " (default)" } else { "" };
                labels.push(format!("{}{}", d.name, suffix));
            }
            let mut selected_idx = match current_id {
                None => 0,
                Some(id) => devices.iter().position(|d| d.id == id).map(|i| i + 1).unwrap_or(0),
            };
            let before = selected_idx;
            ComboBox::new(id_salt, &mut selected_idx, labels).width(360.0).show(ui);
            if selected_idx != before {
                let new_id = if selected_idx == 0 {
                    None
                } else {
                    devices.get(selected_idx - 1).map(|d| d.id.clone())
                };
                on_change(new_id);
            }
        });
}

/// TX-pipeline editor. Renders the live `state.audio.tx_pipeline`,
/// dispatches `Command::UpdateTxPipeline` whenever the user changes a
/// processor's enabled flag or a schema-driven parameter. The next
/// frame's snapshot reflects the change, so the UI stays in lock-step
/// with what the audio task is actually running — no "pending" buffer.
fn processing_page<P: Platform + 'static>(
    ui: &mut Ui,
    state: &State,
    backend: &BackendHandle<P>,
    registry: &ProcessorRegistry,
) {
    ui.label(
        RichText::new("Audio processing")
            .font(ui.theme().font(TextRole::Heading))
            .strong(),
    );
    ui.add_space(4.0);
    ui.label(
        RichText::new("Pipeline applied before encoding. Order matters: each stage feeds the next.")
            .color(ui.theme().tokens().text_muted)
            .font(ui.theme().font(TextRole::Label)),
    );
    ui.add_space(8.0);

    // Edit a clone — if anything changed we send the whole pipeline
    // back to the audio task. Cloning a config with three processors
    // is trivial; not worth a per-field diff.
    let original = state.audio.tx_pipeline.clone();
    let mut working = original.clone();
    let mut dirty = false;

    if working.processors.is_empty() {
        ui.label(
            RichText::new("No processors configured. Default pipeline ships with denoise, VAD, and gain.")
                .color(ui.theme().tokens().text_muted),
        );
    }

    let info: std::collections::HashMap<String, (String, String)> = registry
        .list_available()
        .into_iter()
        .map(|(id, name, desc)| (id.to_string(), (name.to_string(), desc.to_string())))
        .collect();

    for (i, proc) in working.processors.iter_mut().enumerate() {
        let (display_name, description) = info
            .get(&proc.type_id)
            .cloned()
            .unwrap_or_else(|| (proc.type_id.clone(), "Unknown processor".into()));

        let _ = i;
        GroupBox::new(display_name.clone())
            .inner_margin(Margin::symmetric(12, 10))
            .show(ui, |ui| {
                let mut enabled = proc.enabled;
                if ui
                    .checkbox(&mut enabled, "Enabled")
                    .on_hover_text(description)
                    .changed()
                {
                    proc.enabled = enabled;
                    dirty = true;
                }

                if !enabled {
                    return;
                }

                let Some(schema) = registry.settings_schema(&proc.type_id) else {
                    ui.label(
                        RichText::new("(no settings)")
                            .color(ui.theme().tokens().text_muted)
                            .font(ui.theme().font(TextRole::Label)),
                    );
                    return;
                };
                let Some(properties) = schema.get("properties").and_then(|p| p.as_object()) else {
                    return;
                };
                if properties.is_empty() {
                    return;
                }
                ui.add_space(4.0);
                for (key, prop_schema) in properties {
                    if render_schema_field(ui, key, prop_schema, &mut proc.settings) {
                        dirty = true;
                    }
                }
            });
        ui.add_space(6.0);
    }

    if dirty {
        backend.send(Command::UpdateTxPipeline { config: working });
    }

    ui.add_space(8.0);
    GroupBox::new("Input level")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            input_level_meter(ui, state, &original);
        });
}

/// Render one JSON-schema property as a settings field. Mirrors the
/// dispatch in `rumble-egui::render_schema_field` — same `type` →
/// widget mapping (number → Slider, integer → Slider, boolean →
/// checkbox, string → text input), so behaviour is consistent across
/// clients.
fn render_schema_field(
    ui: &mut Ui,
    key: &str,
    prop_schema: &serde_json::Value,
    settings: &mut serde_json::Value,
) -> bool {
    let title = prop_schema.get("title").and_then(|t| t.as_str()).unwrap_or(key);
    let description = prop_schema.get("description").and_then(|d| d.as_str()).unwrap_or("");
    let prop_type = prop_schema.get("type").and_then(|t| t.as_str()).unwrap_or("string");

    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(format!("{title}:"));
        match prop_type {
            "number" => {
                let default = prop_schema.get("default").and_then(|d| d.as_f64()).unwrap_or(0.0) as f32;
                let min = prop_schema.get("minimum").and_then(|m| m.as_f64()).unwrap_or(-100.0) as f32;
                let max = prop_schema.get("maximum").and_then(|m| m.as_f64()).unwrap_or(100.0) as f32;
                let mut value = settings
                    .get(key)
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(default);
                let resp = ui
                    .add(egui::Slider::new(&mut value, min..=max))
                    .on_hover_text(description);
                if resp.changed() {
                    settings[key] = serde_json::json!(value);
                    changed = true;
                }
            }
            "integer" => {
                let default = prop_schema.get("default").and_then(|d| d.as_i64()).unwrap_or(0) as i32;
                let min = prop_schema.get("minimum").and_then(|m| m.as_i64()).unwrap_or(0) as i32;
                let max = prop_schema.get("maximum").and_then(|m| m.as_i64()).unwrap_or(1000) as i32;
                let mut value = settings
                    .get(key)
                    .and_then(|v| v.as_i64())
                    .map(|v| v as i32)
                    .unwrap_or(default);
                let resp = ui
                    .add(egui::Slider::new(&mut value, min..=max))
                    .on_hover_text(description);
                if resp.changed() {
                    settings[key] = serde_json::json!(value);
                    changed = true;
                }
            }
            "boolean" => {
                let default = prop_schema.get("default").and_then(|d| d.as_bool()).unwrap_or(false);
                let mut value = settings.get(key).and_then(|v| v.as_bool()).unwrap_or(default);
                if ui.checkbox(&mut value, "").on_hover_text(description).changed() {
                    settings[key] = serde_json::json!(value);
                    changed = true;
                }
            }
            _ => {
                let default = prop_schema.get("default").and_then(|d| d.as_str()).unwrap_or("");
                let mut value = settings
                    .get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or(default)
                    .to_string();
                if ui.text_edit_singleline(&mut value).on_hover_text(description).changed() {
                    settings[key] = serde_json::json!(value);
                    changed = true;
                }
            }
        }
    });
    changed
}

/// Input-level bar with a vertical line at the current VAD threshold.
/// Helps the user calibrate VAD: the line sits where the gate opens,
/// the coloured bar shows what the mic is picking up right now.
fn input_level_meter(ui: &mut Ui, state: &State, pipeline: &PipelineConfig) {
    let level_db = state.audio.input_level_db;
    let vad_threshold = pipeline
        .processors
        .iter()
        .find(|p| p.type_id == "builtin.vad" && p.enabled)
        .and_then(|p| p.settings.get("threshold_db"))
        .and_then(|v| v.as_f64())
        .map(|t| t as f32);

    let Some(level_db) = level_db else {
        ui.label(RichText::new("— (no input)").color(ui.theme().tokens().text_muted));
        return;
    };

    ui.horizontal(|ui| {
        let normalized = ((level_db + 60.0) / 60.0).clamp(0.0, 1.0);
        let color = if level_db > -3.0 {
            egui::Color32::from_rgb(0xF4, 0x43, 0x36)
        } else if level_db > -12.0 {
            egui::Color32::from_rgb(0xFF, 0x98, 0x00)
        } else {
            egui::Color32::from_rgb(0x4C, 0xAF, 0x50)
        };
        let (rect, _) = ui.allocate_exact_size(egui::vec2(220.0, 16.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 2.0, egui::Color32::DARK_GRAY);
        let filled = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * normalized, rect.height()));
        ui.painter().rect_filled(filled, 2.0, color);
        if let Some(threshold) = vad_threshold {
            let n = ((threshold + 60.0) / 60.0).clamp(0.0, 1.0);
            let x = rect.min.x + rect.width() * n;
            ui.painter().line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                egui::Stroke::new(2.0, egui::Color32::WHITE),
            );
        }
        ui.label(format!("{level_db:.1} dB"));
    });
}

fn chat_page(ui: &mut Ui, store: &mut SettingsStore) {
    ui.label(RichText::new("Chat").font(ui.theme().font(TextRole::Heading)).strong());
    ui.add_space(6.0);

    GroupBox::new("Timestamps")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let mut show = store.settings().chat.show_timestamps;
            if ui.checkbox(&mut show, "Show timestamps next to messages").changed() {
                store.modify(|s| s.chat.show_timestamps = show);
            }

            ui.add_space(4.0);
            ui.add_enabled_ui(show, |ui| {
                let current = store.settings().chat.timestamp_format;
                let labels: Vec<String> = TimestampFormat::ALL.iter().map(|f| f.label().to_string()).collect();
                let mut idx = TimestampFormat::ALL.iter().position(|f| *f == current).unwrap_or(0);
                let before = idx;
                ComboBox::new("chat_timestamp_format", &mut idx, labels)
                    .width(280.0)
                    .show(ui);
                if idx != before
                    && let Some(&fmt) = TimestampFormat::ALL.get(idx)
                {
                    store.modify(|s| s.chat.timestamp_format = fmt);
                }
            });
        });

    ui.add_space(8.0);
    GroupBox::new("History")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let mut auto = store.settings().chat.auto_sync_history;
            if ui
                .checkbox(&mut auto, "Request peer chat history when joining a room")
                .changed()
            {
                store.modify(|s| s.chat.auto_sync_history = auto);
            }
            ui.label(
                RichText::new("Use the ⟳ sync button next to the composer for one-shot history requests.")
                    .color(ui.theme().tokens().text_muted)
                    .font(ui.theme().font(TextRole::Label)),
            );
        });
}

fn statistics_page(ui: &mut Ui, state: &State) {
    ui.label(
        RichText::new("Statistics")
            .font(ui.theme().font(TextRole::Heading))
            .strong(),
    );
    ui.add_space(6.0);

    let audio = &state.audio;
    GroupBox::new("Audio")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            stat_row(
                ui,
                "Input level (dB)",
                &audio
                    .input_level_db
                    .map(|v| format!("{v:.1}"))
                    .unwrap_or_else(|| "—".into()),
            );
            stat_row(ui, "Transmitting", &audio.is_transmitting.to_string());
            stat_row(ui, "Self muted", &audio.self_muted.to_string());
            stat_row(ui, "Self deafened", &audio.self_deafened.to_string());
        });

    ui.add_space(8.0);
    GroupBox::new("Connection")
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let summary = match &state.connection {
                ConnectionState::Connected { server_name, .. } => format!("connected · {server_name}"),
                ConnectionState::Connecting { server_addr } => format!("connecting to {server_addr}"),
                ConnectionState::Disconnected => "disconnected".to_string(),
                ConnectionState::ConnectionLost { error } => format!("lost: {error}"),
                ConnectionState::CertificatePending { .. } => "awaiting certificate approval".to_string(),
            };
            stat_row(ui, "State", &summary);
            stat_row(
                ui,
                "Users in room",
                &crate::adapters::peers_in_current_room(state).to_string(),
            );
        });
}

fn stat_row(ui: &mut Ui, label: &str, value: &str) {
    ui.horizontal(|ui| {
        let tokens = ui.theme().tokens().clone();
        ui.label(
            RichText::new(label)
                .color(tokens.text_muted)
                .font(tokens.font_label.clone()),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(value).font(tokens.font_mono.clone()));
        });
    });
}

fn about_page(ui: &mut Ui, public_key_hex: &str) {
    ui.label(RichText::new("About").font(ui.theme().font(TextRole::Heading)).strong());
    ui.add_space(6.0);

    SurfaceFrame::new(SurfaceKind::Panel)
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            let tokens = ui.theme().tokens().clone();
            ui.label(RichText::new("rumble-next").color(tokens.text).strong());
            ui.label(
                RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                    .color(tokens.text_muted)
                    .font(tokens.font_mono.clone()),
            );
            ui.add_space(8.0);
            ui.label(
                RichText::new("Your public key")
                    .color(tokens.text_muted)
                    .font(tokens.font_label.clone()),
            );
            ui.label(
                RichText::new(public_key_hex)
                    .color(tokens.text)
                    .font(tokens.font_mono.clone()),
            );
        });
}
