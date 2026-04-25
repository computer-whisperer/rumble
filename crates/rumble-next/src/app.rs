//! Top-level `eframe::App` — owns the backend handle, user identity,
//! paradigm choice, and connect form. Installs the right theme and
//! dispatches to either the connect view or the active paradigm.

use std::sync::Arc;

use eframe::egui::{self, CentralPanel, Context, Layout, RichText, TopBottomPanel};
use rumble_client::{ConnectConfig, ProcessorRegistry, handle::BackendHandle, register_builtin_processors};
use rumble_protocol::Command;
use rumble_widgets::{ButtonArgs, PressableRole, SurfaceFrame, SurfaceKind, UiExt, install_theme};

pub use crate::paradigm::Paradigm;
use rumble_desktop_shell::{
    SettingsStore, ToastManager,
    hotkeys::{HotkeyEvent, HotkeyManager},
};

use crate::{
    connect_view::{self, ConnectForm},
    identity::{Identity, default_config_dir},
    paradigm,
    settings_panel::{self, SettingsState},
    shell::Shell,
};

type NativeBackend = BackendHandle<rumble_desktop::NativePlatform>;

pub struct App {
    pub paradigm: Paradigm,
    pub dark: bool,
    /// Last installed `(paradigm, dark)` pair — re-install the theme
    /// whenever either changes.
    installed: Option<(Paradigm, bool)>,
    pub shell: Shell,
    pub form: ConnectForm,
    pub identity: Arc<Identity>,
    pub backend: NativeBackend,
    /// Dispatch `Command::Connect` once, on the first frame. Set via
    /// `RUMBLE_NEXT_AUTOCONNECT=1`; lets us smoke-test connected UI
    /// headlessly.
    auto_connect_pending: bool,
    pub toasts: ToastManager,
    /// Tracks the previous connection state so we can emit a toast only
    /// when the state transitions (not every frame the state is stable).
    prev_connection: Option<ConnectionKind>,
    /// In-memory state of the settings panel (which category is open).
    pub settings_ui: SettingsState,
    /// Persisted user settings (paradigm, dark mode, recent servers,
    /// trusted certs). Saved synchronously on each mutation.
    pub settings: SettingsStore,
    /// Global hotkey service (PTT hold-to-talk, toggle mute, toggle
    /// deafen). Wired in step 3 of the rumble-next bringup.
    hotkeys: HotkeyManager,
    /// Tokio runtime kept alive for the duration of `App` so the
    /// XDG GlobalShortcuts portal listener keeps running. Held even
    /// on non-Wayland systems — the cost of an unused runtime is
    /// negligible compared to the conditional-construction churn.
    _runtime: tokio::runtime::Runtime,
    /// Processor registry used by the settings panel to enumerate
    /// available TX-pipeline stages and introspect their schemas.
    /// Built once at startup; the same factories the audio task uses
    /// internally, so what the UI shows matches what actually runs.
    pub processor_registry: ProcessorRegistry,
}

#[derive(Copy, Clone, PartialEq, Eq)]
enum ConnectionKind {
    Disconnected,
    Connecting,
    Connected,
    Lost,
    CertPending,
}

impl ConnectionKind {
    fn from(state: &rumble_protocol::ConnectionState) -> Self {
        use rumble_protocol::ConnectionState as S;
        match state {
            S::Disconnected => Self::Disconnected,
            S::Connecting { .. } => Self::Connecting,
            S::Connected { .. } => Self::Connected,
            S::ConnectionLost { .. } => Self::Lost,
            S::CertificatePending { .. } => Self::CertPending,
        }
    }
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> std::io::Result<Self> {
        let config_dir = default_config_dir();
        let identity = Arc::new(Identity::load_or_create(&config_dir)?);

        let settings = SettingsStore::load_from_path(Some(config_dir.join("desktop-shell.json")));
        let paradigm = settings
            .settings()
            .paradigm
            .as_deref()
            .and_then(Paradigm::from_persist_str)
            .unwrap_or(Paradigm::Modern);
        let dark = settings.settings().dark;

        let mut config = ConnectConfig::new();
        // Convenience: trust dev certs checked into the repo so the
        // first-connect flow doesn't require hand-approval when running
        // against a local server.
        for candidate in ["dev-certs/server-cert.der", "certs/fullchain.pem"] {
            if std::path::Path::new(candidate).exists() {
                config = config.with_cert(candidate);
            }
        }
        if let Ok(cert_path) = std::env::var("RUMBLE_SERVER_CERT_PATH") {
            config = config.with_cert(cert_path);
        }
        // Re-trust certs the user has already accepted in past sessions.
        // Without this, every restart triggers another approval prompt
        // for self-signed servers — the gripe step 2 was meant to fix.
        for entry in &settings.settings().accepted_certificates {
            match entry.der_bytes() {
                Some(der) => config.accepted_certs.push(der),
                None => tracing::warn!(
                    "settings: accepted cert for {} has invalid base64 — ignored",
                    entry.server_name
                ),
            }
        }

        let ctx_for_repaint = cc.egui_ctx.clone();
        let backend = BackendHandle::with_config(
            move || {
                ctx_for_repaint.request_repaint();
            },
            config,
        );

        // Two ways to auto-connect at launch:
        //   1. The `RUMBLE_NEXT_AUTOCONNECT` env var still works for
        //      headless smoke runs (`examples/screenshot.rs`).
        //   2. The `auto_connect_addr` setting points at one of the
        //      saved servers — set via the connect view's "connect on
        //      launch" checkbox. This is the user-facing path.
        let auto_connect_pending = std::env::var("RUMBLE_NEXT_AUTOCONNECT")
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false)
            || settings.settings().auto_connect_addr.is_some();

        // Tokio runtime for portal-backed hotkeys. A single worker is
        // enough — the portal listener spends almost all its time
        // awaiting D-Bus signals.
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .map_err(|e| std::io::Error::other(format!("tokio runtime: {e}")))?;

        let mut hotkeys = HotkeyManager::new();
        let runtime_handle = runtime.handle().clone();
        runtime.block_on(async {
            hotkeys.init_portal_backend(runtime_handle).await;
        });
        if let Err(e) = hotkeys.register_from_settings(&settings.settings().keyboard) {
            tracing::warn!("hotkey registration failed: {e}");
        }

        let mut processor_registry = ProcessorRegistry::new();
        register_builtin_processors(&mut processor_registry);

        Ok(Self {
            paradigm,
            dark,
            installed: None,
            shell: Shell::default(),
            form: ConnectForm::default(),
            identity,
            backend,
            auto_connect_pending,
            toasts: ToastManager::new(),
            prev_connection: None,
            settings_ui: SettingsState::default(),
            settings,
            hotkeys,
            _runtime: runtime,
            processor_registry,
        })
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        let want = (self.paradigm, self.dark);
        if self.installed != Some(want) {
            install_theme(ctx, self.paradigm.make_theme(self.dark));
            self.installed = Some(want);
        }

        // One state snapshot per frame. The backend calls our repaint
        // callback when something changes, so we get a fresh snapshot
        // exactly when needed.
        let state = self.backend.state();

        self.pump_toasts(&state);
        self.pump_hotkeys(ctx, &state);

        // Refresh display preferences that the shell needs each frame.
        // Cheap copies; toggling the setting takes effect immediately.
        let chat = &self.settings.settings().chat;
        self.shell.chat_timestamp_format = chat.show_timestamps.then_some(chat.timestamp_format);

        // Auto-connect once if requested. Resolve the target server in
        // priority order:
        //   1. `auto_connect_addr` setting → look up its `RecentServer`
        //      and use its saved username.
        //   2. Otherwise (env-var smoke path), fall back to the form
        //      defaults (currently `[::1]:5000` / $USER).
        if self.auto_connect_pending && matches!(state.connection, rumble_protocol::ConnectionState::Disconnected) {
            let target = self
                .settings
                .settings()
                .auto_connect_addr
                .clone()
                .and_then(|addr| {
                    self.settings
                        .settings()
                        .recent_servers
                        .iter()
                        .find(|r| r.addr == addr)
                        .map(|r| (r.addr.clone(), r.username.clone()))
                })
                .unwrap_or_else(|| (self.form.editing.addr.clone(), self.form.editing.username.clone()));
            self.backend.send(Command::Connect {
                addr: target.0,
                name: target.1,
                public_key: self.identity.public_key(),
                signer: self.identity.signer(),
                password: None,
            });
            self.auto_connect_pending = false;
        }

        TopBottomPanel::top("rumble_next_paradigm_picker")
            .resizable(false)
            .show(ctx, |ui| {
                SurfaceFrame::new(SurfaceKind::Toolbar)
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let tokens = ui.theme().tokens().clone();
                            ui.label(
                                RichText::new("rumble-next · paradigm:")
                                    .color(tokens.text_muted)
                                    .font(tokens.font_label.clone()),
                            );
                            for p in Paradigm::ALL {
                                let active = *p == self.paradigm;
                                if ButtonArgs::new(p.label())
                                    .role(PressableRole::Accent)
                                    .active(active)
                                    .min_width(110.0)
                                    .show(ui)
                                    .clicked()
                                    && self.paradigm != *p
                                {
                                    self.paradigm = *p;
                                    self.settings.modify(|s| {
                                        s.paradigm = Some(p.as_persist_str().to_string());
                                    });
                                }
                            }

                            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                                // Connection summary on the far right.
                                let summary = crate::adapters::connection_summary(&state);
                                ui.label(
                                    RichText::new(summary)
                                        .color(tokens.text_muted)
                                        .font(tokens.font_mono.clone()),
                                );

                                ui.add_space(12.0);

                                // Dark-mode toggle. Sun glyph on light,
                                // moon on dark — click flips.
                                let (label, active) = if self.dark {
                                    ("🌙 dark", true)
                                } else {
                                    ("☀ light", false)
                                };
                                if ButtonArgs::new(label)
                                    .role(PressableRole::Ghost)
                                    .active(active)
                                    .min_width(72.0)
                                    .show(ui)
                                    .clicked()
                                {
                                    self.dark = !self.dark;
                                    let dark = self.dark;
                                    self.settings.modify(|s| s.dark = dark);
                                }
                            });
                        });
                    });
            });

        CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let tokens = ui.theme().tokens().clone();
            ui.painter()
                .rect_filled(ui.max_rect(), egui::CornerRadius::ZERO, tokens.surface);

            if !state.connection.is_connected() {
                connect_view::render(
                    ui,
                    &state,
                    &mut self.form,
                    &mut self.settings,
                    &self.identity,
                    &self.backend,
                );
                return;
            }

            match self.paradigm {
                Paradigm::Modern => paradigm::modern::render(ui, &mut self.shell, &state, &self.backend),
                Paradigm::MumbleClassic => paradigm::mumble::render(ui, &mut self.shell, &state, &self.backend),
                Paradigm::Luna => paradigm::luna::render(ui, &mut self.shell, &state, &self.backend),
            }
        });

        if state.connection.is_connected() {
            self.shell.render_overlays(ctx, &state, &self.backend);
        }

        let pub_hex = hex::encode(self.identity.public_key());
        settings_panel::render(
            ctx,
            &mut self.shell.settings_open,
            &mut self.settings_ui,
            &mut self.settings,
            &state,
            &self.backend,
            &self.processor_registry,
            &pub_hex,
        );

        self.toasts.render(ctx);
    }
}

impl App {
    /// Translate one-shot state signals into toast notifications:
    /// connection transitions, permission-denied messages, and kick
    /// reasons. `permission_denied` and `kicked` are `take()`-style
    /// fields on `State` — we drain them via `state_mut()` so a single
    /// event surfaces exactly once.
    fn pump_toasts(&mut self, state: &rumble_protocol::State) {
        let kind = ConnectionKind::from(&state.connection);
        if self.prev_connection != Some(kind) {
            if let Some(prev) = self.prev_connection {
                match (prev, kind) {
                    (_, ConnectionKind::Connected) => {
                        self.toasts.success("Connected to server");
                        self.play_sfx(rumble_client::SfxKind::Connect);
                    }
                    (ConnectionKind::Connected, ConnectionKind::Lost) => {
                        if let rumble_protocol::ConnectionState::ConnectionLost { error } = &state.connection {
                            self.toasts.error(format!("Connection lost: {error}"));
                            self.play_sfx(rumble_client::SfxKind::Disconnect);
                        }
                    }
                    (ConnectionKind::Connecting, ConnectionKind::Lost) => {
                        if let rumble_protocol::ConnectionState::ConnectionLost { error } = &state.connection {
                            self.toasts.error(format!("Could not connect: {error}"));
                        }
                    }
                    _ => {}
                }
            }
            self.prev_connection = Some(kind);
        }

        // Drain one-shot fields. These are `Option<String>`; taking them
        // ensures we only fire the toast once.
        let (perm, kicked) = {
            let mut guard = self.backend.state_mut();
            (guard.permission_denied.take(), guard.kicked.take())
        };
        if let Some(msg) = perm {
            self.toasts.error(format!("Permission denied: {msg}"));
        }
        if let Some(reason) = kicked {
            self.toasts.error(format!("You were kicked: {reason}"));
        }
    }

    /// Drain hotkey events and turn them into backend commands.
    /// Mirrors `rumble-egui::handle_hotkey_event` so behaviour stays
    /// consistent across clients.
    ///
    /// Two sources of events:
    ///   1. Global hotkeys (X11 / Windows / macOS / Wayland portal)
    ///      via `HotkeyManager::poll_events()`.
    ///   2. Window-focused fallback — when the window has focus and
    ///      no global registration is active (e.g. Wayland without
    ///      portal), we synthesise PTT press/release from egui's
    ///      `key_pressed` / `key_released` so the user always has a
    ///      working PTT inside the window.
    ///
    /// Mute/deafen / PTT only act when connected — toggling state
    /// while disconnected would be confusing.
    fn pump_hotkeys(&mut self, ctx: &eframe::egui::Context, state: &rumble_protocol::State) {
        for event in self.hotkeys.poll_events() {
            self.dispatch_hotkey(event, state);
        }

        // Window-focused fallback. Only fires if global hotkeys aren't
        // bound — otherwise we'd double-dispatch on every keypress.
        if !self.hotkeys.is_available()
            && !self.hotkeys.has_portal_backend()
            && let Some(binding) = self.settings.settings().keyboard.ptt_hotkey.as_ref()
            && let Some(key) = HotkeyManager::key_string_to_egui_key(&binding.key)
        {
            let (pressed, released) = ctx.input(|i| (i.key_pressed(key), i.key_released(key)));
            if pressed {
                self.dispatch_hotkey(HotkeyEvent::PttPressed, state);
            }
            if released {
                self.dispatch_hotkey(HotkeyEvent::PttReleased, state);
            }
        }
    }

    fn dispatch_hotkey(&self, event: HotkeyEvent, state: &rumble_protocol::State) {
        if !state.connection.is_connected() {
            return;
        }
        match event {
            HotkeyEvent::PttPressed => {
                tracing::debug!("hotkey: PTT pressed");
                self.backend.send(Command::StartTransmit);
            }
            HotkeyEvent::PttReleased => {
                tracing::debug!("hotkey: PTT released");
                self.backend.send(Command::StopTransmit);
            }
            HotkeyEvent::ToggleMute => {
                tracing::debug!("hotkey: toggle mute");
                let new_muted = !state.audio.self_muted;
                self.backend.send(Command::SetMuted { muted: new_muted });
                self.play_sfx(if new_muted {
                    rumble_client::SfxKind::Mute
                } else {
                    rumble_client::SfxKind::Unmute
                });
            }
            HotkeyEvent::ToggleDeafen => {
                tracing::debug!("hotkey: toggle deafen");
                self.backend.send(Command::SetDeafened {
                    deafened: !state.audio.self_deafened,
                });
            }
        }
    }

    /// Dispatch a sound-effect playback command, gated by the user's
    /// SFX settings. Volume mixes the configured level with the
    /// per-event default (1.0 here — calls don't currently customise).
    fn play_sfx(&self, kind: rumble_client::SfxKind) {
        let sfx = &self.settings.settings().sfx;
        if !sfx.enabled || sfx.volume <= 0.0 {
            return;
        }
        self.backend.send(Command::PlaySfx {
            kind,
            volume: sfx.volume.clamp(0.0, 1.0),
        });
    }
}
