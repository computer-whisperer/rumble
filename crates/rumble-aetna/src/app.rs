//! Top-level aetna `App` for the Rumble client.
//!
//! Owns local UI state (connect form fields, modal flags, selected
//! room) and projects `(state, ui_state) -> El` on every frame.

use std::{
    sync::LazyLock,
    time::{SystemTime, UNIX_EPOCH},
};

use aetna_core::prelude::*;

use rumble_client::{
    ProcessorRegistry, build_default_tx_pipeline, merge_with_default_tx_pipeline, register_builtin_processors,
};
use rumble_desktop_shell::{
    AcceptedCertificate, RecentServer, SettingsStore,
    identity::{connect_and_list_keys, generate_and_add_to_agent},
};
use rumble_protocol::{Command, ConnectionState, PendingCertificate, State, VoiceMode};
use tokio::runtime::Runtime;

use crate::{
    backend::UiBackend,
    identity::Identity,
    settings::{self, SettingsOutcome, SettingsState},
    theme::{self as palette},
    wizard::{self, PendingAgentOp, UnlockState, WizardOutcome, WizardState},
};

// ---- Bundled Mumble SVG glyphs ----
//
// Parsed once at first use (Arc-bumped on every `icon(...)` call) and
// shared across frames. We use `SvgIcon::parse` (not
// `parse_current_color`) because the Mumble theme bakes its semantic
// colors into the SVG paint — red for self-mute, blue for talking, etc.
// — and those colors are exactly the visual signal we want to keep.

static SVG_TALKING_ON: LazyLock<SvgIcon> =
    LazyLock::new(|| SvgIcon::parse(include_str!("../assets/icons/talking_on.svg")).expect("talking_on.svg parses"));
static SVG_TALKING_OFF: LazyLock<SvgIcon> =
    LazyLock::new(|| SvgIcon::parse(include_str!("../assets/icons/talking_off.svg")).expect("talking_off.svg parses"));
static SVG_MUTED_SELF: LazyLock<SvgIcon> =
    LazyLock::new(|| SvgIcon::parse(include_str!("../assets/icons/muted_self.svg")).expect("muted_self.svg parses"));
static SVG_MUTED_SERVER: LazyLock<SvgIcon> = LazyLock::new(|| {
    SvgIcon::parse(include_str!("../assets/icons/muted_server.svg")).expect("muted_server.svg parses")
});

pub struct RumbleApp<B: UiBackend = crate::backend::NativeUiBackend> {
    backend: B,
    identity: Identity,
    settings: SettingsStore,

    /// Tokio runtime for spawning ssh-agent ops and other async work
    /// that needs to outlive a single event handler. The wizard polls
    /// `pending_agent_op.is_finished()` each frame and `block_on`s the
    /// completed handle to land the result on the same frame.
    runtime: Runtime,

    /// First-run identity wizard. `NotNeeded` when an identity is
    /// already configured.
    wizard: WizardState,
    /// Encrypted-key unlock prompt state. Only shown when
    /// `identity.needs_unlock()` is true and the wizard is hidden.
    unlock: UnlockState,
    /// In-flight ssh-agent op spawned on `runtime`.
    pending_agent_op: Option<PendingAgentOp>,

    // ---- Local UI state ----
    /// Add/edit form for a saved server. `None` when closed; `Some`
    /// holds either a fresh add (`editing_index = None`) or an edit
    /// pointed at a row in `SettingsStore.recent_servers`.
    server_form: Option<ServerForm>,
    identity_modal_open: bool,
    settings_state: SettingsState,
    /// Force the unlock prompt visible regardless of `needs_unlock()`.
    /// Set by `set_unlock_state_for_test` so `dump_bundles` can render
    /// the prompt against a fresh on-disk identity that isn't actually
    /// encrypted.
    force_unlock_for_test: bool,
    /// Default username pre-filled when adding a brand-new server entry.
    /// Sourced from `$USER` at startup; updated by the settings dialog.
    /// Per-server usernames live on each `RecentServer` and override
    /// this on connect.
    default_username: String,
    /// Global text-selection slot. Every `text_input` reads its caret /
    /// selection band through `selection.within(key)`; `apply_event`
    /// folds keypresses + clicks back into this single field.
    selection: Selection,

    chat_input: String,

    /// Chat sidebar width in logical pixels — adjusted by dragging
    /// the divider on its right edge. Initialized from
    /// [`tokens::SIDEBAR_WIDTH`] (the conventional ~256px starting
    /// point) and clamped to [`tokens::SIDEBAR_WIDTH_MIN`] /
    /// `_MAX` by the resize handler.
    chat_sidebar_w: f32,
    chat_sidebar_drag: ResizeDrag,

    /// Audio-processor factory registry. Owned by the App so the
    /// settings dialog can read each processor's display name,
    /// description and JSON schema when rendering the Processing tab.
    /// Built once in [`Self::new`] from `register_builtin_processors`.
    processor_registry: ProcessorRegistry,
}

impl<B: UiBackend> RumbleApp<B> {
    pub fn new(backend: B, identity: Identity, settings: SettingsStore, runtime: Runtime) -> Self {
        let wizard = if identity.needs_setup() {
            WizardState::SelectMethod
        } else {
            WizardState::NotNeeded
        };

        // Build the audio-processor registry up front. The schema is
        // also needed by the settings dialog at render time, so the
        // App owns the registry for the lifetime of the process.
        let mut processor_registry = ProcessorRegistry::new();
        register_builtin_processors(&mut processor_registry);

        // Push the initial TX pipeline at boot — either the user's
        // persisted config (merged against the current defaults so
        // newly-added processors slot in), or a fresh default chain.
        let initial_pipeline = match settings.settings().audio.tx_pipeline.as_ref() {
            Some(persisted) => merge_with_default_tx_pipeline(persisted, &processor_registry),
            None => build_default_tx_pipeline(&processor_registry),
        };
        backend.send(Command::UpdateTxPipeline {
            config: initial_pipeline,
        });

        Self {
            backend,
            identity,
            settings,
            runtime,
            wizard,
            unlock: UnlockState::default(),
            pending_agent_op: None,
            server_form: None,
            identity_modal_open: false,
            settings_state: SettingsState::default(),
            force_unlock_for_test: false,
            default_username: default_username(),
            selection: Selection::default(),
            chat_input: String::new(),
            chat_sidebar_w: tokens::SIDEBAR_WIDTH,
            chat_sidebar_drag: ResizeDrag::default(),
            processor_registry,
        }
    }
}

fn default_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "rumble-user".to_string())
}

/// Local state for the saved-server add/edit modal.
///
/// `editing_index` is the position in `Settings.recent_servers` when
/// editing an existing entry, or `None` when the form is being used
/// to add a new server. The text fields are rendered via the global
/// [`Selection`] held on `RumbleApp`, so this struct doesn't carry
/// per-input selection state.
#[derive(Debug, Clone)]
pub struct ServerForm {
    pub editing_index: Option<usize>,
    pub addr: String,
    pub label: String,
    pub username: String,
    /// Inline validation message (e.g. "Address is required").
    pub error: Option<String>,
}

impl ServerForm {
    fn for_add(default_username: &str) -> Self {
        Self {
            editing_index: None,
            addr: "127.0.0.1:5000".to_string(),
            label: String::new(),
            username: default_username.to_string(),
            error: None,
        }
    }

    fn for_edit(idx: usize, server: &RecentServer) -> Self {
        Self {
            editing_index: Some(idx),
            addr: server.addr.clone(),
            label: server.label.clone(),
            username: server.username.clone(),
            error: None,
        }
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Coarse "n minutes ago" / "n hours ago" / "n days ago" formatter for
/// the server-row subtitle. Returns an empty string when `then == 0`
/// (the sentinel for "never connected to this server").
fn relative_time(then: u64) -> String {
    if then == 0 {
        return String::new();
    }
    let now = now_unix();
    if now <= then {
        return "just now".to_string();
    }
    let secs = now - then;
    if secs < 60 {
        format!("{secs}s ago")
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86_400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86_400)
    }
}

impl<B: UiBackend> App for RumbleApp<B> {
    fn before_build(&mut self) {
        self.poll_agent_op();
    }

    fn build(&self) -> El {
        let state = self.backend.state();
        let shell = self.settings.settings();

        let main = column([
            top_toolbar(&state),
            row([
                chat_sidebar(&state, &self.chat_input, &self.selection, self.chat_sidebar_w),
                resize_handle(Axis::Row).key(CHAT_SIDEBAR_HANDLE),
                center_area(&state, &shell.recent_servers),
            ])
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0))
            .align(Align::Stretch),
        ])
        .fill_size()
        .align(Align::Stretch);

        // Wizard takes precedence over everything else — until an identity
        // is configured the rest of the UI is read-only.
        let wizard_open = !matches!(self.wizard, WizardState::NotNeeded | WizardState::Complete);
        let wizard_layer = wizard::render(&self.wizard, self.pending_agent_op.is_some(), &self.selection);

        let unlock_layer = if !wizard_open && (self.identity.needs_unlock() || self.force_unlock_for_test) {
            Some(wizard::render_unlock(&self.unlock, &self.selection))
        } else {
            None
        };

        let cert_layer = if !wizard_open
            && unlock_layer.is_none()
            && let ConnectionState::CertificatePending { cert_info } = &state.connection
        {
            Some(cert_modal(cert_info))
        } else {
            None
        };
        // Suppress the server form whenever a higher-priority modal is up.
        let connect_layer = if !wizard_open
            && unlock_layer.is_none()
            && cert_layer.is_none()
            && let Some(form) = self.server_form.as_ref()
        {
            Some(server_form_modal(form, &self.selection))
        } else {
            None
        };

        let identity_layer =
            if self.identity_modal_open && !wizard_open && unlock_layer.is_none() && cert_layer.is_none() {
                Some(identity_modal(&self.identity))
            } else {
                None
            };

        let (settings_panel, settings_popover) = if !wizard_open && unlock_layer.is_none() && cert_layer.is_none() {
            settings::render(
                &self.settings_state,
                &state,
                &self.identity,
                &self.selection,
                &self.processor_registry,
            )
        } else {
            (None, None)
        };

        let any_layer = wizard_layer.is_some()
            || unlock_layer.is_some()
            || cert_layer.is_some()
            || connect_layer.is_some()
            || identity_layer.is_some()
            || settings_panel.is_some()
            || settings_popover.is_some();
        if any_layer {
            // Layer order matters: paints back-to-front. The settings
            // popover sits above its panel; the wizard sits on top of
            // everything because nothing else is allowed to interact
            // while it's open.
            overlays(
                main,
                [
                    identity_layer,
                    connect_layer,
                    settings_panel,
                    settings_popover,
                    cert_layer,
                    unlock_layer,
                    wizard_layer,
                ],
            )
        } else {
            main
        }
    }

    fn selection(&self) -> Selection {
        self.selection.clone()
    }

    fn on_event(&mut self, event: UiEvent) {
        // The runtime emits `SelectionChanged` when a press / focus move
        // lands somewhere other than a text input — fold it into our
        // single selection slot so static-text + cross-leaf selections
        // clear correctly.
        if event.kind == UiEventKind::SelectionChanged
            && let Some(sel) = event.selection.as_ref()
        {
            self.selection = sel.clone();
            return;
        }

        // Wizard / unlock layers swallow everything until they're done.
        // The wizard scrim is intentionally a no-op (no "click outside
        // to dismiss") so the user can't end up with a half-configured
        // identity by hitting Escape.
        if !matches!(self.wizard, WizardState::NotNeeded | WizardState::Complete) {
            let outcome = wizard::handle_event(&mut self.wizard, &event, &mut self.selection);
            self.dispatch_wizard_outcome(outcome);
            return;
        }
        if self.identity.needs_unlock() {
            let outcome = wizard::handle_unlock_event(&mut self.unlock, &event, &mut self.selection);
            self.dispatch_wizard_outcome(outcome);
            return;
        }

        // Settings dialog owns its own routed-key namespace; let it
        // claim its events first so the toolbar / chat / room handlers
        // below don't accidentally swallow them.
        if self.settings_state.open {
            let outcome = settings::handle_event(
                &mut self.settings_state,
                &event,
                &mut self.selection,
                &self.processor_registry,
            );
            if self.dispatch_settings_outcome(outcome) {
                return;
            }
        }

        // Chat sidebar resize. Routed events return early so the
        // handle's drag stream doesn't fall through to other matchers.
        if event.route() == Some(CHAT_SIDEBAR_HANDLE) {
            resize_handle::apply_event_fixed(
                &mut self.chat_sidebar_w,
                &mut self.chat_sidebar_drag,
                &event,
                CHAT_SIDEBAR_HANDLE,
                Axis::Row,
                tokens::SIDEBAR_WIDTH_MIN,
                tokens::SIDEBAR_WIDTH_MAX,
            );
            return;
        }

        // Saved-server list lifecycle.
        if event.is_click_or_activate("server:add") {
            self.server_form = Some(ServerForm::for_add(&self.default_username));
            return;
        }
        if let Some(idx) = matches!(event.kind, UiEventKind::Click | UiEventKind::Activate)
            .then(|| event.route().and_then(|r| r.strip_prefix("server:connect:")))
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.connect_to_recent(idx);
            return;
        }
        if let Some(idx) = matches!(event.kind, UiEventKind::Click | UiEventKind::Activate)
            .then(|| event.route().and_then(|r| r.strip_prefix("server:edit:")))
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.open_edit_form(idx);
            return;
        }
        if let Some(idx) = matches!(event.kind, UiEventKind::Click | UiEventKind::Activate)
            .then(|| event.route().and_then(|r| r.strip_prefix("server:delete:")))
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.delete_recent(idx);
            return;
        }

        // Add/edit server form lifecycle.
        if event.is_click_or_activate("server_form:cancel")
            || event.is_route("server_form:dismiss") && event.kind == UiEventKind::Click
            || (self.server_form.is_some() && event.kind == UiEventKind::Escape)
        {
            self.server_form = None;
            return;
        }
        if event.is_click_or_activate("server_form:save") {
            if let Some(_saved) = self.save_server_form() {
                self.server_form = None;
            }
            return;
        }
        if event.is_click_or_activate("server_form:connect") {
            if let Some(saved) = self.save_server_form() {
                self.server_form = None;
                self.connect_to_server(&saved);
            }
            return;
        }
        if event.target_key() == Some("server_form:addr") {
            if let Some(form) = self.server_form.as_mut() {
                text_input::apply_event(&mut form.addr, &mut self.selection, "server_form:addr", &event);
            }
            return;
        }
        if event.target_key() == Some("server_form:label") {
            if let Some(form) = self.server_form.as_mut() {
                text_input::apply_event(&mut form.label, &mut self.selection, "server_form:label", &event);
            }
            return;
        }
        if event.target_key() == Some("server_form:user") {
            if let Some(form) = self.server_form.as_mut() {
                text_input::apply_event(&mut form.username, &mut self.selection, "server_form:user", &event);
            }
            return;
        }

        // Cert acceptance prompt. The modal is rendered whenever
        // `state.connection` is `CertificatePending`; clicking the scrim
        // is intentionally a no-op so the user has to make an explicit
        // accept/reject decision.
        if event.is_click_or_activate("cert:accept") {
            self.accept_pending_cert();
            return;
        }
        if event.is_click_or_activate("cert:reject") {
            self.backend.send(Command::RejectCertificate);
            return;
        }

        // Chat composer.
        if event.target_key() == Some("chat:input") {
            // Send on Enter when not Shift-held.
            if let UiEventKind::KeyDown = event.kind
                && let Some(kp) = event.key_press.as_ref()
                && matches!(kp.key, UiKey::Enter)
                && !kp.modifiers.shift
            {
                let trimmed = self.chat_input.trim().to_string();
                if !trimmed.is_empty() {
                    self.backend.send(Command::SendChat { text: trimmed });
                    self.chat_input.clear();
                    self.selection = Selection::default();
                }
                return;
            }
            text_input::apply_event(&mut self.chat_input, &mut self.selection, "chat:input", &event);
            return;
        }

        // Top toolbar.
        if event.is_click_or_activate("toolbar:mute") {
            let muted = self.backend.state().audio.self_muted;
            self.backend.send(Command::SetMuted { muted: !muted });
            return;
        }
        if event.is_click_or_activate("toolbar:deafen") {
            let deafened = self.backend.state().audio.self_deafened;
            self.backend.send(Command::SetDeafened { deafened: !deafened });
            return;
        }
        if event.is_click_or_activate("toolbar:voice-mode") {
            let mode = self.backend.state().audio.voice_mode;
            let next = match mode {
                VoiceMode::PushToTalk => VoiceMode::Continuous,
                VoiceMode::Continuous => VoiceMode::PushToTalk,
            };
            self.backend.send(Command::SetVoiceMode { mode: next });
            return;
        }
        if event.is_click_or_activate("toolbar:disconnect") {
            self.backend.send(Command::Disconnect);
            return;
        }
        if event.is_click_or_activate("toolbar:identity") {
            self.identity_modal_open = true;
            return;
        }
        if event.is_click_or_activate("toolbar:settings") {
            let snapshot = self.backend.state();
            self.settings_state
                .open_with(&snapshot.audio, self.settings.settings(), &self.default_username);
            return;
        }
        if event.is_click_or_activate("identity:close")
            || event.is_route("identity:dismiss") && event.kind == UiEventKind::Click
            || (self.identity_modal_open && event.kind == UiEventKind::Escape)
        {
            self.identity_modal_open = false;
            return;
        }
        if event.is_click_or_activate("identity:regenerate") {
            // Drop the modal, re-enter the wizard. Existing key on disk
            // is *not* deleted yet — only overwritten if the user
            // actually completes a Generate / Select flow.
            self.identity_modal_open = false;
            self.wizard = WizardState::SelectMethod;
            return;
        }

        // Click a room row to join it.
        if matches!(event.kind, UiEventKind::Click | UiEventKind::Activate)
            && let Some(key) = event.route()
            && let Some(room_id) = parse_room_route_key(key)
        {
            self.backend.send(Command::JoinRoom { room_id });
            self.backend.send(Command::RequestChatHistory);
        }
    }
}

impl<B: UiBackend> RumbleApp<B> {
    /// Persist the currently-pending cert into shared shell settings and
    /// tell the backend to proceed. Dedup by `(server_name, fingerprint)`
    /// so accepting the same cert twice doesn't grow the file.
    fn accept_pending_cert(&mut self) {
        let snapshot = self.backend.state();
        let Some(cert_info) = (match &snapshot.connection {
            ConnectionState::CertificatePending { cert_info } => Some(cert_info.clone()),
            _ => None,
        }) else {
            // Race: state changed between event delivery and now. Send
            // the accept anyway — if there's nothing pending the backend
            // will just warn and ignore it.
            self.backend.send(Command::AcceptCertificate);
            return;
        };
        let server_name = cert_info.server_name.clone();
        let fingerprint = cert_info.fingerprint_hex();
        let der = cert_info.certificate_der.clone();
        self.settings.modify(|s| {
            let already = s
                .accepted_certificates
                .iter()
                .any(|c| c.server_name == server_name && c.fingerprint_hex == fingerprint);
            if !already {
                s.accepted_certificates
                    .push(AcceptedCertificate::from_der(server_name, fingerprint, &der));
            }
        });
        self.backend.send(Command::AcceptCertificate);
    }

    /// Look up `recent_servers[idx]` and dispatch a connect to it.
    /// `idx` is the position in the unsorted Vec — that's the only stable
    /// identifier the row keys carry, so it's safe across re-renders that
    /// re-sort the list visually.
    fn connect_to_recent(&mut self, idx: usize) {
        let Some(server) = self.settings.settings().recent_servers.get(idx).cloned() else {
            tracing::warn!("rumble-aetna: connect_to_recent({idx}) — out of bounds");
            return;
        };
        self.connect_to_server(&server);
    }

    /// Open the form pre-populated with `recent_servers[idx]`. No-op if
    /// idx is out of bounds — the row that sourced the click has gone.
    fn open_edit_form(&mut self, idx: usize) {
        let Some(server) = self.settings.settings().recent_servers.get(idx).cloned() else {
            return;
        };
        self.server_form = Some(ServerForm::for_edit(idx, &server));
    }

    fn delete_recent(&mut self, idx: usize) {
        self.settings.modify(|s| {
            if idx < s.recent_servers.len() {
                let removed = s.recent_servers.remove(idx);
                if s.auto_connect_addr.as_deref() == Some(removed.addr.as_str()) {
                    s.auto_connect_addr = None;
                }
            }
        });
    }

    /// Run pre-flight identity checks and dispatch `Command::Connect`
    /// for `server`. Bumps the entry's `last_used_unix` so the list
    /// re-sorts the next frame.
    fn connect_to_server(&mut self, server: &RecentServer) {
        let Some(public_key) = self.identity.public_key() else {
            self.backend.send(Command::LocalMessage {
                text: "Cannot connect: No identity key configured. Please complete first-run setup.".to_string(),
            });
            return;
        };
        if self.identity.needs_unlock() {
            self.backend.send(Command::LocalMessage {
                text: "Cannot connect: Key is encrypted. Please unlock it in settings.".to_string(),
            });
            return;
        }
        let signer = self.identity.signer();

        let addr = if server.addr.trim().is_empty() {
            "127.0.0.1:5000".to_string()
        } else {
            server.addr.trim().to_string()
        };
        let name = if server.username.trim().is_empty() {
            self.default_username.clone()
        } else {
            server.username.trim().to_string()
        };

        // Mark this entry as the most-recently used so it floats to the
        // top of the list. Done before the connect dispatch so a refused
        // connection still updates the order — matches rumble-egui.
        let bump_addr = addr.clone();
        let bump_name = name.clone();
        self.settings.modify(|s| {
            if let Some(entry) = s.recent_servers.iter_mut().find(|r| r.addr == bump_addr) {
                entry.last_used_unix = now_unix();
                entry.username = bump_name;
            }
        });

        self.backend.send(Command::LocalMessage {
            text: format!("Connecting to {addr}..."),
        });
        self.backend.send(Command::Connect {
            addr,
            name,
            public_key,
            signer,
            password: None,
        });
    }

    /// Validate + persist the open `ServerForm`. Returns the saved
    /// `RecentServer` on success so callers can chain a connect; returns
    /// `None` (with `form.error` populated) on validation failure.
    ///
    /// Edit semantics: removes the original entry at `editing_index`
    /// first, then writes the new fields keyed by addr. If the new addr
    /// already exists at a different position, that row's label/username
    /// are overwritten and the edit's `last_used_unix` carries over to
    /// the larger of the two.
    fn save_server_form(&mut self) -> Option<RecentServer> {
        let Some(form) = self.server_form.as_mut() else {
            return None;
        };
        let addr = form.addr.trim().to_string();
        if addr.is_empty() {
            form.error = Some("Address is required.".to_string());
            return None;
        }
        let label = form.label.trim().to_string();
        let username = form.username.trim().to_string();
        let editing_index = form.editing_index;

        let mut saved: Option<RecentServer> = None;
        self.settings.modify(|s| {
            let preserved_last_used = editing_index
                .and_then(|idx| s.recent_servers.get(idx))
                .map(|r| r.last_used_unix)
                .unwrap_or(0);
            if let Some(idx) = editing_index
                && idx < s.recent_servers.len()
            {
                let original_addr = s.recent_servers[idx].addr.clone();
                s.recent_servers.remove(idx);
                if s.auto_connect_addr.as_deref() == Some(original_addr.as_str()) {
                    // Keep auto-connect pointing at this entry by
                    // updating the addr below.
                    s.auto_connect_addr = Some(addr.clone());
                }
            }
            let entry = if let Some(existing) = s.recent_servers.iter_mut().find(|r| r.addr == addr) {
                existing.label = label.clone();
                existing.username = username.clone();
                existing.last_used_unix = existing.last_used_unix.max(preserved_last_used);
                existing.clone()
            } else {
                let new_entry = RecentServer {
                    addr: addr.clone(),
                    label: label.clone(),
                    username: username.clone(),
                    last_used_unix: preserved_last_used,
                };
                s.recent_servers.push(new_entry.clone());
                new_entry
            };
            saved = Some(entry);
        });
        saved
    }

    // ---------- wizard plumbing ----------

    fn dispatch_wizard_outcome(&mut self, outcome: WizardOutcome) {
        match outcome {
            WizardOutcome::Ignored | WizardOutcome::Handled => {}
            WizardOutcome::SpawnConnect => {
                self.spawn_connect_op();
            }
            WizardOutcome::SpawnAddKey { comment } => {
                self.spawn_add_key_op(comment);
            }
            WizardOutcome::GenerateLocal { password } => {
                if let Some(info) = wizard::apply_generate_local(&mut self.wizard, &mut self.identity, password) {
                    self.notify_identity_ready(format!("Identity key generated: {}", info.fingerprint));
                }
            }
            WizardOutcome::SelectAgentKey { key_info } => {
                if let Some(info) = wizard::apply_select_agent_key(&mut self.wizard, &mut self.identity, &key_info) {
                    self.notify_identity_ready(format!("Using SSH agent key: {} ({})", info.comment, info.fingerprint));
                }
            }
            WizardOutcome::Unlock { password } => {
                if wizard::apply_unlock(&mut self.unlock, &mut self.identity) {
                    let _ = password;
                    self.backend.send(Command::LocalMessage {
                        text: "Identity unlocked.".to_string(),
                    });
                }
            }
        }
    }

    fn spawn_connect_op(&mut self) {
        if self.pending_agent_op.is_some() {
            return;
        }
        let handle = self.runtime.spawn(connect_and_list_keys());
        self.pending_agent_op = Some(PendingAgentOp::Connect(handle));
    }

    fn spawn_add_key_op(&mut self, comment: String) {
        if self.pending_agent_op.is_some() {
            return;
        }
        let handle = self.runtime.spawn(generate_and_add_to_agent(comment));
        self.pending_agent_op = Some(PendingAgentOp::AddKey(handle));
    }

    /// Drain a finished agent op, advancing wizard state with the result.
    /// Called from `before_build` so the new state is visible on the
    /// next frame.
    fn poll_agent_op(&mut self) {
        let Some(op) = self.pending_agent_op.as_ref() else {
            return;
        };
        let finished = match op {
            PendingAgentOp::Connect(h) => h.is_finished(),
            PendingAgentOp::AddKey(h) => h.is_finished(),
        };
        if !finished {
            return;
        }
        match self.pending_agent_op.take().unwrap() {
            PendingAgentOp::Connect(handle) => match self.runtime.block_on(handle) {
                Ok(Ok(keys)) => {
                    self.wizard = WizardState::SelectAgentKey {
                        keys,
                        selected: None,
                        error: None,
                    };
                }
                Ok(Err(e)) => {
                    self.wizard = WizardState::Error {
                        message: format!("Failed to connect to SSH agent: {e}"),
                    };
                }
                Err(e) => {
                    self.wizard = WizardState::Error {
                        message: format!("Agent operation panicked: {e}"),
                    };
                }
            },
            PendingAgentOp::AddKey(handle) => match self.runtime.block_on(handle) {
                Ok(Ok(key_info)) => {
                    if let Some(info) = wizard::apply_select_agent_key(&mut self.wizard, &mut self.identity, &key_info)
                    {
                        self.notify_identity_ready(format!(
                            "Added new SSH agent key: {} ({})",
                            info.comment, info.fingerprint
                        ));
                    }
                }
                Ok(Err(e)) => {
                    self.wizard = WizardState::Error {
                        message: format!("Failed to add key to agent: {e}"),
                    };
                }
                Err(e) => {
                    self.wizard = WizardState::Error {
                        message: format!("Agent operation panicked: {e}"),
                    };
                }
            },
        }
    }

    fn notify_identity_ready(&self, msg: String) {
        self.backend.send(Command::LocalMessage { text: msg });
    }

    /// Route a [`SettingsOutcome`] back into the App. Returns `true`
    /// when the outcome consumed the originating event, so the parent
    /// handler can short-circuit.
    fn dispatch_settings_outcome(&mut self, outcome: SettingsOutcome) -> bool {
        match outcome {
            SettingsOutcome::Ignored => false,
            SettingsOutcome::Handled => true,
            SettingsOutcome::Close => {
                self.settings_state.close();
                true
            }
            SettingsOutcome::OpenIdentityWizard => {
                self.settings_state.close();
                self.wizard = WizardState::SelectMethod;
                true
            }
            SettingsOutcome::PreviewSfx { kind, volume } => {
                self.backend.send(Command::PlaySfx { kind, volume });
                true
            }
            SettingsOutcome::RefreshDevices => {
                self.backend.send(Command::RefreshAudioDevices);
                true
            }
            SettingsOutcome::ResetStats => {
                self.backend.send(Command::ResetAudioStats);
                true
            }
            SettingsOutcome::Save(pending) => {
                self.apply_settings(pending);
                self.settings_state.close();
                true
            }
        }
    }

    /// Persist a [`PendingSettings`] snapshot: write the shared shell
    /// fields through `SettingsStore.modify`, dispatch backend commands
    /// for the runtime-mutating fields (audio settings, voice mode,
    /// device selection), and update App-owned state (username).
    fn apply_settings(&mut self, pending: settings::PendingSettings) {
        // App-owned: the global default is applied to brand-new server
        // entries created from the add-form. Per-server usernames live
        // on each `RecentServer` and aren't touched by this dialog.
        let trimmed = pending.username.trim();
        if !trimmed.is_empty() {
            self.default_username = trimmed.to_string();
        }

        // Backend: audio + voice mode + device selection. These all
        // hit the audio task immediately rather than going through the
        // settings store, so we send them even when the value didn't
        // change — they're idempotent.
        self.backend.send(Command::UpdateAudioSettings {
            settings: pending.audio.clone(),
        });
        self.backend.send(Command::SetVoiceMode {
            mode: VoiceMode::from(pending.voice_mode),
        });
        self.backend.send(Command::SetInputDevice {
            device_id: pending.input_device.clone(),
        });
        self.backend.send(Command::SetOutputDevice {
            device_id: pending.output_device.clone(),
        });
        self.backend.send(Command::UpdateTxPipeline {
            config: pending.tx_pipeline.clone(),
        });

        // Shared shell store.
        self.settings.modify(|s| {
            // Audio + voice mode mirror what the backend will report
            // back; persisting them here means a restart re-applies
            // the same configuration.
            s.audio = (&pending.audio).into();
            // The TX pipeline lives nested inside `PersistentAudioSettings`
            // and the `From<&AudioSettings>` impl above stamps it back
            // to `None`, so we re-apply it after the conversion.
            s.audio.tx_pipeline = Some(pending.tx_pipeline.clone());
            s.voice_mode = pending.voice_mode;
            s.input_device_id = pending.input_device.clone();
            s.output_device_id = pending.output_device.clone();

            // Sounds.
            s.sfx.enabled = pending.sfx_enabled;
            s.sfx.volume = pending.sfx_volume.clamp(0.0, 1.0);
            s.sfx.disabled_sounds.clear();
            for (idx, kind) in rumble_client::SfxKind::all().iter().enumerate() {
                if !pending.sfx_kind_enabled.get(idx).copied().unwrap_or(true) {
                    s.sfx.disabled_sounds.insert(*kind);
                }
            }

            // Chat.
            s.chat.show_timestamps = pending.show_timestamps;
            s.chat.timestamp_format = pending.timestamp_format;
            s.chat.auto_sync_history = pending.auto_sync_history;

            // Files (auto-download flag + bandwidth + flags only;
            // per-MIME rules aren't editable in this client yet).
            s.file_transfer.auto_download_enabled = pending.auto_download_enabled;
            s.file_transfer.download_speed_limit = (pending.download_speed_kbps as u64) * 1024;
            s.file_transfer.upload_speed_limit = (pending.upload_speed_kbps as u64) * 1024;
            s.file_transfer.seed_after_download = pending.seed_after_download;
            s.file_transfer.cleanup_on_exit = pending.cleanup_on_exit;

            // Autoconnect: only meaningful once we have a recent server
            // to point at, so reuse the most-recent entry's address. If
            // there isn't one yet we just store the flag intent by
            // marking the username on whatever current addr we have —
            // the actual auto-connect resolver runs at startup.
            if pending.autoconnect {
                let target = s
                    .recent_servers
                    .iter()
                    .max_by_key(|r| r.last_used_unix)
                    .map(|r| r.addr.clone());
                if let Some(addr) = target {
                    s.auto_connect_addr = Some(addr);
                } else {
                    // No recent server yet — clear so we don't claim
                    // to autoconnect to nothing.
                    s.auto_connect_addr = None;
                }
            } else {
                s.auto_connect_addr = None;
            }
        });

        self.backend.send(Command::LocalMessage {
            text: "Settings saved.".to_string(),
        });
    }

    /// Test/scene-dump escape hatch: pretend the identity wizard is
    /// satisfied so callers can render scenes that aren't supposed to
    /// be obscured by it (every scene in `dump_bundles`, every test).
    pub fn suppress_first_run_for_test(&mut self) {
        self.wizard = WizardState::NotNeeded;
        self.unlock = UnlockState::default();
    }

    /// Test/scene-dump hook: drive the wizard into a specific state so
    /// `dump_bundles` can render every wizard screen for visual review.
    pub fn set_wizard_state_for_test(&mut self, state: WizardState) {
        self.wizard = state;
    }

    /// Test/scene-dump hook for the encrypted-key unlock prompt. The
    /// prompt is normally gated on `Identity::needs_unlock()`; this also
    /// flips the test override so a fresh on-disk identity still
    /// produces the modal.
    pub fn set_unlock_state_for_test(&mut self, state: UnlockState) {
        self.unlock = state;
        self.force_unlock_for_test = true;
    }

    /// Test/scene-dump hook for the toolbar "Identity" modal.
    pub fn set_identity_modal_open_for_test(&mut self, open: bool) {
        self.identity_modal_open = open;
    }

    /// Test/scene-dump hook for the settings dialog. Snapshots the
    /// current backend audio state + shared shell settings into the
    /// settings UI state and forces the requested tab to active.
    pub fn open_settings_for_test(&mut self, tab: settings::SettingsTab) {
        let snapshot = self.backend.state();
        self.settings_state
            .open_with(&snapshot.audio, self.settings.settings(), &self.default_username);
        self.settings_state.tab = Some(tab);
    }

    /// Test/scene-dump hook to seed the saved-server list. Replaces the
    /// shared shell's `recent_servers` so the disconnected center area
    /// renders against a deterministic fixture.
    pub fn set_recent_servers_for_test(&mut self, servers: Vec<RecentServer>) {
        self.settings.modify(|s| {
            s.recent_servers = servers;
        });
    }

    /// Test/scene-dump hook for the saved-server add/edit modal.
    pub fn set_server_form_for_test(&mut self, form: ServerForm) {
        self.server_form = Some(form);
    }

    /// Test/scene-dump hook for the timestamp-format dropdown inside
    /// the settings dialog. Used to render the Chat tab with its
    /// dropdown menu open.
    pub fn open_settings_dropdown_for_test(&mut self, which: settings::OpenSelect) {
        self.settings_state.open_select = which;
    }
}

// ---------- view helpers ----------

const CHAT_SIDEBAR_HANDLE: &str = "chat-sidebar:resize";

fn top_toolbar(state: &State) -> El {
    let connected = matches!(state.connection, ConnectionState::Connected { .. });

    let status = match &state.connection {
        ConnectionState::Disconnected => badge("Disconnected").muted(),
        ConnectionState::Connecting { server_addr } => badge(format!("Connecting to {server_addr}…")).warning(),
        ConnectionState::Connected { server_name, .. } => badge(server_name.clone()).success(),
        ConnectionState::ConnectionLost { error } => badge(format!("Connection lost: {error}")).destructive(),
        ConnectionState::CertificatePending { cert_info } => {
            badge(format!("Cert pending: {}", cert_info.server_name)).warning()
        }
    };

    // Mute / deafen indicators.
    let mute_label = if state.audio.self_muted { "Muted" } else { "Mic" };
    let deafen_label = if state.audio.self_deafened { "Deafened" } else { "Sound" };
    let voice_mode_label = match state.audio.voice_mode {
        VoiceMode::PushToTalk => "PTT",
        VoiceMode::Continuous => "Continuous",
    };

    let mut children: Vec<El> = vec![text("Rumble").title(), status, spacer()];

    if connected {
        let mute_btn = button(mute_label).key("toolbar:mute");
        let mute_btn = if state.audio.self_muted {
            mute_btn.text_color(palette::MUTED_SELF)
        } else {
            mute_btn.text_color(palette::TALKING)
        };
        children.push(mute_btn);

        let deafen_btn = button(deafen_label).key("toolbar:deafen");
        let deafen_btn = if state.audio.self_deafened {
            deafen_btn.text_color(palette::MUTED_SELF)
        } else {
            deafen_btn
        };
        children.push(deafen_btn);

        children.push(button(voice_mode_label).key("toolbar:voice-mode").ghost());
        children.push(button("Disconnect").key("toolbar:disconnect").secondary());
    }
    // When disconnected, the center area renders the saved-server picker
    // (with its own "Add server…" / Connect / Edit / Remove controls), so
    // we don't add a redundant toolbar-level connect entry point here.

    children.push(button("Identity").key("toolbar:identity").ghost());
    children.push(button("Settings").key("toolbar:settings").ghost());

    row(children)
        .gap(tokens::SPACE_SM)
        .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
        .height(Size::Fixed(56.0))
        .width(Size::Fill(1.0))
        .fill(tokens::BG_RAISED)
        .align(Align::Center)
}

fn chat_sidebar(state: &State, chat_input: &str, selection: &Selection, width: f32) -> El {
    let messages: Vec<El> = if state.chat_messages.is_empty() {
        vec![
            // wrap_text() so the longer placeholder fits inside narrow
            // sidebar widths (~256 px = SIDEBAR_WIDTH minus padding) —
            // without it, "Connect to a server to start chatting"
            // overflows by a few pixels on the default sidebar.
            text(if matches!(state.connection, ConnectionState::Connected { .. }) {
                "No messages yet"
            } else {
                "Connect to a server to start chatting"
            })
            .muted()
            .wrap_text(),
        ]
    } else {
        state.chat_messages.iter().map(render_chat_line).collect()
    };

    column([
        text("Chat")
            .title()
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM)),
        divider(),
        scroll(messages)
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
            .gap(tokens::SPACE_XS)
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0)),
        divider(),
        text_input(chat_input, selection, "chat:input")
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
            .width(Size::Fill(1.0)),
    ])
    .width(Size::Fixed(width))
    .height(Size::Fill(1.0))
    .fill(tokens::BG_CARD)
}

fn render_chat_line(msg: &rumble_protocol::ChatMessage) -> El {
    use rumble_protocol::ChatMessageKind;

    let prefix = if msg.is_local {
        msg.text.clone()
    } else {
        match &msg.kind {
            ChatMessageKind::Room => format!("{}: {}", msg.sender, msg.text),
            ChatMessageKind::DirectMessage { .. } => {
                format!("[DM] {}: {}", msg.sender, msg.text)
            }
            ChatMessageKind::Tree => format!("[Tree] {}: {}", msg.sender, msg.text),
        }
    };

    let line = paragraph(prefix);
    let line = if msg.is_local {
        line.text_color(palette::CHAT_SYS)
    } else {
        match &msg.kind {
            ChatMessageKind::Room => line,
            ChatMessageKind::DirectMessage { .. } => line.text_color(palette::CHAT_DM),
            ChatMessageKind::Tree => line.text_color(palette::CHAT_TREE),
        }
    };
    line.font_size(tokens::FONT_SM)
}

fn center_area(state: &State, recent_servers: &[RecentServer]) -> El {
    if matches!(state.connection, ConnectionState::Connected { .. }) {
        rooms_view(state)
    } else {
        servers_view(state, recent_servers)
    }
}

fn servers_view(state: &State, recent_servers: &[RecentServer]) -> El {
    // Connection-status banner above the list. Most disconnected
    // states have nothing to say (Disconnected idle); Connecting /
    // ConnectionLost / CertPending get a one-line status so the
    // center area still communicates progress while the list keeps
    // taking up the same space.
    let status_banner: Option<El> = match &state.connection {
        ConnectionState::Disconnected => None,
        ConnectionState::Connecting { server_addr } => Some(text(format!("Connecting to {server_addr}...")).muted()),
        ConnectionState::ConnectionLost { error } => Some(
            text(format!("Connection lost: {error}"))
                .text_color(tokens::DESTRUCTIVE)
                .wrap_text(),
        ),
        ConnectionState::CertificatePending { cert_info } => Some(
            text(format!("Certificate pending for {}", cert_info.server_name))
                .text_color(tokens::WARNING)
                .wrap_text(),
        ),
        ConnectionState::Connected { .. } => unreachable!(),
    };

    let header = row([
        text("Servers").title(),
        spacer(),
        button("Add server…").key("server:add").primary(),
    ])
    .gap(tokens::SPACE_SM)
    .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
    .align(Align::Center)
    .width(Size::Fill(1.0));

    // Render the list sorted by last_used desc; the row's stable
    // identifier remains the unsorted index (`server:connect:{idx}`),
    // so settings.modify by index continues to point at the right row.
    let body: El = if recent_servers.is_empty() {
        column([
            text("No saved servers yet.").muted(),
            paragraph("Click \"Add server…\" to bookmark one.")
                .muted()
                .font_size(tokens::FONT_SM),
        ])
        .gap(tokens::SPACE_SM)
        .align(Align::Center)
        .justify(Justify::Center)
        .padding(tokens::SPACE_XL)
        .width(Size::Fill(1.0))
        .height(Size::Fill(1.0))
    } else {
        let mut order: Vec<usize> = (0..recent_servers.len()).collect();
        order.sort_by(|&a, &b| recent_servers[b].last_used_unix.cmp(&recent_servers[a].last_used_unix));
        let rows: Vec<El> = order
            .into_iter()
            .map(|idx| server_row(idx, &recent_servers[idx]))
            .collect();
        scroll(rows)
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
            .gap(tokens::SPACE_SM)
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0))
    };

    let mut children: Vec<El> = vec![header, divider()];
    if let Some(banner) = status_banner {
        children.push(
            row([banner])
                .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
                .width(Size::Fill(1.0)),
        );
        children.push(divider());
    }
    children.push(body);

    column(children).width(Size::Fill(1.0)).height(Size::Fill(1.0))
}

fn server_row(idx: usize, server: &RecentServer) -> El {
    let title = if server.label.is_empty() {
        server.addr.clone()
    } else {
        server.label.clone()
    };

    // Subtitle holds whichever of (addr, username, last-used) we still
    // have something to show: addr only repeats when label was the
    // primary title; username always shows; last-used is omitted when
    // the bookmark has never been connected to (last_used == 0).
    let mut subtitle_parts: Vec<String> = Vec::new();
    if !server.label.is_empty() {
        subtitle_parts.push(server.addr.clone());
    }
    if !server.username.is_empty() {
        subtitle_parts.push(server.username.clone());
    }
    let rel = relative_time(server.last_used_unix);
    if !rel.is_empty() {
        subtitle_parts.push(rel);
    }
    let subtitle = subtitle_parts.join(" · ");

    let info = column([
        text(title).font_weight(FontWeight::Semibold),
        text(subtitle).muted().font_size(tokens::FONT_SM),
    ])
    .gap(tokens::SPACE_XS)
    .width(Size::Fill(1.0));

    row([
        info,
        button("Connect").key(format!("server:connect:{idx}")).primary(),
        button("Edit").key(format!("server:edit:{idx}")).ghost(),
        button("Remove").key(format!("server:delete:{idx}")).ghost(),
    ])
    .gap(tokens::SPACE_SM)
    .padding(Sides::all(tokens::SPACE_MD))
    .fill(tokens::BG_RAISED)
    .stroke(tokens::BORDER)
    .radius(tokens::RADIUS_MD)
    .align(Align::Center)
    .width(Size::Fill(1.0))
}

fn rooms_view(state: &State) -> El {
    let mut entries: Vec<El> = Vec::new();
    for &root_id in &state.room_tree.roots {
        push_room_subtree(state, root_id, 0, &mut entries);
    }

    if entries.is_empty() {
        entries.push(text("No rooms received yet.").muted());
    }

    column([
        text("Rooms")
            .title()
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM)),
        divider(),
        scroll(entries)
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_MD))
            .gap(tokens::SPACE_XS)
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0)),
    ])
    .width(Size::Fill(1.0))
    .height(Size::Fill(1.0))
}

fn push_room_subtree(state: &State, room_id: uuid::Uuid, depth: usize, out: &mut Vec<El>) {
    use rumble_protocol::uuid_from_room_id;

    let Some(node) = state.room_tree.nodes.get(&room_id) else {
        return;
    };

    let user_count = state
        .users
        .iter()
        .filter(|u| {
            u.current_room
                .as_ref()
                .and_then(uuid_from_room_id)
                .is_some_and(|id| id == room_id)
        })
        .count();
    let is_current = state.my_room_id == Some(room_id);

    let indent = depth as f32 * tokens::SPACE_LG;
    let suffix = if user_count > 0 {
        format!("  ({user_count})")
    } else {
        String::new()
    };
    let mut label = text(format!("{}{}", node.name, suffix)).font_size(tokens::FONT_BASE);
    if user_count == 0 {
        label = label.muted();
    }
    if is_current {
        label = label.text_color(tokens::PRIMARY);
    }

    out.push(
        row([
            spacer().width(Size::Fixed(indent)),
            icon(IconName::Folder).text_color(if is_current {
                tokens::PRIMARY
            } else {
                tokens::TEXT_MUTED_FOREGROUND
            }),
            label,
        ])
        .key(room_route_key(room_id))
        .gap(tokens::SPACE_SM)
        .align(Align::Center)
        .padding(Sides::xy(tokens::SPACE_XS, tokens::SPACE_XS))
        .focusable(),
    );

    for user in state.users.iter().filter(|u| {
        u.current_room
            .as_ref()
            .and_then(uuid_from_room_id)
            .is_some_and(|id| id == room_id)
    }) {
        let user_id = user.user_id.as_ref().map(|u| u.value).unwrap_or(0);
        // Mic-state glyph picks up its color from the bundled Mumble
        // SVG itself (red self-mute, blue server-mute, blue talking,
        // green idle), so no `.text_color(...)` override here.
        let mic_icon: SvgIcon = if user.server_muted {
            SVG_MUTED_SERVER.clone()
        } else if user.is_muted {
            SVG_MUTED_SELF.clone()
        } else if state.audio.talking_users.contains(&user_id) {
            SVG_TALKING_ON.clone()
        } else {
            SVG_TALKING_OFF.clone()
        };

        let mut name_el = text(user.username.clone()).font_size(tokens::FONT_SM);
        if user.is_elevated {
            name_el = name_el.text_color(palette::ELEVATED);
        }

        out.push(
            row([
                spacer().width(Size::Fixed(indent + tokens::SPACE_LG)),
                icon(mic_icon).icon_size(12.0),
                name_el,
            ])
            .gap(tokens::SPACE_SM)
            .align(Align::Center)
            .padding(Sides::xy(tokens::SPACE_XS, 2.0)),
        );
    }

    for &child in &node.children {
        push_room_subtree(state, child, depth + 1, out);
    }
}

fn room_route_key(id: uuid::Uuid) -> String {
    format!("room:{}", id)
}

fn parse_room_route_key(key: &str) -> Option<uuid::Uuid> {
    key.strip_prefix("room:").and_then(|s| uuid::Uuid::parse_str(s).ok())
}

fn cert_modal(cert_info: &PendingCertificate) -> El {
    modal(
        "cert",
        "Untrusted certificate",
        [
            paragraph("The server presented a self-signed or unknown certificate.").text_color(tokens::WARNING),
            row([
                text("Server:").muted(),
                text(cert_info.server_addr.clone()).font_weight(FontWeight::Semibold),
            ])
            .gap(tokens::SPACE_SM)
            .align(Align::Center),
            row([
                text("Certificate for:").muted(),
                text(cert_info.server_name.clone()).font_weight(FontWeight::Semibold),
            ])
            .gap(tokens::SPACE_SM)
            .align(Align::Center),
            text("Fingerprint (SHA-256)").muted(),
            // SHA-256 hex with colon separators is 79 chars wide —
            // wrap_text() so it flows across two lines instead of
            // overflowing the modal. The user needs to read the full
            // hash, so .ellipsis() would be wrong here.
            mono(cert_info.fingerprint_hex()).font_size(tokens::FONT_SM).wrap_text(),
            paragraph(
                "Only accept if this fingerprint matches what the server administrator gave you. Once accepted, the \
                 certificate is saved for future connections.",
            )
            .muted()
            .font_size(tokens::FONT_SM),
            row([
                button("Reject").key("cert:reject"),
                spacer(),
                button("Trust and connect").key("cert:accept").primary(),
            ])
            .gap(tokens::SPACE_SM)
            .width(Size::Fill(1.0))
            .align(Align::Center),
        ],
    )
}

fn identity_modal(identity: &Identity) -> El {
    use rumble_desktop_shell::KeySource;

    let fingerprint = identity.fingerprint();
    let (source_label, detail) = match identity.manager().config().map(|c| &c.source) {
        Some(KeySource::LocalPlaintext { .. }) => (
            "Local key (plaintext)",
            "Stored unencrypted at identity.json — fine for personal machines.".to_string(),
        ),
        Some(KeySource::LocalEncrypted { .. }) => (
            "Local key (encrypted)",
            "Encrypted with Argon2 + ChaCha20-Poly1305. Password required at startup.".to_string(),
        ),
        Some(KeySource::SshAgent {
            fingerprint: agent_fp,
            comment,
        }) => {
            let line = if comment.is_empty() {
                format!("ssh-agent fingerprint: {agent_fp}")
            } else {
                format!("ssh-agent: {comment} ({agent_fp})")
            };
            ("SSH agent", line)
        }
        None => ("Not configured", "Run the identity wizard to set this up.".to_string()),
    };
    let path = identity.manager().config_dir().join("identity.json");

    modal(
        "identity",
        "Rumble identity",
        [
            text("Fingerprint (SHA-256)").muted(),
            mono(fingerprint).font_size(tokens::FONT_SM).wrap_text(),
            divider(),
            text("Storage").muted(),
            text(source_label.to_string()).font_weight(FontWeight::Semibold),
            paragraph(detail).muted().font_size(tokens::FONT_SM),
            text("On disk").muted(),
            mono(path.display().to_string()).font_size(tokens::FONT_SM).wrap_text(),
            divider(),
            paragraph(
                "Generating a new identity overwrites identity.json. Servers that knew the old key won't recognise \
                 the new one — you'll have to re-register or be re-approved.",
            )
            .text_color(tokens::WARNING)
            .font_size(tokens::FONT_SM),
            row([
                button("Close").key("identity:close"),
                spacer(),
                button("Generate new identity…")
                    .key("identity:regenerate")
                    .destructive(),
            ])
            .gap(tokens::SPACE_SM)
            .width(Size::Fill(1.0))
            .align(Align::Center),
        ],
    )
}

fn server_form_modal(form: &ServerForm, selection: &Selection) -> El {
    let title = if form.editing_index.is_some() {
        "Edit server"
    } else {
        "Add server"
    };

    let mut body: Vec<El> = vec![
        text("Address").muted(),
        text_input(&form.addr, selection, "server_form:addr"),
        text("Label (optional)").muted(),
        text_input(&form.label, selection, "server_form:label"),
        text("Username").muted(),
        text_input(&form.username, selection, "server_form:user"),
    ];

    if let Some(err) = &form.error {
        body.push(
            paragraph(err.clone())
                .text_color(tokens::DESTRUCTIVE)
                .font_size(tokens::FONT_SM),
        );
    }

    body.push(
        row([
            button("Cancel").key("server_form:cancel"),
            spacer(),
            button("Save").key("server_form:save"),
            button("Save & Connect").key("server_form:connect").primary(),
        ])
        .gap(tokens::SPACE_SM)
        .width(Size::Fill(1.0))
        .align(Align::Center),
    );

    modal("server_form", title, body)
}
