//! Shell = the pieces of the Rumble UI that are the same across every
//! paradigm (tree, chat, composer, self-state toggles). Paradigm-specific
//! chrome (top bar / toolbar / statusbar) is layered on top in
//! `paradigm::*`.
//!
//! `Shell` now reads from a live `State` snapshot and issues `Command`s
//! via a `BackendHandle`. Per-frame flow: the app clones `State` once,
//! converts it via `crate::adapters`, and hands `(state, tree_nodes)` to
//! the shell's render functions.

use std::collections::HashMap;

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, Pos2, RichText, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, Vec2, epaint::RectShape,
};
use rumble_client::handle::BackendHandle;
use rumble_client_traits::Platform;
use rumble_protocol::{Command, State};
use rumble_widgets::{
    ButtonArgs, GroupBox, PressableRole, SurfaceFrame, SurfaceKind, TextInput, Tree, TreeNode, TreeNodeId, UiExt,
};
use uuid::Uuid;

use crate::{
    adapters::{self, NodeRef, crumbs_for_room, is_room},
    data::{ChatEntry, ChatMsg, Media, SysMsg, SysTone},
};

/// What the right-click context menu is attached to.
#[derive(Clone, Debug)]
pub enum ContextTarget {
    User { id: u64, name: String, locally_muted: bool },
    Room { id: Uuid, name: String },
}

#[derive(Clone, Debug)]
pub struct TreeContext {
    target: ContextTarget,
    pos: Pos2,
}

/// A modal that needs a text input before dispatching. Only one can be
/// open at a time — opening a new one closes the previous.
#[derive(Clone, Debug)]
pub enum PendingModal {
    CreateRoom {
        parent: Option<Uuid>,
        parent_name: String,
        name: String,
    },
    RenameRoom {
        id: Uuid,
        original: String,
        name: String,
    },
    DeleteRoom {
        id: Uuid,
        name: String,
    },
    Ban {
        user_id: u64,
        username: String,
        reason: String,
    },
    DirectMessage {
        user_id: u64,
        username: String,
        text: String,
    },
}

/// UI-local state that isn't part of the backend's `State` — caret
/// positions, composer buffer, expanded/collapsed channel set, etc.
#[derive(Default)]
pub struct Shell {
    /// Persistent expansion state per channel `TreeNodeId`. Rebuilt
    /// tree copies start expanded; this map flips them. Present so the
    /// tree doesn't snap back when `build_tree` re-runs each frame.
    expanded: HashMap<TreeNodeId, bool>,
    /// Selected row — used for highlight. Falls back to `my_room_id`
    /// if the user hasn't clicked anything.
    pub selected: Option<TreeNodeId>,
    pub composer: String,
    /// Right-click menu anchored at this position. Cleared when the
    /// user picks something or clicks away.
    context_menu: Option<TreeContext>,
    /// Modal awaiting text input. Only one at a time.
    modal: Option<PendingModal>,
    /// Settings panel visibility (rendered as an overlay).
    pub settings_open: bool,
    /// Active chat timestamp format. `None` = hide timestamps. Set by
    /// `App::update` from `settings.chat` each frame, so toggling the
    /// preference reflects without restarting.
    pub chat_timestamp_format: Option<rumble_desktop_shell::TimestampFormat>,
}

// ---------- Tree pane ----------

impl Shell {
    /// Render the nested channel/user tree. Emits `Command::JoinRoom`
    /// when a channel is double-clicked or activated via Enter.
    pub fn tree_pane<P: Platform + 'static>(&mut self, ui: &mut Ui, state: &State, backend: &BackendHandle<P>) {
        let (mut tree, id_map) = adapters::build_tree(state);

        // Apply our local expansion overrides. (Live tree nodes start
        // expanded; the user's preference overrides that.)
        apply_expanded(&mut tree, &self.expanded);

        // Default selection = our current room, if user hasn't picked.
        let selected = self.selected.or_else(|| state.my_room_id.map(adapters::room_node_id));

        ScrollArea::vertical().id_salt("rumble_next_tree").show(ui, |ui| {
            let resp = Tree::new("rumble_next_tree", &tree).selected(selected).show(ui);

            if let Some(id) = resp.toggled {
                let current = lookup_expanded(&tree, id).unwrap_or(true);
                self.expanded.insert(id, !current);
            }
            if let Some(id) = resp.clicked {
                self.selected = Some(id);
            }
            if let Some(new_sel) = resp.selection_changed {
                self.selected = new_sel;
            }
            if let Some(id) = resp.double_clicked.or(resp.activated)
                && is_room(id)
                && let Some(NodeRef::Room(uuid)) = id_map.get(&id)
            {
                backend.send(Command::JoinRoom { room_id: *uuid });
            }
            if let Some((id, pos)) = resp.context
                && let Some(node_ref) = id_map.get(&id)
            {
                let target = match node_ref {
                    NodeRef::User(uid) => {
                        let name = state
                            .get_user(*uid)
                            .map(|u| u.username.clone())
                            .unwrap_or_else(|| format!("user #{uid}"));
                        ContextTarget::User {
                            id: *uid,
                            name,
                            locally_muted: state.audio.is_user_muted(*uid),
                        }
                    }
                    NodeRef::Room(rid) => {
                        let name = state
                            .room_tree
                            .get(*rid)
                            .map(|n| n.name.clone())
                            .unwrap_or_else(|| "(room)".into());
                        ContextTarget::Room { id: *rid, name }
                    }
                };
                self.context_menu = Some(TreeContext { target, pos });
            }
        });
    }
}

fn apply_expanded(nodes: &mut [TreeNode], overrides: &HashMap<TreeNodeId, bool>) {
    for n in nodes {
        if let Some(v) = overrides.get(&n.id) {
            n.expanded = *v;
        }
        apply_expanded(&mut n.children, overrides);
    }
}

fn lookup_expanded(nodes: &[TreeNode], id: TreeNodeId) -> Option<bool> {
    for n in nodes {
        if n.id == id {
            return Some(n.expanded);
        }
        if let Some(r) = lookup_expanded(&n.children, id) {
            return Some(r);
        }
    }
    None
}

// ---------- Room header (breadcrumbs) ----------

pub fn room_header(ui: &mut Ui, state: &State) {
    let theme = ui.theme();
    let tokens = theme.tokens().clone();

    let (crumbs, peers) = match state.my_room_id {
        Some(id) => (crumbs_for_room(state, id), adapters::peers_in_current_room(state)),
        None => (vec!["— not in a room —".to_string()], 0),
    };

    SurfaceFrame::new(SurfaceKind::Toolbar)
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let muted = tokens.text_muted;
                let faint = tokens.line_soft;
                let mono = tokens.font_mono.clone();

                let (last, head) = crumbs
                    .split_last()
                    .map(|(l, h)| (l.as_str(), h))
                    .unwrap_or(("(no room)", &[]));
                for c in head {
                    ui.label(RichText::new(c).color(muted).font(mono.clone()));
                    ui.label(RichText::new("/").color(faint).font(mono.clone()));
                }
                ui.label(
                    RichText::new(last)
                        .color(tokens.text)
                        .strong()
                        .font(tokens.font_body.clone()),
                );

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let summary = format!("● {peers} connected · text: ephemeral (last 50)");
                    ui.label(RichText::new(summary).color(muted).font(mono));
                });
            });
        });
}

// ---------- Chat stream ----------

const AVATAR_SIZE: f32 = 24.0;

impl Shell {
    pub fn chat_stream(&mut self, ui: &mut Ui, state: &State) {
        let theme = ui.theme();
        let tokens = theme.tokens().clone();

        let entries = adapters::chat_entries(state, self.chat_timestamp_format);

        ScrollArea::vertical()
            .id_salt("rumble_next_chat")
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.style_mut().spacing.item_spacing.y = 8.0;
                ui.add_space(8.0);
                if entries.is_empty() {
                    ui.label(
                        RichText::new("No chat messages yet. Say hello.")
                            .color(tokens.text_muted)
                            .italics(),
                    );
                }
                for entry in &entries {
                    match entry {
                        ChatEntry::Sys(m) => draw_sys(ui, &tokens, m),
                        ChatEntry::Msg(m) => draw_msg(ui, &tokens, m),
                    }
                }
                ui.add_space(10.0);
            });
    }
}

fn draw_sys(ui: &mut Ui, tokens: &rumble_widgets::Tokens, m: &SysMsg) {
    let dot_color = match m.tone {
        SysTone::Join => Color32::from_rgb(0x2f, 0x85, 0x5a),
        SysTone::Disc => tokens.danger,
        SysTone::Info => tokens.line_soft,
    };
    ui.horizontal(|ui| {
        ui.add_space(34.0);
        let mono = tokens.font_mono.clone();
        ui.label(RichText::new(&m.t).color(tokens.line_soft).font(mono.clone()));
        ui.label(RichText::new("●").color(dot_color).font(mono.clone()));
        ui.label(RichText::new(&m.text).color(tokens.text_muted).font(mono));
    });
}

fn draw_msg(ui: &mut Ui, tokens: &rumble_widgets::Tokens, m: &ChatMsg) {
    ui.horizontal_top(|ui| {
        let (avatar_rect, _) = ui.allocate_exact_size(Vec2::splat(AVATAR_SIZE), Sense::hover());
        let initial = m
            .who
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .unwrap_or('?')
            .to_string();
        ui.painter().add(RectShape::new(
            avatar_rect,
            CornerRadius::same(3),
            tokens.line_soft,
            Stroke::NONE,
            StrokeKind::Inside,
        ));
        ui.painter().text(
            avatar_rect.center(),
            Align2::CENTER_CENTER,
            initial,
            FontId::new(10.0, tokens.font_body.family.clone()),
            Color32::WHITE,
        );

        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(&m.who)
                        .color(tokens.text)
                        .strong()
                        .font(tokens.font_body.clone()),
                );
                ui.label(
                    RichText::new(&m.t)
                        .color(tokens.text_muted)
                        .font(tokens.font_mono.clone()),
                );
            });
            if let Some(body) = &m.body {
                ui.label(RichText::new(body).color(tokens.text).font(tokens.font_body.clone()));
            }
            if let Some(media) = &m.media {
                draw_media(ui, tokens, media);
            }
        });
    });
}

fn draw_media(ui: &mut Ui, tokens: &rumble_widgets::Tokens, media: &Media) {
    match media {
        Media::Image { name, size } => {
            ui.add_space(2.0);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(220.0, 140.0), Sense::hover());
            ui.painter().add(RectShape::new(
                rect,
                CornerRadius::same(4),
                Color32::from_rgb(0xef, 0xef, 0xec),
                Stroke::new(1.0, tokens.line_soft),
                StrokeKind::Inside,
            ));
            let p = ui.painter();
            let stripe = Color32::from_rgb(0xe6, 0xe6, 0xe2);
            let step = 20.0;
            let mut x = rect.left() - rect.height();
            while x < rect.right() {
                p.line_segment(
                    [egui::pos2(x, rect.top()), egui::pos2(x + rect.height(), rect.bottom())],
                    Stroke::new(6.0, stripe),
                );
                x += step;
            }
            p.text(
                rect.left_bottom() + Vec2::new(8.0, -8.0),
                Align2::LEFT_BOTTOM,
                format!("[img] {name} · {size}"),
                tokens.font_mono.clone(),
                tokens.text_muted,
            );
        }
        Media::File { ext, name, size } => {
            ui.add_space(2.0);
            SurfaceFrame::new(SurfaceKind::Group)
                .inner_margin(Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (ico, _) = ui.allocate_exact_size(Vec2::new(36.0, 40.0), Sense::hover());
                        ui.painter().add(RectShape::new(
                            ico,
                            CornerRadius::same(2),
                            tokens.surface,
                            Stroke::new(1.0, tokens.line_soft),
                            StrokeKind::Inside,
                        ));
                        ui.painter().text(
                            ico.center(),
                            Align2::CENTER_CENTER,
                            ext.as_str(),
                            tokens.font_mono.clone(),
                            tokens.text_muted,
                        );
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(name.as_str())
                                    .color(tokens.text)
                                    .strong()
                                    .font(tokens.font_body.clone()),
                            );
                            ui.label(
                                RichText::new(size.as_str())
                                    .color(tokens.text_muted)
                                    .font(tokens.font_mono.clone()),
                            );
                        });
                    });
                });
        }
    }
}

// ---------- Composer ----------

impl Shell {
    pub fn composer<P: Platform + 'static>(&mut self, ui: &mut Ui, state: &State, backend: &BackendHandle<P>) {
        let disabled = state.my_room_id.is_none();
        let placeholder = if disabled {
            "connect and join a room to chat"
        } else {
            "type a message — try /msg <user> hi or /tree announcement"
        };

        SurfaceFrame::new(SurfaceKind::Panel)
            .inner_margin(Margin::symmetric(10, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let _ = ButtonArgs::new("+ attach")
                        .role(PressableRole::Ghost)
                        .disabled(disabled)
                        .show(ui);

                    if ButtonArgs::new("⟳ sync")
                        .role(PressableRole::Ghost)
                        .disabled(disabled)
                        .show(ui)
                        .clicked()
                    {
                        backend.send(Command::RequestChatHistory);
                    }

                    let avail = ui.available_width() - 96.0;
                    let mut submitted: Option<String> = None;

                    ui.add_enabled_ui(!disabled, |ui| {
                        let resp = TextInput::new(&mut self.composer)
                            .placeholder(placeholder)
                            .submit_on_enter(true)
                            .desired_width(avail.max(80.0))
                            .show(ui);
                        if let Some(text) = resp.submitted
                            && !text.trim().is_empty()
                        {
                            submitted = Some(text);
                        }
                    });

                    if ButtonArgs::new("send ↵")
                        .role(PressableRole::Primary)
                        .disabled(disabled || self.composer.trim().is_empty())
                        .show(ui)
                        .clicked()
                    {
                        submitted = Some(std::mem::take(&mut self.composer));
                    }

                    if let Some(text) = submitted {
                        dispatch_composer(state, backend, text);
                    }
                });
            });
    }
}

/// Translate a composer line into the right `Command`. Slash commands
/// route to specialised flows; everything else is room chat.
///
/// - `/msg <user> <text>` → `SendDirectMessage` to the matching user.
///   Validation errors land in chat as a local system message, not
///   as a toast — a toast for a typo is heavier than the offence
///   warrants.
/// - `/tree <text>` → `SendTreeChat` (broadcast to current room and
///   all descendant rooms).
/// - `<text>` → `SendChat` to the current room.
fn dispatch_composer<P: Platform + 'static>(state: &State, backend: &BackendHandle<P>, text: String) {
    let trimmed = text.trim();
    if let Some(rest) = trimmed.strip_prefix("/msg ") {
        let mut parts = rest.splitn(2, char::is_whitespace);
        let username = parts.next().unwrap_or("").trim();
        let body = parts.next().map(str::trim).unwrap_or("");
        if username.is_empty() || body.is_empty() {
            backend.send(Command::SendChat {
                text: "(usage: /msg <username> <text>)".to_string(),
            });
            return;
        }
        match find_user_by_name(state, username) {
            Some((target_user_id, target_username)) => backend.send(Command::SendDirectMessage {
                target_user_id,
                target_username,
                text: body.to_string(),
            }),
            None => backend.send(Command::SendChat {
                text: format!("(no user named '{username}' is connected)"),
            }),
        }
        return;
    }
    if let Some(rest) = trimmed.strip_prefix("/tree ") {
        let body = rest.trim();
        if !body.is_empty() {
            backend.send(Command::SendTreeChat { text: body.to_string() });
        }
        return;
    }
    backend.send(Command::SendChat { text });
}

/// Case-insensitive username → `(user_id, canonical username)` lookup
/// against the current roster. Returns the first match. The canonical
/// username preserves the casing the server reported; the DM command
/// wants both fields.
fn find_user_by_name(state: &State, name: &str) -> Option<(u64, String)> {
    let needle = name.to_ascii_lowercase();
    state.users.iter().find_map(|u| {
        if u.username.to_ascii_lowercase() == needle {
            u.user_id.as_ref().map(|id| (id.value, u.username.clone()))
        } else {
            None
        }
    })
}

// ---------- Voice-state toggles ----------

impl Shell {
    pub fn voice_row<P: Platform + 'static>(&mut self, ui: &mut Ui, state: &State, backend: &BackendHandle<P>) {
        let muted = state.audio.self_muted;
        let deafened = state.audio.self_deafened;
        let ptt_active = state.audio.is_transmitting;

        if ButtonArgs::new("🎤 Mute")
            .role(PressableRole::Default)
            .active(muted)
            .show(ui)
            .clicked()
        {
            backend.send(Command::SetMuted { muted: !muted });
        }
        if ButtonArgs::new("🔇 Deafen")
            .role(PressableRole::Danger)
            .active(deafened)
            .show(ui)
            .clicked()
        {
            backend.send(Command::SetDeafened { deafened: !deafened });
        }

        // Latched click toggle. Hold-to-talk lives on the global
        // hotkey (default Space), wired in `App::pump_hotkeys`; this
        // button is a mouse-friendly fallback.
        let ptt_resp = ButtonArgs::new("● PTT")
            .role(PressableRole::Accent)
            .active(ptt_active)
            .show(ui);
        if ptt_resp.clicked() {
            if ptt_active {
                backend.send(Command::StopTransmit);
            } else {
                backend.send(Command::StartTransmit);
            }
        }
    }
}

// ---------- Self / avatar pill ----------

pub fn avatar_pill(ui: &mut Ui, name: &str) {
    let theme = ui.theme();
    let tokens = theme.tokens().clone();
    let _ = rumble_widgets::Pressable::new(("avatar-pill", name))
        .role(PressableRole::Ghost)
        .min_size(Vec2::new(120.0, 30.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::hover());
                ui.painter().add(RectShape::new(
                    rect,
                    CornerRadius::same(11),
                    tokens.text,
                    Stroke::NONE,
                    StrokeKind::Inside,
                ));
                let initial = name
                    .chars()
                    .next()
                    .map(|c| c.to_ascii_uppercase())
                    .unwrap_or('?')
                    .to_string();
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    initial,
                    FontId::new(11.0, tokens.font_body.family.clone()),
                    tokens.surface,
                );
                ui.add_space(6.0);
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                ui.painter()
                    .circle_filled(dot.center(), 4.0, Color32::from_rgb(0x2f, 0x85, 0x5a));
                ui.label(RichText::new(name).color(tokens.text).font(tokens.font_body.clone()));
            });
        });
}

// ---------- Context menu + modals (overlays) ----------

impl Shell {
    /// Render any open overlay: the right-click context menu on the
    /// tree, or a text-input modal for rename/create/ban/DM. Called by
    /// each paradigm after its main body so overlays float above it.
    pub fn render_overlays<P: Platform + 'static>(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        backend: &BackendHandle<P>,
    ) {
        // The context menu reads `state` to surface the live per-user
        // volume override; the modal needs `state` for username lookups.
        self.render_context_menu(ctx, state, backend);
        self.render_pending_modal(ctx, state, backend);
    }

    fn render_context_menu<P: Platform + 'static>(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        backend: &BackendHandle<P>,
    ) {
        let Some(menu) = self.context_menu.clone() else {
            return;
        };

        let mut close = false;
        let mut next_modal: Option<PendingModal> = None;

        egui::Area::new(egui::Id::new("rumble_next_tree_ctx_menu"))
            .order(egui::Order::Foreground)
            .fixed_pos(menu.pos)
            .show(ctx, |ui| {
                SurfaceFrame::new(SurfaceKind::Popup)
                    .inner_margin(Margin::same(6))
                    .show(ui, |ui| {
                        ui.set_min_width(180.0);
                        match &menu.target {
                            ContextTarget::User {
                                id,
                                name,
                                locally_muted,
                            } => {
                                header(ui, name);
                                if ctx_btn(
                                    ui,
                                    if *locally_muted {
                                        "Unmute locally"
                                    } else {
                                        "Mute locally"
                                    },
                                ) {
                                    backend.send(if *locally_muted {
                                        Command::UnmuteUser { user_id: *id }
                                    } else {
                                        Command::MuteUser { user_id: *id }
                                    });
                                    close = true;
                                }
                                // Per-user volume slider. The current
                                // override (if any) lives in
                                // `state.per_user_rx`; absent users
                                // default to 0 dB.
                                let mut volume_db =
                                    state.audio.per_user_rx.get(id).map(|cfg| cfg.volume_db).unwrap_or(0.0);
                                let before = volume_db;
                                ui.label(
                                    egui::RichText::new("Local volume")
                                        .color(ui.theme().tokens().text_muted)
                                        .font(ui.theme().font(rumble_widgets::TextRole::Label)),
                                );
                                rumble_widgets::Slider::new(&mut volume_db, -40.0..=40.0)
                                    .step(1.0)
                                    .suffix(" dB")
                                    .show(ui);
                                if (volume_db - before).abs() > 0.01 {
                                    backend.send(Command::SetUserVolume {
                                        user_id: *id,
                                        volume_db,
                                    });
                                }
                                ui.separator();
                                if ctx_btn(ui, "Direct message…") {
                                    next_modal = Some(PendingModal::DirectMessage {
                                        user_id: *id,
                                        username: name.clone(),
                                        text: String::new(),
                                    });
                                    close = true;
                                }
                                ui.separator();
                                if ctx_danger(ui, "Kick") {
                                    backend.send(Command::KickUser {
                                        target_user_id: *id,
                                        reason: String::new(),
                                    });
                                    close = true;
                                }
                                if ctx_danger(ui, "Ban…") {
                                    next_modal = Some(PendingModal::Ban {
                                        user_id: *id,
                                        username: name.clone(),
                                        reason: String::new(),
                                    });
                                    close = true;
                                }
                            }
                            ContextTarget::Room { id, name } => {
                                header(ui, name);
                                if ctx_btn(ui, "Join") {
                                    backend.send(Command::JoinRoom { room_id: *id });
                                    close = true;
                                }
                                if ctx_btn(ui, "New sub-room…") {
                                    next_modal = Some(PendingModal::CreateRoom {
                                        parent: Some(*id),
                                        parent_name: name.clone(),
                                        name: String::new(),
                                    });
                                    close = true;
                                }
                                if ctx_btn(ui, "Rename…") {
                                    next_modal = Some(PendingModal::RenameRoom {
                                        id: *id,
                                        original: name.clone(),
                                        name: name.clone(),
                                    });
                                    close = true;
                                }
                                ui.separator();
                                if ctx_danger(ui, "Delete…") {
                                    next_modal = Some(PendingModal::DeleteRoom {
                                        id: *id,
                                        name: name.clone(),
                                    });
                                    close = true;
                                }
                            }
                        }
                    });
            });

        // Click outside the menu closes it. `any_click` fires on
        // release (frame after the press), so the right-click that
        // opened the menu won't immediately close it the same frame.
        let clicked_outside = ctx.input(|i| i.pointer.any_click()) && !ctx.is_pointer_over_area();
        if close || clicked_outside {
            self.context_menu = None;
        }
        if let Some(m) = next_modal {
            self.modal = Some(m);
        }
    }

    fn render_pending_modal<P: Platform + 'static>(
        &mut self,
        ctx: &egui::Context,
        state: &State,
        backend: &BackendHandle<P>,
    ) {
        let Some(modal) = self.modal.as_mut() else {
            return;
        };

        let mut close = false;
        let mut submit: Option<Command> = None;
        let title: &str;
        let primary_label: &str;

        // Snapshot the simple type-based primary-button label here; the
        // body of the modal edits the buffer in place below.
        match modal {
            PendingModal::CreateRoom { .. } => {
                title = "Create room";
                primary_label = "Create";
            }
            PendingModal::RenameRoom { .. } => {
                title = "Rename room";
                primary_label = "Rename";
            }
            PendingModal::DeleteRoom { .. } => {
                title = "Delete room";
                primary_label = "Delete";
            }
            PendingModal::Ban { .. } => {
                title = "Ban user";
                primary_label = "Ban";
            }
            PendingModal::DirectMessage { .. } => {
                title = "Direct message";
                primary_label = "Send";
            }
        }

        let _ = state; // future use for permission-gated flows

        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| match modal {
                PendingModal::CreateRoom {
                    parent,
                    parent_name,
                    name,
                } => {
                    ui.label(RichText::new(format!("Parent: {parent_name}")).color(ui.theme().tokens().text_muted));
                    ui.add_space(6.0);
                    GroupBox::new("Name")
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            TextInput::new(name)
                                .placeholder("Room name")
                                .desired_width(280.0)
                                .show(ui);
                        });
                    ui.add_space(8.0);
                    let can_submit = !name.trim().is_empty();
                    modal_buttons(ui, primary_label, can_submit, &mut close, |go| {
                        if go {
                            submit = Some(Command::CreateRoom {
                                name: name.trim().to_string(),
                                parent_id: *parent,
                            });
                        }
                    });
                }
                PendingModal::RenameRoom { id, original, name } => {
                    ui.label(RichText::new(format!("Was: {original}")).color(ui.theme().tokens().text_muted));
                    ui.add_space(6.0);
                    GroupBox::new("New name")
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            TextInput::new(name)
                                .placeholder("Room name")
                                .desired_width(280.0)
                                .show(ui);
                        });
                    ui.add_space(8.0);
                    let trimmed = name.trim().to_string();
                    let can_submit = !trimmed.is_empty() && trimmed != *original;
                    modal_buttons(ui, primary_label, can_submit, &mut close, |go| {
                        if go {
                            submit = Some(Command::RenameRoom {
                                room_id: *id,
                                new_name: trimmed.clone(),
                            });
                        }
                    });
                }
                PendingModal::DeleteRoom { id, name } => {
                    ui.label(
                        RichText::new(format!("Permanently delete room \"{name}\"?")).color(ui.theme().tokens().text),
                    );
                    ui.label(RichText::new("This cannot be undone.").color(ui.theme().tokens().text_muted));
                    ui.add_space(8.0);
                    modal_buttons(ui, primary_label, true, &mut close, |go| {
                        if go {
                            submit = Some(Command::DeleteRoom { room_id: *id });
                        }
                    });
                }
                PendingModal::Ban {
                    user_id,
                    username,
                    reason,
                } => {
                    ui.label(
                        RichText::new(format!("Ban {username}"))
                            .strong()
                            .color(ui.theme().tokens().text),
                    );
                    ui.add_space(6.0);
                    GroupBox::new("Reason")
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            TextInput::new(reason)
                                .placeholder("optional reason shown to the user")
                                .desired_width(320.0)
                                .show(ui);
                        });
                    ui.add_space(8.0);
                    modal_buttons(ui, primary_label, true, &mut close, |go| {
                        if go {
                            submit = Some(Command::BanUser {
                                target_user_id: *user_id,
                                reason: reason.clone(),
                                duration_seconds: None,
                            });
                        }
                    });
                }
                PendingModal::DirectMessage {
                    user_id,
                    username,
                    text,
                } => {
                    ui.label(
                        RichText::new(format!("Message {username}"))
                            .strong()
                            .color(ui.theme().tokens().text),
                    );
                    ui.add_space(6.0);
                    GroupBox::new("Text")
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            TextInput::new(text)
                                .placeholder("write a direct message…")
                                .desired_width(320.0)
                                .submit_on_enter(true)
                                .show(ui);
                        });
                    ui.add_space(8.0);
                    let can_submit = !text.trim().is_empty();
                    let target_name = username.clone();
                    modal_buttons(ui, primary_label, can_submit, &mut close, |go| {
                        if go {
                            submit = Some(Command::SendDirectMessage {
                                target_user_id: *user_id,
                                target_username: target_name.clone(),
                                text: text.trim().to_string(),
                            });
                        }
                    });
                }
            });

        if let Some(cmd) = submit {
            backend.send(cmd);
            close = true;
        }
        if close {
            self.modal = None;
        }
    }
}

fn header(ui: &mut Ui, name: &str) {
    let tokens = ui.theme().tokens().clone();
    ui.label(
        RichText::new(name)
            .color(tokens.text_muted)
            .font(tokens.font_label.clone()),
    );
    ui.add_space(2.0);
}

fn ctx_btn(ui: &mut Ui, label: &str) -> bool {
    ButtonArgs::new(label)
        .role(PressableRole::Ghost)
        .min_width(160.0)
        .show(ui)
        .clicked()
}

fn ctx_danger(ui: &mut Ui, label: &str) -> bool {
    ButtonArgs::new(label)
        .role(PressableRole::Danger)
        .min_width(160.0)
        .show(ui)
        .clicked()
}

fn modal_buttons(ui: &mut Ui, primary: &str, can_submit: bool, close: &mut bool, mut on_action: impl FnMut(bool)) {
    ui.horizontal(|ui| {
        if ButtonArgs::new("Cancel")
            .role(PressableRole::Default)
            .show(ui)
            .clicked()
        {
            on_action(false);
            *close = true;
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ButtonArgs::new(primary)
                .role(PressableRole::Primary)
                .disabled(!can_submit)
                .show(ui)
                .clicked()
            {
                on_action(true);
            }
        });
    });
}
