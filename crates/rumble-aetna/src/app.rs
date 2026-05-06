//! Top-level aetna `App` for the Rumble client.
//!
//! Owns local UI state (connect form fields, modal flags, selected
//! room) and projects `(state, ui_state) -> El` on every frame.

use std::sync::LazyLock;

use aetna_core::prelude::*;

use rumble_desktop_shell::{AcceptedCertificate, SettingsStore};
use rumble_protocol::{Command, ConnectionState, PendingCertificate, State, VoiceMode};

use crate::{
    backend::UiBackend,
    theme::{self as palette},
};

// ---- Bundled Mumble SVG glyphs ----
//
// Parsed once at first use (Arc-bumped on every `icon(...)` call) and
// shared across frames. We use `SvgIcon::parse` (not
// `parse_current_color`) because the Mumble theme bakes its semantic
// colors into the SVG paint — red for self-mute, blue for talking, etc.
// — and those colors are exactly the visual signal we want to keep.

static SVG_TALKING_ON: LazyLock<SvgIcon> = LazyLock::new(|| {
    SvgIcon::parse(include_str!("../assets/icons/talking_on.svg"))
        .expect("talking_on.svg parses")
});
static SVG_TALKING_OFF: LazyLock<SvgIcon> = LazyLock::new(|| {
    SvgIcon::parse(include_str!("../assets/icons/talking_off.svg"))
        .expect("talking_off.svg parses")
});
static SVG_MUTED_SELF: LazyLock<SvgIcon> = LazyLock::new(|| {
    SvgIcon::parse(include_str!("../assets/icons/muted_self.svg"))
        .expect("muted_self.svg parses")
});
static SVG_MUTED_SERVER: LazyLock<SvgIcon> = LazyLock::new(|| {
    SvgIcon::parse(include_str!("../assets/icons/muted_server.svg"))
        .expect("muted_server.svg parses")
});

/// Local-only first-run identity wrapper.
///
/// We share the on-disk `identity.json` with `rumble-egui` /
/// `rumble-next` so a user with an existing key can launch this client
/// without redoing first-run.
pub struct Identity {
    manager: rumble_desktop_shell::KeyManager,
}

impl Identity {
    pub fn load(config_dir: std::path::PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        Ok(Self {
            manager: rumble_desktop_shell::KeyManager::new(config_dir),
        })
    }

    pub fn public_key(&self) -> Option<[u8; 32]> {
        self.manager.public_key_bytes()
    }

    pub fn signer(&self) -> rumble_client::SigningCallback {
        match self.manager.create_signer() {
            Some(s) => s,
            None => std::sync::Arc::new(|_payload: &[u8]| Err("identity locked or unsupported".to_string())),
        }
    }

    pub fn needs_setup(&self) -> bool {
        self.manager.needs_setup()
    }

    pub fn manager_mut(&mut self) -> &mut rumble_desktop_shell::KeyManager {
        &mut self.manager
    }
}

pub struct RumbleApp<B: UiBackend = crate::backend::NativeUiBackend> {
    backend: B,
    identity: Identity,
    settings: SettingsStore,

    // ---- Local UI state ----
    connect_modal_open: bool,
    address: String,
    address_sel: TextSelection,
    username: String,
    username_sel: TextSelection,

    chat_input: String,
    chat_sel: TextSelection,

    /// Chat sidebar width in logical pixels — adjusted by dragging
    /// the divider on its right edge. Initialized from
    /// [`tokens::SIDEBAR_WIDTH`] (the conventional ~256px starting
    /// point) and clamped to [`tokens::SIDEBAR_WIDTH_MIN`] /
    /// `_MAX` by the resize handler.
    chat_sidebar_w: f32,
    chat_sidebar_drag: ResizeDrag,
}

impl<B: UiBackend> RumbleApp<B> {
    pub fn new(backend: B, identity: Identity, settings: SettingsStore) -> Self {
        Self {
            backend,
            identity,
            settings,
            connect_modal_open: false,
            address: "127.0.0.1:5000".to_string(),
            address_sel: TextSelection::default(),
            username: default_username(),
            username_sel: TextSelection::default(),
            chat_input: String::new(),
            chat_sel: TextSelection::default(),
            chat_sidebar_w: tokens::SIDEBAR_WIDTH,
            chat_sidebar_drag: ResizeDrag::default(),
        }
    }
}

fn default_username() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "rumble-user".to_string())
}

impl<B: UiBackend> App for RumbleApp<B> {
    fn build(&self) -> El {
        let state = self.backend.state();

        let main = column([
            top_toolbar(&state),
            row([
                chat_sidebar(&state, &self.chat_input, self.chat_sel, self.chat_sidebar_w),
                resize_handle(Axis::Row).key(CHAT_SIDEBAR_HANDLE),
                center_area(&state, self.connect_modal_open),
            ])
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0))
            .align(Align::Stretch),
        ])
        .fill_size()
        .align(Align::Stretch);

        let cert_layer = if let ConnectionState::CertificatePending { cert_info } = &state.connection {
            Some(cert_modal(cert_info))
        } else {
            None
        };
        // Suppress the connect modal whenever a cert prompt is up — the
        // user can't usefully edit address/username while the backend
        // is waiting for an approval decision on the previous attempt.
        let connect_layer = if self.connect_modal_open && cert_layer.is_none() {
            Some(connect_modal(
                &self.address,
                self.address_sel,
                &self.username,
                self.username_sel,
            ))
        } else {
            None
        };

        if connect_layer.is_some() || cert_layer.is_some() {
            overlays(main, [connect_layer, cert_layer])
        } else {
            main
        }
    }

    fn on_event(&mut self, event: UiEvent) {
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

        // Connect modal lifecycle.
        if event.is_click_or_activate("connect:open") {
            self.connect_modal_open = true;
            return;
        }
        if event.is_click_or_activate("connect:cancel")
            || event.is_route("connect:dismiss") && event.kind == UiEventKind::Click
            || (self.connect_modal_open && event.kind == UiEventKind::Escape)
        {
            self.connect_modal_open = false;
            return;
        }
        if event.is_click_or_activate("connect:submit") {
            self.connect_modal_open = false;
            self.do_connect();
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

        // Connect-form text inputs.
        if event.target_key() == Some("connect:addr") {
            text_input::apply_event(&mut self.address, &mut self.address_sel, &event);
            return;
        }
        if event.target_key() == Some("connect:user") {
            text_input::apply_event(&mut self.username, &mut self.username_sel, &event);
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
                    self.chat_sel = TextSelection::default();
                }
                return;
            }
            text_input::apply_event(&mut self.chat_input, &mut self.chat_sel, &event);
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

    fn do_connect(&mut self) {
        let Some(public_key) = self.identity.public_key() else {
            tracing::warn!("rumble-aetna: no identity key — first-run wizard not implemented yet");
            self.backend.send(Command::LocalMessage {
                text: "No identity key configured. Run rumble-egui or rumble-next once to generate one.".to_string(),
            });
            return;
        };
        let signer = self.identity.signer();
        let addr = if self.address.trim().is_empty() {
            "127.0.0.1:5000".to_string()
        } else {
            self.address.trim().to_string()
        };
        self.backend.send(Command::LocalMessage {
            text: format!("Connecting to {addr}..."),
        });
        self.backend.send(Command::Connect {
            addr,
            name: self.username.clone(),
            public_key,
            signer,
            password: None,
        });
    }
}

// ---------- view helpers ----------

const CHAT_SIDEBAR_HANDLE: &str = "chat-sidebar:resize";

fn top_toolbar(state: &State) -> El {
    let connected = matches!(state.connection, ConnectionState::Connected { .. });

    let status = match &state.connection {
        ConnectionState::Disconnected => badge("Disconnected").muted(),
        ConnectionState::Connecting { server_addr } => {
            badge(format!("Connecting to {server_addr}…")).warning()
        }
        ConnectionState::Connected { server_name, .. } => badge(server_name.clone()).success(),
        ConnectionState::ConnectionLost { error } => {
            badge(format!("Connection lost: {error}")).destructive()
        }
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
    } else {
        children.push(button("Connect…").key("connect:open").primary());
    }

    row(children)
        .gap(tokens::SPACE_SM)
        .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
        .height(Size::Fixed(56.0))
        .width(Size::Fill(1.0))
        .fill(tokens::BG_RAISED)
        .align(Align::Center)
}

fn chat_sidebar(state: &State, chat_input: &str, chat_sel: TextSelection, width: f32) -> El {
    let messages: Vec<El> = if state.chat_messages.is_empty() {
        vec![
            text(if matches!(state.connection, ConnectionState::Connected { .. }) {
                "No messages yet"
            } else {
                "Connect to a server to start chatting"
            })
            .muted(),
        ]
    } else {
        state.chat_messages.iter().map(render_chat_line).collect()
    };

    column([
        text("Chat").title().padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM)),
        divider(),
        scroll(messages)
            .padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM))
            .gap(tokens::SPACE_XS)
            .width(Size::Fill(1.0))
            .height(Size::Fill(1.0)),
        divider(),
        text_input(chat_input, chat_sel)
            .key("chat:input")
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

fn center_area(state: &State, connect_modal_open: bool) -> El {
    let _ = connect_modal_open;
    if matches!(state.connection, ConnectionState::Connected { .. }) {
        rooms_view(state)
    } else {
        disconnected_view(state)
    }
}

fn disconnected_view(state: &State) -> El {
    let body = match &state.connection {
        ConnectionState::Disconnected => text("Not connected.").muted(),
        ConnectionState::Connecting { server_addr } => text(format!("Connecting to {server_addr}...")).muted(),
        ConnectionState::ConnectionLost { error } => {
            text(format!("Connection lost: {error}")).text_color(tokens::DESTRUCTIVE)
        }
        ConnectionState::CertificatePending { cert_info } => {
            text(format!("Certificate pending for {}", cert_info.server_name)).text_color(tokens::WARNING)
        }
        ConnectionState::Connected { .. } => unreachable!(),
    };

    column([
        h2("Welcome to Rumble"),
        body,
        button("Connect…").key("connect:open").primary(),
    ])
    .gap(tokens::SPACE_LG)
    .padding(tokens::SPACE_XL)
    .align(Align::Center)
    .justify(Justify::Center)
    .width(Size::Fill(1.0))
    .height(Size::Fill(1.0))
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
        text("Rooms").title().padding(Sides::xy(tokens::SPACE_LG, tokens::SPACE_SM)),
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
            mono(cert_info.fingerprint_hex()).font_size(tokens::FONT_SM),
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

fn connect_modal(address: &str, address_sel: TextSelection, username: &str, username_sel: TextSelection) -> El {
    modal(
        "connect",
        "Connect to a Rumble server",
        [
            text("Address").muted(),
            text_input(address, address_sel).key("connect:addr"),
            text("Username").muted(),
            text_input(username, username_sel).key("connect:user"),
            row([
                button("Cancel").key("connect:cancel"),
                spacer(),
                button("Connect").key("connect:submit").primary(),
            ])
            .gap(tokens::SPACE_SM)
            .width(Size::Fill(1.0))
            .align(Align::Center),
        ],
    )
}
