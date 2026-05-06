//! Settings dialog — tab-based configuration UI for the aetna client.
//!
//! Owns its own ephemeral UI state (selected tab, pending values for
//! each section, which dropdown is open). The App routes events here
//! via [`handle_event`] and dispatches the resulting [`SettingsOutcome`]
//! back to the backend / `SettingsStore` / identity wizard.
//!
//! Save semantics mirror the rumble-egui dialog: most edits accumulate
//! in `pending_*` fields and only land when the user clicks Save. A few
//! controls (Refresh devices, Reset stats, Preview sfx, Regenerate
//! identity) fire immediately because they're side-effecting actions
//! rather than persisted state.

use aetna_core::prelude::*;

use rumble_client::SfxKind;
use rumble_desktop_shell::{PersistentVoiceMode, Settings, TimestampFormat};
use rumble_protocol::{AudioSettings, AudioState, State};

use crate::identity::Identity;

// ============================================================
// State
// ============================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Connection,
    Devices,
    Voice,
    Sounds,
    Chat,
    Files,
    Stats,
}

impl SettingsTab {
    pub const ALL: &'static [SettingsTab] = &[
        SettingsTab::Connection,
        SettingsTab::Devices,
        SettingsTab::Voice,
        SettingsTab::Sounds,
        SettingsTab::Chat,
        SettingsTab::Files,
        SettingsTab::Stats,
    ];

    fn slug(self) -> &'static str {
        match self {
            SettingsTab::Connection => "connection",
            SettingsTab::Devices => "devices",
            SettingsTab::Voice => "voice",
            SettingsTab::Sounds => "sounds",
            SettingsTab::Chat => "chat",
            SettingsTab::Files => "files",
            SettingsTab::Stats => "stats",
        }
    }

    fn label(self) -> &'static str {
        match self {
            SettingsTab::Connection => "Connection",
            SettingsTab::Devices => "Devices",
            SettingsTab::Voice => "Voice",
            SettingsTab::Sounds => "Sounds",
            SettingsTab::Chat => "Chat",
            SettingsTab::Files => "Files",
            SettingsTab::Stats => "Stats",
        }
    }

    fn from_slug(s: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|t| t.slug() == s)
    }
}

/// At most one select dropdown is open at a time. Tracking it here
/// avoids one bool per select and gives `handle_event` a single place
/// to clear it when the user opens a different one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenSelect {
    None,
    InputDevice,
    OutputDevice,
    TimestampFormat,
}

impl Default for OpenSelect {
    fn default() -> Self {
        OpenSelect::None
    }
}

/// Pending edits accumulated while the dialog is open. Initialised
/// from the live values in [`SettingsState::open_with`] and read back
/// by [`SettingsOutcome::Save`].
#[derive(Debug, Clone)]
pub struct PendingSettings {
    // Connection
    pub username: String,
    pub username_sel: TextSelection,
    pub autoconnect: bool,

    // Devices — `None` means "system default"; the outer Option tracks
    // whether the user touched the field at all.
    pub input_device: Option<String>,
    pub output_device: Option<String>,

    // Voice
    pub voice_mode: PersistentVoiceMode,
    pub audio: AudioSettings,

    // Sounds
    pub sfx_enabled: bool,
    pub sfx_volume: f32,
    /// Indexed by `SfxKind::all()` order.
    pub sfx_kind_enabled: Vec<bool>,

    // Chat
    pub show_timestamps: bool,
    pub timestamp_format: TimestampFormat,
    pub auto_sync_history: bool,

    // Files
    pub auto_download_enabled: bool,
    pub download_speed_kbps: u32,
    pub upload_speed_kbps: u32,
    pub seed_after_download: bool,
    pub cleanup_on_exit: bool,
}

impl PendingSettings {
    fn from_live(audio: &AudioState, settings: &Settings, username: &str) -> Self {
        let sfx_kind_enabled: Vec<bool> = SfxKind::all()
            .iter()
            .map(|k| !settings.sfx.disabled_sounds.contains(k))
            .collect();
        Self {
            username: username.to_string(),
            username_sel: TextSelection::default(),
            autoconnect: settings.auto_connect_addr.is_some(),
            input_device: audio.selected_input.clone(),
            output_device: audio.selected_output.clone(),
            voice_mode: (&audio.voice_mode).into(),
            audio: audio.settings.clone(),
            sfx_enabled: settings.sfx.enabled,
            sfx_volume: settings.sfx.volume,
            sfx_kind_enabled,
            show_timestamps: settings.chat.show_timestamps,
            timestamp_format: settings.chat.timestamp_format,
            auto_sync_history: settings.chat.auto_sync_history,
            auto_download_enabled: settings.file_transfer.auto_download_enabled,
            download_speed_kbps: (settings.file_transfer.download_speed_limit / 1024) as u32,
            upload_speed_kbps: (settings.file_transfer.upload_speed_limit / 1024) as u32,
            seed_after_download: settings.file_transfer.seed_after_download,
            cleanup_on_exit: settings.file_transfer.cleanup_on_exit,
        }
    }
}

#[derive(Debug, Default)]
pub struct SettingsState {
    pub open: bool,
    pub tab: Option<SettingsTab>,
    pub open_select: OpenSelect,
    pub pending: Option<PendingSettings>,
}

impl SettingsState {
    /// Snapshot the live settings into pending state and show the
    /// dialog. Defaults the active tab to Connection.
    pub fn open_with(&mut self, audio: &AudioState, settings: &Settings, username: &str) {
        self.open = true;
        self.tab = Some(SettingsTab::Connection);
        self.open_select = OpenSelect::None;
        self.pending = Some(PendingSettings::from_live(audio, settings, username));
    }

    pub fn close(&mut self) {
        self.open = false;
        self.tab = None;
        self.open_select = OpenSelect::None;
        self.pending = None;
    }
}

/// Outcome of routing a single event into the settings dialog.
pub enum SettingsOutcome {
    Ignored,
    Handled,
    /// Close the dialog without saving.
    Close,
    /// Apply pending fields and close. The App reads the carried
    /// `PendingSettings` and writes it through to the backend +
    /// `SettingsStore`.
    Save(PendingSettings),
    /// User clicked "Generate new identity"; close settings and open
    /// the identity wizard.
    OpenIdentityWizard,
    /// One-shot side effects.
    PreviewSfx {
        kind: SfxKind,
        volume: f32,
    },
    RefreshDevices,
    ResetStats,
}

// ============================================================
// Routed-key constants
// ============================================================

const KEY_TABS: &str = "settings:tabs";
const KEY_DISMISS: &str = "settings:dismiss";
const KEY_CLOSE: &str = "settings:close";
const KEY_SAVE: &str = "settings:save";

const KEY_USERNAME: &str = "settings:conn:username";
const KEY_AUTOCONNECT: &str = "settings:conn:autoconnect";
const KEY_REGENERATE: &str = "settings:conn:regenerate";

const KEY_INPUT_DEVICE: &str = "settings:dev:input";
const KEY_OUTPUT_DEVICE: &str = "settings:dev:output";
const KEY_REFRESH_DEVICES: &str = "settings:dev:refresh";

const KEY_VOICE_MODE_TABS: &str = "settings:voice:mode";
const KEY_BITRATE_TABS: &str = "settings:voice:bitrate";
const KEY_VOICE_FEC: &str = "settings:voice:fec";
const KEY_VOICE_COMPLEXITY: &str = "settings:voice:complexity";
const KEY_VOICE_JITTER: &str = "settings:voice:jitter";
const KEY_VOICE_PACKET_LOSS: &str = "settings:voice:packet-loss";

const KEY_SFX_ENABLED: &str = "settings:sfx:enabled";
const KEY_SFX_VOLUME: &str = "settings:sfx:volume";

const KEY_CHAT_SHOW_TIMESTAMPS: &str = "settings:chat:show-timestamps";
const KEY_CHAT_FORMAT: &str = "settings:chat:format";
const KEY_CHAT_AUTO_SYNC: &str = "settings:chat:auto-sync";

const KEY_FILES_AUTO_DOWNLOAD: &str = "settings:files:auto-download";
const KEY_FILES_DL_LIMIT: &str = "settings:files:download-limit";
const KEY_FILES_UL_LIMIT: &str = "settings:files:upload-limit";
const KEY_FILES_SEED: &str = "settings:files:seed";
const KEY_FILES_CLEANUP: &str = "settings:files:cleanup";

const KEY_STATS_RESET: &str = "settings:stats:reset";

/// Limits clamp KB/s sliders. 5000 KB/s ≈ 5 MB/s — well above any
/// realistic rumble file-transfer cap.
const MAX_SPEED_KBPS: u32 = 5000;

fn sfx_kind_key(idx: usize) -> String {
    format!("settings:sfx:kind:{idx}")
}
fn sfx_preview_key(idx: usize) -> String {
    format!("settings:sfx:preview:{idx}")
}

// ============================================================
// Render
// ============================================================

/// Render the settings dialog and any open dropdown popover. Returns
/// `(panel_layer, popover_layer)` so the App can compose them as
/// independent overlay layers — popovers must paint above the modal
/// panel they were anchored to. Returns `None` for the panel when the
/// dialog is closed.
pub fn render(state: &SettingsState, app_state: &State, identity: &Identity) -> (Option<El>, Option<El>) {
    if !state.open {
        return (None, None);
    }
    let pending = match &state.pending {
        Some(p) => p,
        None => return (None, None),
    };
    let tab = state.tab.unwrap_or(SettingsTab::Connection);

    let body = match tab {
        SettingsTab::Connection => render_connection(pending, identity),
        SettingsTab::Devices => render_devices(pending, &app_state.audio),
        SettingsTab::Voice => render_voice(pending),
        SettingsTab::Sounds => render_sounds(pending),
        SettingsTab::Chat => render_chat(pending),
        SettingsTab::Files => render_files(pending),
        SettingsTab::Stats => render_stats(&app_state.audio),
    };

    let tabs_row = tabs_list(
        KEY_TABS,
        &tab.slug(),
        SettingsTab::ALL.iter().map(|t| (t.slug(), t.label())),
    );

    let footer = row([
        button("Close").key(KEY_CLOSE),
        spacer(),
        button("Save").key(KEY_SAVE).primary(),
    ])
    .gap(tokens::SPACE_SM)
    .width(Size::Fill(1.0))
    .align(Align::Center);

    let panel = panel_with_size(
        "Settings",
        720.0,
        620.0,
        [
            tabs_row,
            scroll([body])
                .padding(Sides::xy(0.0, tokens::SPACE_SM))
                .gap(tokens::SPACE_MD)
                .width(Size::Fill(1.0))
                .height(Size::Fill(1.0)),
            divider(),
            footer,
        ],
    );

    let panel_layer = overlay([scrim(KEY_DISMISS), panel.block_pointer()]);

    let popover_layer = match state.open_select {
        OpenSelect::None => None,
        OpenSelect::InputDevice => Some(select_menu(
            KEY_INPUT_DEVICE,
            std::iter::once(("__default".to_string(), "System default".to_string())).chain(
                app_state
                    .audio
                    .input_devices
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone())),
            ),
        )),
        OpenSelect::OutputDevice => Some(select_menu(
            KEY_OUTPUT_DEVICE,
            std::iter::once(("__default".to_string(), "System default".to_string())).chain(
                app_state
                    .audio
                    .output_devices
                    .iter()
                    .map(|d| (d.id.clone(), d.name.clone())),
            ),
        )),
        OpenSelect::TimestampFormat => Some(select_menu(
            KEY_CHAT_FORMAT,
            TimestampFormat::ALL
                .iter()
                .enumerate()
                .map(|(idx, fmt)| (idx.to_string(), fmt.label().to_string())),
        )),
    };

    (Some(panel_layer), popover_layer)
}

/// A wider variant of [`modal_panel`]. The stock panel is fixed at
/// 420 px which is too narrow for a tabbed settings dialog with two
/// columns of form rows; we duplicate the styling here and pass our
/// own width/height.
fn panel_with_size<I, E>(title: impl Into<String>, w: f32, h: f32, body: I) -> El
where
    I: IntoIterator<Item = E>,
    E: Into<El>,
{
    let mut children: Vec<El> = vec![h3(title)];
    children.extend(body.into_iter().map(Into::into));
    El::new(Kind::Modal)
        .style_profile(StyleProfile::Surface)
        .surface_role(SurfaceRole::Popover)
        .children(children)
        .fill(tokens::BG_CARD)
        .stroke(tokens::BORDER)
        .radius(tokens::RADIUS_LG)
        .shadow(tokens::SHADOW_LG)
        .padding(tokens::SPACE_LG)
        .gap(tokens::SPACE_MD)
        .width(Size::Fixed(w))
        .height(Size::Fixed(h))
        .axis(Axis::Column)
        .align(Align::Stretch)
        .clip()
}

// ---- per-tab views --------------------------------------------------

fn render_connection(pending: &PendingSettings, identity: &Identity) -> El {
    use rumble_desktop_shell::KeySource;

    let identity_lines: Vec<El> = if let Some(config) = identity.manager().config() {
        let storage = match &config.source {
            KeySource::LocalPlaintext { .. } => "Local (unencrypted)",
            KeySource::LocalEncrypted { .. } => "Local (password protected)",
            KeySource::SshAgent { .. } => "SSH agent",
        };
        vec![
            field_row("Storage", text(storage.to_string()).font_weight(FontWeight::Semibold)),
            field_row(
                "Fingerprint",
                mono(identity.fingerprint())
                    .font_size(tokens::FONT_SM)
                    .ellipsis()
                    .width(Size::Fill(1.0)),
            ),
        ]
    } else {
        vec![paragraph("No identity configured.").text_color(tokens::WARNING)]
    };

    let mut children: Vec<El> = Vec::new();
    children.push(section_heading("Identity"));
    children.extend(identity_lines);
    children.push(
        row([
            spacer(),
            button("Generate new identity…").key(KEY_REGENERATE).destructive(),
        ])
        .width(Size::Fill(1.0)),
    );
    children.push(divider());
    children.push(section_heading("Username"));
    children.push(text_input(&pending.username, pending.username_sel).key(KEY_USERNAME));
    children.push(
        paragraph("Display name shown to other users on a server.")
            .muted()
            .font_size(tokens::FONT_SM),
    );
    children.push(divider());
    children.push(field_row(
        "Autoconnect on launch",
        switch(pending.autoconnect).key(KEY_AUTOCONNECT),
    ));
    children.push(
        paragraph(
            "Reconnects to the most recently used server on startup. Effective once you've connected to at least one \
             server.",
        )
        .muted()
        .font_size(tokens::FONT_SM),
    );

    column(children).gap(tokens::SPACE_MD).width(Size::Fill(1.0))
}

fn render_devices(pending: &PendingSettings, audio: &AudioState) -> El {
    let input_label = device_label_for(pending.input_device.as_deref(), &audio.input_devices);
    let output_label = device_label_for(pending.output_device.as_deref(), &audio.output_devices);

    column([
        section_heading("Input device"),
        select_trigger(KEY_INPUT_DEVICE, input_label),
        spacer().height(Size::Fixed(tokens::SPACE_XS)),
        section_heading("Output device"),
        select_trigger(KEY_OUTPUT_DEVICE, output_label),
        spacer().height(Size::Fixed(tokens::SPACE_SM)),
        row([button("Refresh devices").key(KEY_REFRESH_DEVICES).secondary(), spacer()]).width(Size::Fill(1.0)),
        divider(),
        paragraph(
            "Device changes apply when you click Save. Switching devices while connected may briefly drop audio.",
        )
        .muted()
        .font_size(tokens::FONT_SM),
    ])
    .gap(tokens::SPACE_SM)
    .width(Size::Fill(1.0))
}

fn render_voice(pending: &PendingSettings) -> El {
    let mode_slug = match pending.voice_mode {
        PersistentVoiceMode::PushToTalk => "ptt",
        PersistentVoiceMode::Continuous => "cont",
    };
    let bitrate_slug = bitrate_slug(pending.audio.bitrate);

    column([
        section_heading("Voice mode"),
        tabs_list(
            KEY_VOICE_MODE_TABS,
            &mode_slug,
            [("ptt", "Push-to-Talk"), ("cont", "Continuous")],
        ),
        paragraph(match pending.voice_mode {
            PersistentVoiceMode::PushToTalk => "Hold the configured PTT key (default: Space) to transmit.",
            PersistentVoiceMode::Continuous => "Always transmitting. Add a VAD processor to gate on voice activity.",
        })
        .muted()
        .font_size(tokens::FONT_SM),
        divider(),
        section_heading("Encoder"),
        field_row(
            "Bitrate",
            tabs_list(
                KEY_BITRATE_TABS,
                &bitrate_slug,
                [
                    ("low", "24 kbps"),
                    ("medium", "32 kbps"),
                    ("high", "64 kbps"),
                    ("very-high", "96 kbps"),
                ],
            )
            .width(Size::Fixed(360.0)),
        ),
        field_row(
            format!("Complexity ({})", pending.audio.encoder_complexity),
            slider(pending.audio.encoder_complexity as f32 / 10.0, tokens::PRIMARY)
                .key(KEY_VOICE_COMPLEXITY)
                .width(Size::Fixed(280.0)),
        ),
        paragraph("Higher complexity = better quality, more CPU. Range 0–10.")
            .muted()
            .font_size(tokens::FONT_SM),
        divider(),
        section_heading("Network"),
        field_row(
            "Forward Error Correction",
            switch(pending.audio.fec_enabled).key(KEY_VOICE_FEC),
        ),
        field_row(
            format!(
                "Jitter buffer ({} packets · ~{}ms)",
                pending.audio.jitter_buffer_delay_packets,
                pending.audio.jitter_buffer_delay_packets * 20
            ),
            slider(
                (pending.audio.jitter_buffer_delay_packets as f32 - 1.0) / 9.0,
                tokens::PRIMARY,
            )
            .key(KEY_VOICE_JITTER)
            .width(Size::Fixed(280.0)),
        ),
        field_row(
            format!("Expected packet loss ({}%)", pending.audio.packet_loss_percent),
            slider(pending.audio.packet_loss_percent as f32 / 25.0, tokens::PRIMARY)
                .key(KEY_VOICE_PACKET_LOSS)
                .width(Size::Fixed(280.0)),
        ),
    ])
    .gap(tokens::SPACE_SM)
    .width(Size::Fill(1.0))
}

fn render_sounds(pending: &PendingSettings) -> El {
    let mut rows: Vec<El> = Vec::new();
    rows.push(field_row(
        "Enable sound effects",
        switch(pending.sfx_enabled).key(KEY_SFX_ENABLED),
    ));
    rows.push(field_row(
        format!("Volume ({}%)", (pending.sfx_volume * 100.0).round() as i32),
        slider(pending.sfx_volume, tokens::PRIMARY)
            .key(KEY_SFX_VOLUME)
            .width(Size::Fixed(280.0)),
    ));
    rows.push(divider());
    rows.push(section_heading("Individual sounds"));
    for (idx, kind) in SfxKind::all().iter().enumerate() {
        let enabled = pending.sfx_kind_enabled.get(idx).copied().unwrap_or(true);
        rows.push(
            row([
                text(kind.label().to_string()).label(),
                spacer(),
                button("Preview").key(sfx_preview_key(idx)).secondary(),
                switch(enabled).key(sfx_kind_key(idx)),
            ])
            .gap(tokens::SPACE_SM)
            .align(Align::Center)
            .width(Size::Fill(1.0)),
        );
    }
    column(rows).gap(tokens::SPACE_SM).width(Size::Fill(1.0))
}

fn render_chat(pending: &PendingSettings) -> El {
    column([
        field_row(
            "Show timestamps",
            switch(pending.show_timestamps).key(KEY_CHAT_SHOW_TIMESTAMPS),
        ),
        field_row(
            "Timestamp format",
            select_trigger(KEY_CHAT_FORMAT, pending.timestamp_format.label()).width(Size::Fixed(280.0)),
        ),
        divider(),
        field_row(
            "Auto-sync history on join",
            switch(pending.auto_sync_history).key(KEY_CHAT_AUTO_SYNC),
        ),
        paragraph("Asks peers for backlog when joining a room so you can read what was said before you arrived.")
            .muted()
            .font_size(tokens::FONT_SM),
    ])
    .gap(tokens::SPACE_SM)
    .width(Size::Fill(1.0))
}

fn render_files(pending: &PendingSettings) -> El {
    column([
        section_heading("Auto-download"),
        field_row(
            "Enable auto-download",
            switch(pending.auto_download_enabled).key(KEY_FILES_AUTO_DOWNLOAD),
        ),
        paragraph(
            "Auto-download rules (per-MIME size limits) are not yet editable in the aetna client. Use rumble-egui to \
             edit the rule list; this client honours whatever rules are stored.",
        )
        .muted()
        .font_size(tokens::FONT_SM),
        divider(),
        section_heading("Bandwidth limits"),
        field_row(
            format!("Download limit ({})", speed_label(pending.download_speed_kbps)),
            slider(
                pending.download_speed_kbps as f32 / MAX_SPEED_KBPS as f32,
                tokens::PRIMARY,
            )
            .key(KEY_FILES_DL_LIMIT)
            .width(Size::Fixed(280.0)),
        ),
        field_row(
            format!("Upload limit ({})", speed_label(pending.upload_speed_kbps)),
            slider(
                pending.upload_speed_kbps as f32 / MAX_SPEED_KBPS as f32,
                tokens::PRIMARY,
            )
            .key(KEY_FILES_UL_LIMIT)
            .width(Size::Fixed(280.0)),
        ),
        divider(),
        section_heading("Seeding"),
        field_row(
            "Continue seeding after download",
            switch(pending.seed_after_download).key(KEY_FILES_SEED),
        ),
        field_row(
            "Clean up downloads on exit",
            switch(pending.cleanup_on_exit).key(KEY_FILES_CLEANUP),
        ),
    ])
    .gap(tokens::SPACE_SM)
    .width(Size::Fill(1.0))
}

fn render_stats(audio: &AudioState) -> El {
    let stats = &audio.stats;
    let loss_pct = stats.packet_loss_percent();
    let loss_color = if loss_pct > 5.0 {
        tokens::DESTRUCTIVE
    } else if loss_pct > 1.0 {
        tokens::WARNING
    } else {
        tokens::SUCCESS
    };

    column([
        stat_row(
            "Actual bitrate",
            format!("{:.1} kbps", stats.actual_bitrate_bps / 1000.0),
            None,
        ),
        stat_row(
            "Avg frame size",
            format!("{:.1} bytes", stats.avg_frame_size_bytes),
            None,
        ),
        stat_row("Packets sent", stats.packets_sent.to_string(), None),
        stat_row("Packets received", stats.packets_received.to_string(), None),
        stat_row(
            "Packet loss",
            format!("{:.1}% ({} lost)", loss_pct, stats.packets_lost),
            Some(loss_color),
        ),
        stat_row("FEC recovered", stats.packets_recovered_fec.to_string(), None),
        stat_row("Frames concealed", stats.frames_concealed.to_string(), None),
        stat_row(
            "Buffer level",
            format!("{} packets", stats.playback_buffer_packets),
            None,
        ),
        spacer().height(Size::Fixed(tokens::SPACE_SM)),
        row([spacer(), button("Reset statistics").key(KEY_STATS_RESET).secondary()]).width(Size::Fill(1.0)),
    ])
    .gap(tokens::SPACE_XS)
    .width(Size::Fill(1.0))
}

// ---- view helpers ---------------------------------------------------

fn section_heading(label: impl Into<String>) -> El {
    text(label).semibold().font_size(tokens::FONT_BASE)
}

/// `[label .... control]` — labelled form row used throughout the
/// dialog. Aetna doesn't ship a built-in form-row helper, so this
/// keeps the spacing/justification consistent in one place.
fn field_row(label: impl Into<String>, control: impl Into<El>) -> El {
    row([text(label).label(), spacer(), control.into()])
        .gap(tokens::SPACE_MD)
        .align(Align::Center)
        .width(Size::Fill(1.0))
}

fn stat_row(label: impl Into<String>, value: impl Into<String>, color: Option<Color>) -> El {
    let value_text = text(value).mono().font_size(tokens::FONT_SM);
    let value_text = if let Some(c) = color {
        value_text.text_color(c)
    } else {
        value_text
    };
    row([text(label).muted(), spacer(), value_text])
        .gap(tokens::SPACE_MD)
        .align(Align::Center)
        .width(Size::Fill(1.0))
}

fn device_label_for(selected: Option<&str>, devices: &[rumble_protocol::AudioDeviceInfo]) -> String {
    match selected {
        None => "System default".to_string(),
        Some(id) => devices
            .iter()
            .find(|d| d.id == id)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| format!("(missing) {id}")),
    }
}

fn speed_label(kbps: u32) -> String {
    if kbps == 0 {
        "unlimited".to_string()
    } else {
        format!("{kbps} KB/s")
    }
}

fn bitrate_slug(bitrate: i32) -> &'static str {
    match bitrate {
        AudioSettings::BITRATE_LOW => "low",
        AudioSettings::BITRATE_MEDIUM => "medium",
        AudioSettings::BITRATE_HIGH => "high",
        AudioSettings::BITRATE_VERY_HIGH => "very-high",
        _ => "high",
    }
}

fn parse_bitrate(slug: &str) -> Option<i32> {
    Some(match slug {
        "low" => AudioSettings::BITRATE_LOW,
        "medium" => AudioSettings::BITRATE_MEDIUM,
        "high" => AudioSettings::BITRATE_HIGH,
        "very-high" => AudioSettings::BITRATE_VERY_HIGH,
        _ => return None,
    })
}

// ============================================================
// Event handling
// ============================================================

pub fn handle_event(state: &mut SettingsState, event: &UiEvent) -> SettingsOutcome {
    if !state.open {
        return SettingsOutcome::Ignored;
    }

    // Esc unconditionally cancels.
    if matches!(event.kind, UiEventKind::Escape) {
        if state.open_select != OpenSelect::None {
            state.open_select = OpenSelect::None;
            return SettingsOutcome::Handled;
        }
        return SettingsOutcome::Close;
    }

    // Scrim click + Close button.
    if event.is_click_or_activate(KEY_DISMISS)
        || (event.is_route(KEY_DISMISS) && event.kind == UiEventKind::Click)
        || event.is_click_or_activate(KEY_CLOSE)
    {
        return SettingsOutcome::Close;
    }

    // Save: hand back the pending state for the App to apply.
    if event.is_click_or_activate(KEY_SAVE) {
        if let Some(pending) = state.pending.clone() {
            return SettingsOutcome::Save(pending);
        }
        return SettingsOutcome::Close;
    }

    // Tab switching.
    if let Some(tab) = state.tab.as_mut()
        && tabs::apply_event(tab, event, KEY_TABS, |s| SettingsTab::from_slug(s))
    {
        // Switching tabs implicitly closes any open dropdown so its
        // popover doesn't outlive the surface it was anchored to.
        state.open_select = OpenSelect::None;
        return SettingsOutcome::Handled;
    }

    // Identity regenerate is a one-shot side effect — close + open wizard.
    if event.is_click_or_activate(KEY_REGENERATE) {
        return SettingsOutcome::OpenIdentityWizard;
    }
    if event.is_click_or_activate(KEY_REFRESH_DEVICES) {
        return SettingsOutcome::RefreshDevices;
    }
    if event.is_click_or_activate(KEY_STATS_RESET) {
        return SettingsOutcome::ResetStats;
    }

    let pending = match state.pending.as_mut() {
        Some(p) => p,
        None => return SettingsOutcome::Ignored,
    };

    // ---------- per-tab pending edits ----------

    // Username text input.
    if event.target_key() == Some(KEY_USERNAME) {
        text_input::apply_event(&mut pending.username, &mut pending.username_sel, event);
        return SettingsOutcome::Handled;
    }

    // Switches.
    if switch::apply_event(&mut pending.autoconnect, event, KEY_AUTOCONNECT)
        || switch::apply_event(&mut pending.audio.fec_enabled, event, KEY_VOICE_FEC)
        || switch::apply_event(&mut pending.sfx_enabled, event, KEY_SFX_ENABLED)
        || switch::apply_event(&mut pending.show_timestamps, event, KEY_CHAT_SHOW_TIMESTAMPS)
        || switch::apply_event(&mut pending.auto_sync_history, event, KEY_CHAT_AUTO_SYNC)
        || switch::apply_event(&mut pending.auto_download_enabled, event, KEY_FILES_AUTO_DOWNLOAD)
        || switch::apply_event(&mut pending.seed_after_download, event, KEY_FILES_SEED)
        || switch::apply_event(&mut pending.cleanup_on_exit, event, KEY_FILES_CLEANUP)
    {
        return SettingsOutcome::Handled;
    }

    // Per-sfx switch + preview button.
    if matches!(event.kind, UiEventKind::Click | UiEventKind::Activate)
        && let Some(route) = event.route()
    {
        if let Some(idx) = route
            .strip_prefix("settings:sfx:kind:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(slot) = pending.sfx_kind_enabled.get_mut(idx)
        {
            *slot = !*slot;
            return SettingsOutcome::Handled;
        }
        if let Some(idx) = route
            .strip_prefix("settings:sfx:preview:")
            .and_then(|s| s.parse::<usize>().ok())
            && let Some(kind) = SfxKind::all().get(idx).copied()
        {
            return SettingsOutcome::PreviewSfx {
                kind,
                volume: pending.sfx_volume.max(0.3),
            };
        }
    }

    // Voice mode tabs.
    if tabs::apply_event(&mut pending.voice_mode, event, KEY_VOICE_MODE_TABS, |s| match s {
        "ptt" => Some(PersistentVoiceMode::PushToTalk),
        "cont" => Some(PersistentVoiceMode::Continuous),
        _ => None,
    }) {
        return SettingsOutcome::Handled;
    }

    // Bitrate tabs.
    if tabs::apply_event(&mut pending.audio.bitrate, event, KEY_BITRATE_TABS, parse_bitrate) {
        return SettingsOutcome::Handled;
    }

    // Sliders — pointer drag.
    if matches!(
        event.kind,
        UiEventKind::PointerDown | UiEventKind::Drag | UiEventKind::Click
    ) && let Some(route) = event.route()
        && let (Some(rect), Some(x)) = (event.target_rect(), event.pointer_x())
    {
        match route {
            KEY_VOICE_COMPLEXITY => {
                let n = slider::normalized_from_event(rect, x);
                pending.audio.encoder_complexity = (n * 10.0).round() as i32;
                return SettingsOutcome::Handled;
            }
            KEY_VOICE_JITTER => {
                let n = slider::normalized_from_event(rect, x);
                pending.audio.jitter_buffer_delay_packets = (1.0 + n * 9.0).round() as u32;
                return SettingsOutcome::Handled;
            }
            KEY_VOICE_PACKET_LOSS => {
                let n = slider::normalized_from_event(rect, x);
                pending.audio.packet_loss_percent = (n * 25.0).round() as i32;
                return SettingsOutcome::Handled;
            }
            KEY_SFX_VOLUME => {
                pending.sfx_volume = slider::normalized_from_event(rect, x);
                return SettingsOutcome::Handled;
            }
            KEY_FILES_DL_LIMIT => {
                let n = slider::normalized_from_event(rect, x);
                pending.download_speed_kbps = (n * MAX_SPEED_KBPS as f32).round() as u32;
                return SettingsOutcome::Handled;
            }
            KEY_FILES_UL_LIMIT => {
                let n = slider::normalized_from_event(rect, x);
                pending.upload_speed_kbps = (n * MAX_SPEED_KBPS as f32).round() as u32;
                return SettingsOutcome::Handled;
            }
            _ => {}
        }
    }

    // Sliders — keyboard arrows. Step granularity matches the pointer
    // resolution: complexity steps by 1 (=10%), packet-loss by 4%
    // (1/25), jitter by 1 packet (~11%), volume by 5%, speed by 5%.
    if let Some(route) = event.route() {
        let normalized = match route {
            KEY_VOICE_COMPLEXITY => Some(pending.audio.encoder_complexity as f32 / 10.0),
            KEY_VOICE_JITTER => Some((pending.audio.jitter_buffer_delay_packets as f32 - 1.0) / 9.0),
            KEY_VOICE_PACKET_LOSS => Some(pending.audio.packet_loss_percent as f32 / 25.0),
            KEY_SFX_VOLUME => Some(pending.sfx_volume),
            KEY_FILES_DL_LIMIT => Some(pending.download_speed_kbps as f32 / MAX_SPEED_KBPS as f32),
            KEY_FILES_UL_LIMIT => Some(pending.upload_speed_kbps as f32 / MAX_SPEED_KBPS as f32),
            _ => None,
        };
        if let Some(mut n) = normalized
            && slider::apply_event(&mut n, event, route, 0.05, 0.25)
        {
            n = n.clamp(0.0, 1.0);
            match route {
                KEY_VOICE_COMPLEXITY => pending.audio.encoder_complexity = (n * 10.0).round() as i32,
                KEY_VOICE_JITTER => pending.audio.jitter_buffer_delay_packets = (1.0 + n * 9.0).round() as u32,
                KEY_VOICE_PACKET_LOSS => pending.audio.packet_loss_percent = (n * 25.0).round() as i32,
                KEY_SFX_VOLUME => pending.sfx_volume = n,
                KEY_FILES_DL_LIMIT => pending.download_speed_kbps = (n * MAX_SPEED_KBPS as f32).round() as u32,
                KEY_FILES_UL_LIMIT => pending.upload_speed_kbps = (n * MAX_SPEED_KBPS as f32).round() as u32,
                _ => {}
            }
            return SettingsOutcome::Handled;
        }
    }

    // Selects (dropdowns). Each select fans into Toggle/Dismiss/Pick;
    // we route them by hand because the typed value the select picks
    // varies (Option<String> for devices, TimestampFormat for chat).
    if let Some(action) = select::classify_event(event, KEY_INPUT_DEVICE) {
        match action {
            select::SelectAction::Toggle => {
                state.open_select = if state.open_select == OpenSelect::InputDevice {
                    OpenSelect::None
                } else {
                    OpenSelect::InputDevice
                };
            }
            select::SelectAction::Dismiss => state.open_select = OpenSelect::None,
            select::SelectAction::Pick(value) => {
                pending.input_device = (value != "__default").then(|| value.to_string());
                state.open_select = OpenSelect::None;
            }
            // SelectAction is #[non_exhaustive] — fall through harmlessly
            // for any new variant aetna adds upstream.
            _ => {}
        }
        return SettingsOutcome::Handled;
    }
    if let Some(action) = select::classify_event(event, KEY_OUTPUT_DEVICE) {
        match action {
            select::SelectAction::Toggle => {
                state.open_select = if state.open_select == OpenSelect::OutputDevice {
                    OpenSelect::None
                } else {
                    OpenSelect::OutputDevice
                };
            }
            select::SelectAction::Dismiss => state.open_select = OpenSelect::None,
            select::SelectAction::Pick(value) => {
                pending.output_device = (value != "__default").then(|| value.to_string());
                state.open_select = OpenSelect::None;
            }
            _ => {}
        }
        return SettingsOutcome::Handled;
    }
    if let Some(action) = select::classify_event(event, KEY_CHAT_FORMAT) {
        match action {
            select::SelectAction::Toggle => {
                state.open_select = if state.open_select == OpenSelect::TimestampFormat {
                    OpenSelect::None
                } else {
                    OpenSelect::TimestampFormat
                };
            }
            select::SelectAction::Dismiss => state.open_select = OpenSelect::None,
            select::SelectAction::Pick(value) => {
                if let Ok(idx) = value.parse::<usize>()
                    && let Some(fmt) = TimestampFormat::ALL.get(idx).copied()
                {
                    pending.timestamp_format = fmt;
                }
                state.open_select = OpenSelect::None;
            }
            _ => {}
        }
        return SettingsOutcome::Handled;
    }

    SettingsOutcome::Ignored
}
