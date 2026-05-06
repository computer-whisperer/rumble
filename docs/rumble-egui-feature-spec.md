# rumble-egui Feature Specification

This document is a port-oriented inventory of every distinct user-visible
feature in `crates/rumble-egui`, the long-running egui-based desktop
client. It exists so that a port to a different GUI library (e.g. *aetna*)
can be done with eyes open: we know what we are reproducing, what is
present-but-latent, and which features have already migrated into the
`rumble-next` parallel client.

Conventions:

- File paths are absolute from the repo root.
- Line references are to `crates/rumble-egui/src/app.rs` unless noted.
- "Backend" / "BackendHandle" refers to `rumble_client::handle::BackendHandle`,
  which holds an `Arc<RwLock<rumble_client::State>>` and a command channel.
- "Local UI state" lives on `RumbleApp` itself (not in the backend).
- "Commands" are variants of `rumble_client::Command` that the UI emits.

When a feature lives mostly in `rumble-next` (a parallel re-design of this
client) the spec calls it out. The intent is that the new aetna client
should reach **rumble-next parity** plus everything in `rumble-egui` that
hasn't yet been ported.

---

## 1. Top-Level Structure

### 1.1 Crate layout

- Library entry: `crates/rumble-egui/src/lib.rs` — re-exports `RumbleApp`,
  `TestHarness`, `Args`, `PersistentSettings`, plus the shared identity /
  hotkey / toast types from `rumble-desktop-shell`.
- Binary entry: `crates/rumble-egui/src/main.rs` — the eframe runner.
- `RumbleApp` (the main app type) is in `app.rs` (~5535 lines) — it is
  intentionally GUI-runtime-agnostic so it can be driven from eframe,
  the in-process test harness, or harness-cli.
- Custom desktop widget kit: `crates/rumble-widgets/` (consumed by
  `rumble-next`, **not** by `rumble-egui` itself — egui uses stock widgets
  plus `egui_ltreeview` from a u6bkep fork).
- Default backend type alias: `pub type BackendHandle =
  rumble_client::handle::BackendHandle<rumble_desktop::NativePlatform>`.

### 1.2 Window / viewport

Defined in `main.rs:59-64`.

- Title: `"Rumble"`.
- Initial inner size: 1000 × 700.
- Minimum inner size: 800 × 500.
- No tray icon, no minimize-to-tray, no always-on-top.

### 1.3 Runtime composition (port checklist)

Order in `main.rs`:

1. Build a `tracing_subscriber::EnvFilter` and add the directive
   `egui_winit::clipboard=off` to suppress a known noisy ERROR that
   fires on Wayland/X11 when the clipboard contains image data.
2. `Args::parse()`.
3. **RPC client short-circuit (Unix only).** If `--rpc <cmd>` is set,
   build a fresh single-thread tokio runtime, call
   `rumble_egui::rpc_client::run_rpc_command(socket_path, cmd)`, exit.
4. `PersistentSettings::load(args.config_dir)` from JSON in the config
   dir resolved by `directories::ProjectDirs::from("com","rumble","Rumble")`.
5. **`HotkeyManager::new()` on the main thread** — required by the
   `global-hotkey` crate on macOS, must happen before `eframe::run_native`
   takes the main thread. Then
   `register_from_settings(&settings.keyboard)`.
6. Build a multi-thread tokio runtime (`worker_threads(1)`,
   `enable_all`) — passed down into `RumbleApp`.
7. `eframe::run_native("Rumble", options, ...)`. Inside the creator
   closure, install image loaders:
   `egui_extras::install_image_loaders(&cc.egui_ctx)`.
8. Inside `EframeWrapper::new`: `runtime.block_on(
   hotkey_manager.init_portal_backend(handle))` — initialises the XDG
   Portal hotkey backend on Wayland (zbus needs a tokio runtime).
9. Construct `RumbleApp::new(ctx, runtime.handle().clone(), args)`.
10. Push hotkey state into the app:
    `set_portal_hotkeys_available(...)`,
    `set_hotkey_registration_status(...)`,
    and on Linux `set_portal_shortcuts(...)`.
11. Per frame, in `EframeWrapper::update`, drain
    `hotkey_manager.poll_events()` into `app.handle_hotkey_event(...)`,
    then call `app.render(ctx)`.
12. `Drop` impl on `RumbleApp` sends `Command::Disconnect` if connected.

A port to a non-eframe runtime needs to reproduce all of the above. In
particular: image decoder install, AccessKit (only enabled by the
harness-cli daemon today), portal backend init order, and the main-thread
hotkey-manager constraint.

### 1.4 Logging

All tracing goes through `tracing_subscriber::fmt()` with the
`RUST_LOG`-driven env filter plus the clipboard suppression directive.
User-facing errors are surfaced two ways:

- **Toasts** via `ToastManager` (auto-hiding overlays, see §11).
- **Local-only chat lines** via `Command::LocalMessage { text }`
  (rendered inline in the chat panel as gray italic system messages).

---

## 2. Application Lifecycle & First Run

### 2.1 First-run identity wizard

State machine in `crates/rumble-egui/src/first_run.rs`; rendering at
`app.rs:2817-3243`. Triggered automatically on launch when
`KeyManager::needs_setup()` returns true (i.e. no `identity.json` on
disk). Rendered as a non-dismissible `egui::Modal`.

States:

- **`SelectMethod`** — welcome screen with two buttons:
  - **Generate Local Key** (recommended).
  - **Use SSH Agent** (advanced; disabled with
    *"⚠ SSH_AUTH_SOCK not set"* if `SshAgentClient::is_available()` is
    false).
- **`GenerateLocal { password, password_confirm, error }`** — optional
  password entry (with confirm). Empty password = plaintext storage;
  non-empty = ChaCha20-Poly1305 encrypted with Argon2-derived key
  (`encrypted-keys` feature). Live "Passwords don't match" warning;
  Generate button disabled until match. `← Back` returns.
- **`ConnectingAgent`** — spinner while a tokio task connects to the
  SSH agent and lists Ed25519 keys. `← Cancel` returns.
- **`SelectAgentKey { keys, selected, error }`** — selectable list of
  `KeyInfo { fingerprint, comment, public_key }` rendered as
  `"{comment} ({fingerprint})"`. Buttons: **Use Selected Key**,
  **Generate New Key**, `← Back`.
- **`GenerateAgentKey { comment }`** — generate a fresh Ed25519 key and
  `ssh-add` it into the running agent with a user-supplied comment
  (default `"rumble-identity"`). Spinner + "Generating..." while pending.
- **`Error { message }`** — red error message, `← Back to Start`.

Side effects on success:

- Writes `identity.json`.
- Posts a `Command::LocalMessage` to chat:
  *"Identity key generated: {fingerprint}"* or
  *"Using SSH agent key: {comment} ({fingerprint})"*.

Legacy migration: if the old `settings.json` still has
`identity_private_key_hex`, it is auto-imported as plaintext on first
launch (no UI shown). See `app.rs:585-596`.

### 2.2 Identity storage (`KeyManager`)

In `crates/rumble-desktop-shell/src/identity/key_manager.rs`. Re-exported
as `rumble_egui::key_manager`.

- `KeyConfig { source: KeySource, public_key_hex }` persists to
  `<config_dir>/identity.json` (separate from `settings.json`).
- `KeySource` variants:
  - `LocalPlaintext { private_key_hex }` (32-byte Ed25519 in hex).
  - `LocalEncrypted { encrypted_data, salt_hex, nonce_hex }` — Argon2 →
    ChaCha20-Poly1305; 16-byte salt, 12-byte nonce.
  - `SshAgent { fingerprint, comment }` — agent holds the private key.
- Fingerprint format: `SHA256:xx:xx:...` (16 colon-separated hex bytes —
  first 16 bytes of SHA-256). See `compute_fingerprint`.
- Plaintext keys are eagerly decoded into a cached `SigningKey` on
  `KeyManager::new`; encrypted keys are not until `unlock_local_key`.
- `create_signer()` returns a closure
  `Arc<dyn Fn(&[u8]) -> Result<[u8;64], String>>` used as the connect
  handshake signer.
  - Plain/Encrypted: synchronous `SigningKey::sign`.
  - SSH agent: spawns a one-shot thread per signature, builds a fresh
    current-thread tokio runtime, connects, signs, returns.
- **Latent**: `needs_unlock()` is true for `LocalEncrypted` when there
  is no cached key, and `connect()` will surface
  *"Cannot connect: Key is encrypted. Please unlock it in settings."* —
  but **there is no unlock UI in settings today**.

### 2.3 Settings → Connection: Identity panel

`app.rs:1333-1374`. The in-settings surface for identity:

- **Public Key** — truncated `xxxxxxxx...xxxxxxxx` in a code block, plus
  a 📋 copy button (full hex via `ui.ctx().copy_text`).
- **Storage** — human label: *Local (unencrypted)*, *Local (password
  protected)*, or *SSH Agent ({comment})*.
- **🔑 Generate New Identity...** — re-enters first-run flow at
  `SelectMethod`. *No confirmation prompt; will replace the current key.*
- When `KeyManager` has no config (shouldn't happen normally): yellow
  *"⚠ No identity key configured"* + **🔑 Configure Identity...** button.

### 2.4 No identity-import UI

`KeyManager::import_signing_key` is only used for legacy migration. There
is no user-facing import-by-paste or import-from-file UI today.

---

## 3. Connection / Servers

### 3.1 Connect dialog

Triggered by **Server → Connect...** menu item or `--server` CLI arg.
Modal at `app.rs:4769-4799`, fixed width 280px.

Fields:

- **Server address** — `text_edit_singleline` bound to `connect_address`.
  Default `127.0.0.1:5000` when empty at connect time.
- **Username** — `text_edit_singleline` bound to `client_name`.
- **Password (optional)** — `text_edit_singleline` bound to
  `connect_password`. **Note**: not masked (no `.password(true)`),
  the value is shown in plaintext. Fix when porting.
- **Trust dev cert** — checkbox bound to `trust_dev_cert`; when set,
  `dev-certs/server-cert.der` is added to the QUIC trust list.

Buttons: **Connect** (calls `RumbleApp::connect()` then closes) and
**Cancel** (just closes).

### 3.2 `connect()` flow

`app.rs:1006-1059`.

Pre-flight checks (each emits a `Command::LocalMessage` and aborts):

- No public key bytes → *"Cannot connect: No identity key configured.
  Please complete first-run setup."*
- `key_manager.needs_unlock()` → *"Cannot connect: Key is encrypted.
  Please unlock it in settings."*
- No signer otherwise → *"Cannot connect: Failed to create signer for
  key."*

Side effects on success:

- Posts *"Connecting to {addr}..."* to chat.
- `remember_shared_server(addr, name)` updates `desktop-shell.json`
  recent-servers list.
- Sends `Command::Connect { addr, name, public_key, signer, password }`.

### 3.3 Auto-connect on launch

Triggers (mutually exclusive with first-run gate):

- `--server <addr>` non-empty AND first-run not needed → connect
  immediately.
- `persistent_settings.autoconnect_on_launch == true` AND
  `connect_address` is non-empty AND first-run not needed → emit
  *"Auto-connecting..."* and connect.

### 3.4 Server menu

`app.rs:3634-3652`.

- **Connect...** — opens connect modal.
- **Disconnect** (only when connected) — sends `Command::Disconnect`.
- **Reconnect** (only when not connected and `connect_address`
  non-empty) — `RumbleApp::reconnect()` posts *"Reconnecting to
  {addr}..."* and re-runs `connect()`.

`Drop` impl on `RumbleApp` sends `Command::Disconnect` if still
connected, ensuring clean shutdown.

### 3.5 Connection-status pill (top-right of menu bar)

Reads `state.connection: ConnectionState` (5 variants). At
`app.rs:3704-3725`.

- **Disconnected** → gray *"○ Disconnected"*.
- **Connecting { server_addr }** → yellow *"⚫ Connecting..."*.
- **Connected { server_name, user_id }** → green *"⚫ Connected"*.
- **ConnectionLost { error }** → red *"⚫ Connection Lost: {error}"* +
  **⟳ Reconnect** button.
- **CertificatePending { cert_info }** → yellow
  *"⚠ Certificate Verification"*.

### 3.6 Connection state-transition effects

`app.rs:3340-3369`. Tracked via `prev_connection_state`.

- `→ Connected`: success toast *"Connected to server"* + `SfxKind::Connect`.
- `→ ConnectionLost`: error toast *"Connection lost: {error}"* +
  `SfxKind::Disconnect`.
- `Connected → Disconnected`: `SfxKind::Disconnect` only (treated as
  user-initiated, no toast).
- `Connected/Lost → Connecting`: info toast *"Reconnecting to server..."*.
- After every frame, drains `state.permission_denied: Option<String>` →
  error toast *"Permission denied: {msg}"*.
- After every frame, drains `state.kicked: Option<String>` → error
  toast with the reason.
- PTT-active flag is force-cleared if connection drops mid-PTT.

### 3.7 Recent servers (latent — populated, not surfaced)

`shared_settings.recent_servers: Vec<RecentServer { addr, label,
username, last_used_unix }>` is populated on every successful connect
and used to pre-fill the connect dialog with the most-recently-used
server (sorted by `last_used_unix`). **No user-visible saved-server
picker, no rename, no "switch to" dropdown today.** The `label` field is
defined but never set or displayed.

### 3.8 Auto-connect address (shared store)

`shared_settings.auto_connect_addr: Option<String>` tracks which recent
server to auto-connect to. It is set/cleared inside
`remember_shared_server` based on `autoconnect_on_launch`.

---

## 4. Certificate Trust (TOFU)

### 4.1 Pre-load on startup

`app.rs:660-692`. On startup the app iterates
`persistent_settings.accepted_certificates` and
`shared_settings.accepted_certificates`, base64-decodes the DER bytes,
and pushes them into `ConnectConfig.accepted_certs`. Bad base64 logs a
warning but does not fail.

It also adds:

- `dev-certs/server-cert.der` if `trust_dev_cert` is true.
- The `--cert <path>` value if set.
- The `RUMBLE_SERVER_CERT_PATH` env-var path if set.

### 4.2 Untrusted-cert modal

Auto-shown when `state.connection == CertificatePending { cert_info }`.
At `app.rs:4660-4767`, fixed width 450px.

Layout:

- Heading: *"⚠ Untrusted Certificate"*.
- Body: *"The server presented a certificate that is not trusted."*
- *"Server: {server_addr}"* and *"Certificate for: {server_name}"* (bold).
- Read-only multiline `TextEdit` showing the SHA-256 fingerprint as
  colon-separated hex (`cert_info.fingerprint_hex()`), 2 rows, monospace.
- Help: *"If you expected to connect to a server with a self-signed
  certificate, verify the fingerprint matches what the server
  administrator provided."*
- Footer: weak *"⚠ Only accept if you trust this server"* on the left,
  **Accept Certificate** + **Reject** on the right.

`PendingCertificate` carries the full handshake state
(`certificate_der`, raw 32-byte fingerprint, `server_name`,
`server_addr`, `username`, `password`, `public_key`, `signer`) so accept
retries with identical credentials — the user does not re-enter
anything.

### 4.3 Accept side effects

- Pushes into `persistent_settings.accepted_certificates: Vec<
  AcceptedCertificate { server_name, fingerprint_hex,
  certificate_der_base64 }>` (deduped by fingerprint); saves
  `settings.json`.
- Pushes into `shared_settings.accepted_certificates` (deduped by
  `(server_name, fingerprint)`); saves `desktop-shell.json`.
- Sends `Command::AcceptCertificate`.
- Posts *"Accepted certificate for {server_name} (saved for future
  connections)"* to chat.

### 4.4 Reject side effects

- Sends `Command::RejectCertificate`.
- Posts *"Rejected certificate for {server_name}"* to chat.

### 4.5 Caveat: TOFU is fingerprint-based, not server-pinned

Accepted certs apply globally across server names — there is no
per-server pinning enforcement at the UI layer.

---

## 5. Window Layout

The top-level shell is a stack of egui panels around a central area.

| Region | Type | Where | Purpose |
|---|---|---|---|
| Top menu bar | `TopBottomPanel::top("top_panel")` | `app.rs:3632-3727` | Server/Settings/File-Transfer menus + right-aligned status pill. |
| Toolbar | `TopBottomPanel::top("toolbar_panel")` | `app.rs:3730-3879` | Mute/Deafen/Voice mode/Elevate/Settings gear. |
| Left side | `SidePanel::left("left_panel")`, default 320px, resizable | `app.rs:3882-4056` | Chat panel (header + history + input). |
| Center | `CentralPanel::default` | `app.rs:4059-4654` | Room / user tree. |

There is **no** bottom status bar, **no** docked right panel, **no** tray
icon, **no** always-on-top, **no** minimize-to-tray.

A drag-hover overlay is drawn as an `egui::Area` at `Order::Foreground`
when files are being dragged over the window (see §10.3).

---

## 6. Top Menu Bar

`app.rs:3634-3725`.

### 6.1 Server menu

- **Connect...** — open connect modal.
- **Disconnect** (when connected) — `Command::Disconnect`.
- **Reconnect** (when not connected, address non-empty) —
  `RumbleApp::reconnect()`.

### 6.2 Settings menu

- **Open Settings** — primes pending settings state from current backend
  state and opens the settings modal (same as gear icon).

### 6.3 File Transfer menu

- **Share File...** — gated on `Permissions::SHARE_FILE` and no
  in-flight dialog. Spawns
  `rfd::AsyncFileDialog::new().pick_file()` on the tokio runtime;
  pending `JoinHandle` is polled per frame and fires
  `Command::ShareFile { path }` when complete. Also flips
  `show_transfers = true`.
- **Show Transfers** — sets `self.show_transfers = true`. **Note:**
  rumble-egui itself never *renders* a transfers window from this flag;
  the bool is exposed via `set_show_transfers` / `is_transfers_visible`
  for external consumers (RPC, harness, other shells). The actual
  transfers UI lives in `rumble-next`.

### 6.4 Right side: connection-status pill

See §3.5 for details.

There is no File / Edit / View / Help equivalent.

---

## 7. Toolbar

`app.rs:3730-3879`. Order, left-to-right:

### 7.1 Self-mute button

- **Server-muted**: red 🔒, hover *"Server muted - you cannot speak in
  this room"*, non-clickable.
- **Self-muted**: 🔇 red, hover *"Unmute"*.
- **Live**: 🎤 green, hover *"Mute"*.
- Click → `Command::SetMuted { muted }`.
- State: `state.audio.self_muted`, plus own user's `server_muted`.

### 7.2 Self-deafen button

- 🔇 red (deafened) or 🔊 green (hearing).
- Click → `Command::SetDeafened { deafened }`.
- State: `state.audio.self_deafened`.

### 7.3 Voice-mode dropdown

ComboBox showing 🎤 PTT or 📡 Continuous. Selecting persists immediately
(does not require Apply). Sends `Command::SetVoiceMode`.

### 7.4 Elevate (sudo) button

Visible only when connected and the local user is not already elevated.
🔑 button → opens the Elevate modal (see §13.6). After elevation, the
button disappears and the user gains a 🛡 badge in the user list.

### 7.5 Settings gear

⚙ button on the right. Snapshots audio/connection state into
`SettingsModalState::pending_*` fields and opens the settings modal.

### 7.6 Implicit / latent

There is **no** toolbar transmit indicator (mic flash). Self-transmit is
visible only as the green mic icon next to your username in the user
list.

---

## 8. Room Tree (Central Panel)

`app.rs:4059-4654`. Uses `egui_ltreeview` from a u6bkep fork (see
`Cargo.toml:22`). Override indent: 20px.

### 8.1 Node identity

```
enum TreeNodeId {
    Room(Uuid),
    User { room_id: Uuid, user_id: u64 },
}
```

Users appear inline as leaf nodes inside their room — there is no
separate user-list panel.

### 8.2 Auto-expand

A room is opened by default if it or any descendant contains users
(`rooms_with_users` set, computed by walking ancestors). Empty branches
stay collapsed.

### 8.3 Room label

`📁 {name}` plus `({user_count})` suffix when non-zero, plus
`  [current]` when this is the user's current room. Empty rooms are
rendered with `weak()` styling.

### 8.4 Activation

`Action::Activate` (Enter or double-click) sends
`Command::JoinRoom { room_id }`. If `auto_sync_history` is enabled, also
fires `Command::RequestChatHistory` immediately afterward.

### 8.5 Drag-and-drop reparenting

Dragging one room onto another opens a confirmation modal; on confirm,
sends `Command::MoveRoom { source, target }`. Source and target must
both be `TreeNodeId::Room` and different. Users cannot be dragged.

### 8.6 Room context menu (right-click)

Shows room metadata (name, ID, parent UUID, italic description) followed
by:

- **Join** — always.
- **Rename...** — gated on `MODIFY_ROOM`. Opens `RenameModalState`.
- **Edit Description...** — gated on `MODIFY_ROOM`. Opens
  `DescriptionModalState`.
- **Add Child Room** — gated on `MAKE_ROOM`. Sends
  `Command::CreateRoom { name: "New Room", parent_id }` immediately.
- **Delete Room** — gated on `MODIFY_ROOM` + non-root. Confirmation
  modal.
- **Edit ACLs** — gated on `WRITE`. Opens room ACL editor (see §13.10).

### 8.7 Empty / disconnected placeholders

- Disconnected: *"Not connected. Use Server > Connect..."*.
- Connected but no rooms: *"No rooms received yet."* + **Join Root** /
  **Refresh** buttons (both `Command::JoinRoom { ROOT_ROOM_UUID }`).

### 8.8 Per-room permissions

`state.per_room_permissions: HashMap<Uuid, u32>` is consulted (with
fallback to `state.effective_permissions`) so each context-menu item is
gated correctly per room.

---

## 9. User List (inline in tree)

`app.rs:4264-4514`. Each user leaf shows, left-to-right:

### 9.1 Mic icon (priority order)

1. **🔒 red** — server-muted (highest priority).
2. **🎤 green** — talking (or the local user when
   `state.audio.is_transmitting`).
3. **🎤 dark-red** — self-muted.
4. **🎤 dark-gray** — idle.

### 9.2 Other status icons

- **🔇 dark-red** — user is deafened.
- **🔕 yellow** — locally muted (only shown for other users).
- **🛡 gold** — elevated/superuser, hover *"Elevated (Superuser)"*.

### 9.3 Username

Plain `Label`. **No avatars, no per-user color, no AFK indicator.**

### 9.4 Self context menu

Header rows: User name, ID, current room, optional Groups.

- **Mute / Unmute** — `Command::SetMuted`.
- **Deafen / Undeafen** — `Command::SetDeafened`.
- **Register / Unregister** — gated on `SELF_REGISTER`.
  `Command::RegisterUser` / `UnregisterUser` for own user.

### 9.5 Other-user context menu

- **🔊 Volume** slider, -40..=20 dB, 1 dB step → `Command::SetUserVolume
  { user_id, volume_db }`.
- **Reset Volume** button.
- **🔕 Mute Locally / 🔔 Unmute Locally** — `Command::MuteUser` /
  `UnmuteUser`.
- **🔒 Server Mute / 🔒 Remove Server Mute** — gated on `MUTE_DEAFEN`.
  `Command::SetServerMute`.
- **⚡ Kick** — gated on `KICK`. Opens kick modal.
- **🚫 Ban** — gated on `BAN`. Opens ban modal.
- **📝 Register / ❌ Unregister** — gated on `REGISTER`.

State touched: `state.audio.muted_users`, `state.audio.per_user_rx`,
`state.per_room_permissions`.

---

## 10. Voice Path

### 10.1 Self transmit indicator

Driven by `state.audio.is_transmitting`. Visible only via the green mic
icon next to the local user in the tree.

### 10.2 PTT — global hotkey path

`HotkeyManager::poll_events()` is drained every frame in
`EframeWrapper::update`; events route to
`RumbleApp::handle_hotkey_event(...)` (`app.rs:1162-1205`).

- `HotkeyEvent::PttPressed` → `Command::StartTransmit`.
- `HotkeyEvent::PttReleased` → `Command::StopTransmit`.
- `HotkeyEvent::ToggleMute` → `Command::SetMuted { !current }`.
- `HotkeyEvent::ToggleDeafen` → `Command::SetDeafened { !current }`.

Works when the window is unfocused (Win/macOS/X11 via `global-hotkey`;
Wayland via the XDG portal — see §15).

### 10.3 PTT — window-focused fallback

`app.rs:3496-3534`. Each frame, polls the configured PTT key via
`ctx.input(|i| i.key_down(...))`. Suppressed when a text input has
focus (`ctx.wants_keyboard_input()`). Releases on key-up.

The toggle-mute / toggle-deafen hotkeys have an analogous fallback at
`app.rs:3537-3591` using `key_pressed` semantics.

### 10.4 Drag-and-drop file sharing (window-level)

Files dropped onto the window iterate
`ctx.input(|i| i.raw.dropped_files.clone())` and send
`Command::ShareFile { path }` per file. Sets `show_transfers = true`.

### 10.5 Drag-hover overlay

While files are being dragged over the window, a translucent blue
full-window overlay (`Color32::from_rgba_unmultiplied(0,100,200,100)`)
is drawn with centered heading *"Drop files here to share"*. Gated on
`state.connection.is_connected()`.

---

## 11. Toasts

Implemented by `ToastManager` in
`crates/rumble-desktop-shell/src/toasts.rs`. Rendered as the very last
thing in `RumbleApp::render` (`app.rs:5533`) so it overlays everything.

### 11.1 Placement / styling

- Bottom-right, stacked vertically (newest at the bottom).
- 12px screen margin, 320px width, 6px spacing.
- Each toast is its own foreground `egui::Area`, non-interactable;
  rounded-rect (radius 6) background + white text.
- Severity colors:
  - Success — `#4CAF50`.
  - Error — `#F44336`.
  - Info — `#2196F3`.
  - Warning — `#FF9800`.
- Default durations: Success/Info 4s; Error/Warning 6s. Last 1s fades
  alpha 255 → 0 linearly.
- While any toast is alive: `request_repaint_after(50ms)`.

### 11.2 Trigger sites

- Connection-state transitions (§3.6).
- `state.permission_denied` → error toast.
- `state.kicked` → error toast.
- *"Settings saved"* on Apply.
- Clipboard image paste path: warning/error/success outcomes.

### 11.3 No per-message chat toast

New chat messages do **not** raise a toast — only the
`SfxKind::Message` SFX (see §12.7).

---

## 12. Chat Panel

`app.rs:3882-4056`. Rendered inside the left side panel, laid out as a
vertical `egui_extras::StripBuilder` with sections (header /
remainder-scroll-area / 4px separator / 28px input row).

> **Important:** the recent rumble-next commits
> (`rumble-next: inline image previews, lightbox, file-card right-click
> menu`, `Replace JSON file messages with typed FileOffer attachment +
> auto-download`) live **only in `rumble-next`**, not in `rumble-egui`.
> The chat panel in `rumble-egui` is bare-bones. The new aetna client
> should pick up rumble-next's expanded chat/file UX in addition to the
> features below.

### 12.1 Composer

- Single-line `TextEdit::singleline` bound to `chat_input`.
- Send triggers: Enter (focus-lost on press) or **Send** button.
- On send, clears the input.
- **No multiline support** (no Shift+Enter newline).
- Disabled with hint *"Connect to a server to chat"* when disconnected,
  or *"You don't have permission to chat in this room"* when lacking
  `Permissions::TEXT_MESSAGE`.

### 12.2 Slash commands

Parsed before sending plain chat:

- `/msg <username> <message>` → resolves `username` against
  `state.users`, sends `Command::SendDirectMessage { target_user_id,
  target_username, text }`. Unknown user → local error
  *"User '<name>' not found"*.
- `/tree <message>` → `Command::SendTreeChat { text }` (broadcasts to
  the current room and all descendants).
- Anything else → `Command::SendChat { text }`.

Usage errors like *"Usage: /msg <username> <message>"* are
`Command::LocalMessage` lines.

### 12.3 Paste-image button (📋)

`app.rs:891-929` (`paste_clipboard_image`). Reads RGBA from the system
clipboard via `arboard::Clipboard::new().get_image()` (bypassing
egui_winit's text-only clipboard), encodes PNG via `image::RgbaImage`,
writes to a `tempfile::tempdir()`-managed path (which is
`std::mem::forget`'d so the file outlives the call), and shares via
`Command::ShareFile`. Outcomes:

- *"Connect to a server before pasting images"*.
- *"Could not access clipboard"*.
- *"No image on clipboard"*.
- *"Failed to process clipboard image"*.
- Success: toast *"Sharing pasted image"*; opens transfers.

`Ctrl+V` is **not** wired up — egui_winit currently swallows `Key::V`,
tracked at egui#2108.

### 12.4 Sync (request chat history) button

**↻ Sync** sends `Command::RequestChatHistory` to ask peers in the
current room for their chat history. The backend posts a local
*"Requesting chat history from peers..."* status line (see
`crates/rumble-client/src/handle.rs:1179-1188`).

### 12.5 History rendering

Vertical `ScrollArea` with `stick_to_bottom(true)`. Empty state:

- Connected: *"No messages yet"* (centered gray italic).
- Disconnected: *"Connect to a server to start chatting"*.

Each `ChatMessage` is rendered as a single `Label`. **No avatar, no
per-user color, no message grouping, no bubbles, no separator.** Three
branches:

- `is_local == true` — gray italic system text.
- `kind == DirectMessage` — purple `RGB(200,150,255)`,
  prefix `[DM] sender: text`.
- `kind == Tree` — green `RGB(150,200,150)`, prefix
  `[Tree] sender: text`.
- `kind == Room` — default color, `sender: text`.

### 12.6 Optional timestamps

`persistent_settings.show_chat_timestamps == true` prefixes every line
with `[<formatted-time>] `. Format from `TimestampFormat::all()`:

- `Time24h` — *"24-hour (14:30:05)"*.
- `Time12h` — *"12-hour (2:30:05 PM)"*.
- `DateTime24h`, `DateTime12h`.
- `Relative` — *"5m ago"*.

### 12.7 New-message SFX

`SfxKind::Message` plays when `state.chat_messages.len()` increases
between frames and at least one new message has `is_local == false`.
Tracked via `prev_chat_count` (`app.rs:548`).

### 12.8 Message types & attachments (wire)

`crates/rumble-protocol/src/types.rs`:

- `ChatMessage { id: [u8;16], sender, text, timestamp, is_local, kind,
  attachment }`.
- `ChatMessageKind = Room | DirectMessage { other_user_id,
  other_username } | Tree`.
- `ChatAttachment::FileOffer(FileOfferInfo { schema_version,
  transfer_id, name, size, mime, share_data })`.

**`rumble-egui` does not render `attachment`.** A file offer arrives as
plain text with the attachment present but invisible. The renderer at
`app.rs:3912` only inspects `kind` and `text`. (rumble-next renders
the `FileOffer` attachment as an inline file card with image preview /
right-click menu / lightbox.)

### 12.9 System messages from the client

Local-only `is_local=true` messages injected for: client banner
*"Rumble Client v…"*, client name, *"Connecting to <addr>..."*,
*"Auto-connecting..."*, *"Reconnecting to <addr>..."*, *"Settings
saved."* / *"Failed to save settings: …"*, key-status messages, slash-
command usage errors, *"Requesting chat history from peers..."*. Joins
/ leaves / topic changes / mutes are **not** written to chat — those
surface via tree state, toasts, and SFX.

### 12.10 Per-room vs DM

Single shared `Vec<ChatMessage>`. There is no per-DM tab or per-room
chat tab; DMs and tree broadcasts are interleaved with room chat and
distinguished only by prefix and color.

### 12.11 Latent: image-view modal

`ImageViewModalState` (`app.rs:82-106`) and full rendering at
`app.rs:5389-5530` exist:

- Header with image name + zoom controls (`−` / `+` / **Fit** = 100% /
  **Close**), zoom-percentage label.
- Scroll-wheel zooms (uses `raw_scroll_delta`); drag pans.
- Lazy load full-resolution from disk via `std::fs::read(path)` +
  `ctx.include_bytes(uri, bytes)` + `ctx.try_load_texture(uri,
  TextureOptions::LINEAR, SizeHint::Scale(1.0))`.
- `ctx.forget_image(uri)` on close to free GPU memory.
- Zoom clamp `[0.25, 10.0]`. +/- step 1.25×. Wheel step
  `raw_scroll_delta * 0.005`.

`image_view_modal.open` is **never set to true anywhere in
`rumble-egui`**, so this UI is unreachable in this crate as-is. It is
the design that rumble-next reuses in its lightbox.

---

## 13. Modals (application-level dialogs)

All are `egui::Modal` (single-screen overlays), not floating windows.

| Modal | Where | Trigger |
|---|---|---|
| First-run identity setup | `app.rs:2817-3243` | Auto on launch when `KeyManager::needs_setup()` |
| Connect | `app.rs:4769-4799` | Server → Connect... |
| Untrusted certificate | `app.rs:4660-4767` | `state.connection == CertificatePending` |
| Settings | `app.rs:4801-4979` | Toolbar gear / Settings menu |
| Rename room | `app.rs:4982-5014` | Room context menu |
| Edit room description | `app.rs:5016-5051` | Room context menu |
| Move-room confirmation | `app.rs:5054-5091` | Drag-drop in tree |
| Delete-room confirmation | `app.rs:5094-5133` | Room context menu |
| Kick user | `app.rs:5136-5169` | User context menu |
| Ban user | `app.rs:5172-5220` | User context menu |
| Elevate (sudo) | `app.rs:5223-5252` | 🔑 toolbar button |
| Delete-group confirmation | `app.rs:5254-5285` | Admin settings |
| Room ACL editor | `app.rs:5287-5387` | Room context menu |
| Image-view (lightbox, latent) | `app.rs:5389-5530` | unreachable in egui crate |

### 13.1 Connect — see §3.1.

### 13.2 Untrusted certificate — see §4.2.

### 13.3 Rename room

Single text field. Sends `Command::RenameRoom { room_id, new_name }`.

### 13.4 Edit room description

4-row multiline `TextEdit`. Sends
`Command::SetRoomDescription { room_id, description }`.

### 13.5 Move-room confirmation

Confirms a tree-drag reparenting; sends `Command::MoveRoom`.

### 13.6 Elevate (sudo)

Width 280px. **Properly masked** password field
(`TextEdit::singleline(...).password(true)`). Buttons **Elevate**
(sends `Command::Elevate { password }`, closes) and **Cancel**.

### 13.7 Kick

Width adapted to contents. Reason text input, red **Kick** button →
`Command::KickUser { target_user_id, reason }`. State:
`KickModalState { open, target_user_id, target_username, reason }`.

### 13.8 Ban

Reason text input + duration ComboBox indexed into `BAN_DURATIONS`:

- *Permanent* (0s).
- *1 hour* (3600).
- *1 day* (86400).
- *1 week* (604800).
- *30 days* (2592000).

Red **Ban** → `Command::BanUser { target_user_id, reason,
duration_seconds }`.

### 13.9 Delete-room confirmation

Red **Delete** button → `Command::DeleteRoom { room_id }`.

### 13.10 Room ACL editor

Header *"ACLs: <room name>"*.

- **Inherit from parent** checkbox → `inherit_acl: bool`.
- For each ACL entry (in order):
  - Group ComboBox populated from `state.group_definitions`.
  - **Here** checkbox (`apply_here`).
  - **Subs** checkbox (`apply_subs`).
  - **X** small button to remove the entry.
  - **Grant:** row with the compact 10-permission checklist
    (Traverse, Enter, Speak, Text, Files, Mute, Move, Create Rm, Mod
    Rm, Edit ACL — server-scoped flags are NOT exposed in the ACL
    editor; only the 10 room-scoped ones).
  - **Deny:** row with the same checklist.
- **+ Add Entry** appends a new entry: `group = "default"`, no
  grants/denies, both apply flags true.
- **Save** → `Command::SetRoomAcl { room_id, inherit_acl, entries:
  Vec<RoomAclEntry { group, grant, deny, apply_here, apply_subs }> }`.

**No reorder UI** (no up/down arrows) and **no per-entry tooltip beyond
the checkbox hover** — the design doc calls for these, but they're not
implemented.

### 13.11 Delete group

Modal: *"Are you sure you want to delete group '<name>'?"* → sends
`Command::DeleteGroup { name }`.

---

## 14. Settings Modal

`app.rs:4802-4979`.

### 14.1 Layout

- `egui::Modal` with min size 600×400, max 800×600. Does **not**
  auto-close on click-outside.
- Header: *"Settings"*.
- Left sidebar (`SidePanel::left`, 130px, non-resizable): vertical list
  of `SettingsCategory` labels.
- Central panel: scrollable; renders all selected categories in
  canonical order, separated by `ui.separator()`.
- Footer (`TopBottomPanel::bottom("settings_footer")`): yellow
  *"Changes not yet applied"* indicator if `dirty == true`, plus
  three buttons: **Apply**, **Cancel**, **Close**.

### 14.2 Multi-select

Plain click selects only that category. **Ctrl+click toggles** category
membership in a `HashSet<SettingsCategory>` so multiple panels can be
rendered concatenated.

### 14.3 Pending state buffer (`SettingsModalState`)

`app.rs:402-431`. Fields:

- `selected_categories: HashSet<SettingsCategory>`.
- `pending_settings: AudioSettings` (encoder/jitter knobs).
- `pending_input_device: Option<Option<String>>` (outer `Some(None)` =
  Default).
- `pending_output_device`.
- `pending_voice_mode`.
- `pending_autoconnect`.
- `pending_username`.
- `pending_tx_pipeline: PipelineConfig`.
- `pending_show_timestamps`, `pending_timestamp_format`.
- `dirty: bool`.
- `hotkey_capture_target: Option<HotkeyCaptureTarget>`.
- `hotkey_conflict_pending: Option<(target, binding, name)>`.

### 14.4 Apply / Cancel / Close semantics

- **Apply** — only enabled while `dirty`. Calls
  `apply_pending_settings()` (pushes commands to the backend) then
  `save_settings()` (writes `settings.json`) then toast
  *"Settings saved"*.
- **Cancel** — discards `SettingsModalState` (reverts pending values),
  closes modal.
- **Close** — like Apply if dirty, then closes.

### 14.5 Bypass paths

Some settings write directly to `persistent_settings` and call `save()`
on every change (do NOT participate in the pending/dirty flow):

- **Sounds** panel (every checkbox / slider).
- **File Transfer** panel (writes directly but still flips `dirty` so
  Apply persists; the writes themselves don't `save()` — Apply does).
- **Voice mode dropdown** in the toolbar (saves immediately).
- **Quick mute / deafen** buttons in the Voice settings panel (send
  commands immediately, no pending state).

### 14.6 Categories

`SettingsCategory` (`app.rs:344-392`):
`Connection`, `Devices`, `Voice`, `Sounds`, `Processing`, `Encoder`,
`Chat`, `FileTransfer`, `Keyboard`, `Statistics`, `Admin`. The **Admin**
category is hidden from the sidebar unless
`state.effective_permissions` contains `Permissions::MANAGE_ACL`.

Each category is detailed in §15-§24.

---

## 15. Settings — Connection

`render_settings_connection`, `app.rs:1273-1375`. See also §3, §4.

- **Server / status** read-out: address (display of `connect_address`)
  and *"Connected"* / *"Disconnected"*.
- **Username** field — pending; applied on Apply.
- **Autoconnect on launch** checkbox — pending; on Apply, flips
  `autoconnect_on_launch` and updates `auto_connect_addr`.
- **Identity sub-panel** — see §2.3.

The connect address and password are NOT exposed in the settings modal
— only via the Connect... dialog.

---

## 16. Settings — Devices (audio I/O)

`render_settings_devices`, `app.rs:1378-1506`.

- **🔄 Refresh Devices** — `Command::RefreshAudioDevices` (immediate).
- **Input Device (Microphone)** — ComboBox listing devices plus a
  *Default* entry. Each entry uses a label combining `name`, optional
  `pipeline` (e.g. ALSA endpoint), and a `(default)` marker. Pending.
- **Output Device (Speakers)** — same structure. Pending.
- **Input level meter** — 200×16 horizontal bar showing live input
  level in dB; color-coded (green ≤ −12 dB, yellow ≤ −3 dB, red > −3 dB
  / clipping). If a VAD processor is enabled in the pending pipeline,
  draws a vertical white line at its `threshold_db`. Numeric
  *"{:.0} dB"* value to the right.

There is **no** sample-rate / buffer-size picker (Opus is fixed at
48 kHz / 20 ms internally) and **no** loopback monitor button.

Apply emits `Command::SetInputDevice` / `Command::SetOutputDevice`.

---

## 17. Settings — Voice

`render_settings_voice`, `app.rs:1509-1612`.

- **Voice Mode** selector — two `selectable_label` toggles:
  - **🎤 Push-to-Talk** (hover *"Hold SPACE to transmit"*).
  - **📡 Continuous** (hover advises enabling VAD processor for voice
    activation).
- **Quick mute** — immediate `Command::SetMuted`.
- **Quick deafen** — immediate `Command::SetDeafened`.
- **Status read-out**: *"Muted"* / PTT-or-Continuous hint, deafened
  banner, green *"🎤 Transmitting…"* if `state.audio.is_transmitting`.

---

## 18. Settings — Sounds (SFX)

`render_settings_sounds`, `app.rs:1615-1676`. **Writes directly,
bypasses pending state.**

- **Enable sound effects** checkbox.
- **Volume** slider 0–100% with `%` suffix.
- For each `SfxKind` (`UserJoin`, `UserLeave`, `Connect`, `Disconnect`,
  `Mute`, `Unmute`, `Message`):
  - Checkbox using `kind.label()`.
  - **Preview** button → `Command::PlaySfx { kind, volume:
    max(volume, 0.3) }` regardless of disable state.

There is no file picker for custom SFX — sounds are built-in only.
There is no separate "talk start" SFX.

---

## 19. Settings — Processing (audio pipeline editor)

`render_settings_processing`, `app.rs:1679-1790`.

- **TX Pipeline** — iterates `pending_tx_pipeline.processors` in order.
  For each processor row:
  - Enable/disable checkbox labelled with `display_name` from
    `ProcessorRegistry`; hover tooltip = description.
  - When enabled, indented sub-section with a dynamic settings form
    generated from the processor's JSON schema (via
    `render_schema_field` at `app.rs:3244-3327`):
    - `number` → `egui::Slider` with min/max from schema.
    - `integer` → integer slider.
    - `boolean` → checkbox.
    - else → text input.
  - Each field shows `title` from schema and `description` as hover.

Built-in processors (registered by `register_builtin_processors` in
`crates/rumble-client/src/processors/mod.rs:34-38`):

- `builtin.gain` — volume adjustment (default ON).
- `builtin.denoise` — RNNoise (default ON).
- `builtin.vad` — voice activity detection (default OFF).

Order is fixed by `DEFAULT_TX_PIPELINE` (Gain → Denoise → VAD).
**There is no drag-reorder UI** and **no preset system** — the only
"preset" is `build_default_tx_pipeline()`.

The input level meter (with VAD threshold line) is duplicated below the
pipeline editor for convenient threshold tuning.

There is no RX pipeline editor, although `state.audio.rx_pipeline_defaults`
exists in state.

Apply sends `Command::UpdateTxPipeline { config: PipelineConfig }`.

---

## 20. Settings — Encoder

`render_settings_encoder`, `app.rs:1793-1897`.

- **Enable Forward Error Correction** — checkbox, hover *"Add
  redundancy for packet loss recovery"*.
- **Encoder Bitrate** — four `selectable_label` buttons: 24 kbps (LOW)
  / 32 kbps (MEDIUM) / 64 kbps (HIGH, default) / 96 kbps (VERY_HIGH).
- **Encoder Complexity** — 0–10 slider, hover *"Higher = better quality
  but more CPU"*.
- **Jitter Buffer Delay** — 1–10 packet slider; below shows
  *"Playback delay: ~{n*20}ms"*.
- **Expected Packet Loss** — 0–25 % slider.

Apply sends `Command::UpdateAudioSettings { settings: AudioSettings {
bitrate, encoder_complexity, jitter_buffer_delay_packets, fec_enabled,
packet_loss_percent } }`.

---

## 21. Settings — Chat

`render_settings_chat`, `app.rs:1959-2010`.

- **Show timestamps** — checkbox.
- **Timestamp format** — ComboBox enabled only when timestamps are
  shown. Options match `TimestampFormat::all()` (see §12.6).

There is no theme / font scale / density / appearance panel. Theme is
egui's built-in default; styling overrides are done inline.

---

## 22. Settings — File Transfer

`render_settings_file_transfer`, `app.rs:2012-2186`.

- **Enable auto-download** — checkbox; gates the rules table.
- **Auto-download rules table** — striped 3-column grid:
  - **MIME Pattern** — single-line text edit, e.g. `image/*`.
  - **Max Size** — `DragValue` 0–1000 MB; 0 = disabled for that pattern.
  - **Remove** button per row.
  - **Add Rule** button below appends a new row (default 10 MB, empty
    pattern).
  - Defaults: `image/* ≤ 10 MB`, `audio/* ≤ 50 MB`, `text/* ≤ 1 MB`.
- **Download limit** — `DragValue` in KB/s, range 0–100 000; 0 =
  *"(unlimited)"*. Stored internally in bytes.
- **Upload limit** — same.
- **Continue seeding after download** — checkbox.
- **Clean up downloaded files on exit** — checkbox.
- **Chat History** sub-section:
  - **Auto-sync history on room join** — when enabled, every
    `Command::JoinRoom` is followed by `Command::RequestChatHistory`.

There is no download-directory picker. The bandwidth caps and seed /
cleanup options are present in `rumble-egui` even though
`rumble-next` hides them (per `docs/rumble-next-bringup.md`) because
the `rumble-next` relay plugin doesn't enforce them yet.

---

## 23. Settings — Keyboard / Hotkeys

`render_settings_keyboard`, `app.rs:2189-2647`.

### 23.1 Wayland-portal vs fallback header

- `is_wayland = std::env::var("XDG_SESSION_TYPE") == "wayland"`.
- `portal_active = is_wayland && portal_hotkeys_available`.

Three branches:

- **Wayland + portal available** — section *"Global Hotkeys (via XDG
  Portal)"* with an info paragraph and a read-only grid of currently-
  bound shortcuts as reported by the portal
  (`portal_shortcuts: Vec<ShortcutInfo>`, each `description` +
  `trigger_description`). Unconfigured shortcuts render as gray
  *"(not configured)"*. **Configure in System Settings...** button calls
  `rumble_desktop_shell::hotkeys::portal::open_shortcut_settings()`.
  An additional *"Window-Focused Keys"* section provides the in-app
  capture UI as a fallback.
- **Wayland without portal** — yellow warning frame: *"Global shortcuts
  aren't available on this Wayland compositor… The keys below work only
  when Rumble's window is focused. Supported compositors: KDE Plasma
  5.27+, GNOME 47+, Hyprland."*
- **Non-Wayland (X11/Win/Mac)** — section *"Global Hotkeys"* with an
  *"Enable global hotkeys"* checkbox + grey notice *"Note: Changes to
  global hotkeys require restarting the application."*

### 23.2 Per-action rows (PTT / Toggle Mute / Toggle Deafen)

Each row shows:

- A colored status dot — green (`Registered`), red (`Failed`), gray
  (`NotConfigured`). Hidden in Portal mode (would be misleading).
- Bold action label.
- Binding display (e.g. `Ctrl+Shift+Space`).
- **Change** button.
- **Clear** button (only if a binding exists).

Clicking **Change** switches the row to a highlighted blue capture
frame: *"Press the desired key combination, then release. Press Escape
to cancel."* Capture reads `egui::Event::Key` events and translates with
`HotkeyManager::egui_key_to_string`.

Bindings are stored as
`HotkeyBinding { modifiers: HotkeyModifiers { ctrl, shift, alt, super_key },
key: String }` in `KeyboardSettings { ptt_hotkey, toggle_mute_hotkey,
toggle_deafen_hotkey, global_hotkeys_enabled }`. Default PTT is `Space`.

### 23.3 Conflict warning

When the user captures a key already bound to another action, an orange
warning frame: *"This key is already bound to {Other}. Setting it here
will remove the other binding."* with **Apply anyway** (clears the
conflicting binding and applies new) and **Cancel** buttons.

### 23.4 Hotkey actions exposed

Only **PTT**, **ToggleMute**, **ToggleDeafen**. No bindings for
"join room", "open settings", "next/prev channel", etc.

### 23.5 Hotkey hints in the main shell

The main toolbar/menu does **not** display hotkey reminders. Tooltips
just say *"Mute"* / *"Unmute"* without listing the bound key.

---

## 24. Settings — Statistics

`render_settings_statistics`, `app.rs:1900-1956`. The only diagnostics
surface today.

- **Audio Statistics** — read-only 2-column grid of live
  `state.audio.stats` fields:
  - Actual Bitrate (kbps).
  - Avg Frame Size (bytes).
  - Packets Sent.
  - Packets Received.
  - Packet Loss — color-coded (green ≤ 1%, yellow ≤ 5%, red > 5%) +
    absolute count.
  - FEC Recovered.
  - Frames Concealed.
  - Buffer Level (packets).
- **Reset Statistics** button → `Command::ResetAudioStats`.

There is **no** log viewer, **no** general-purpose mic test, and **no**
About / Version page in settings — the version banner is only emitted
as a chat `LocalMessage` at startup (*"Rumble Client v{CARGO_PKG_VERSION}"*).

---

## 25. Settings — Admin (gated on `MANAGE_ACL`)

`render_settings_admin`, `app.rs:2649-2813`. Hidden from the sidebar
unless `state.effective_permissions` contains `Permissions::MANAGE_ACL`.

### 25.1 Group list

3-column striped grid (Group / Permissions / Actions). Each row:

- Group name.
- Permission summary via `format_permission_summary` (e.g.
  *"Traverse, Enter, Speak"* or *"5 permissions"* if more than 4); full
  list available as hover-text via `format_permission_details`.
- Per-row **Edit** and **Delete** buttons. Built-in groups (`default`,
  `admin`) show *"(built-in)"* in place of Delete.

### 25.2 Edit-group inline form

Shows `render_permission_checkboxes` — a grouped checklist with two
sub-groups:

- **Room-Scoped**: Traverse, Enter, Speak, Text Message, Share File,
  Mute/Deafen Others, Move User, Make Room, Modify Room, Edit ACL.
- **Server-Scoped**: Kick, Ban, Register Others, Self Register, Manage
  ACLs, Sudo.

Buttons: **Save** → `Command::ModifyGroup { name, permissions }`;
**Cancel**.

### 25.3 Create group

Name text input + the same room-/server-scoped checkbox grid +
**+ Create Group** button → `Command::CreateGroup { name, permissions }`.

### 25.4 Delete group

Confirmation modal (see §13.11) → `Command::DeleteGroup { name }`.

### 25.5 User group memberships

For each user in `state.users`, two rows:

- *"<username>: <comma-separated groups or '(none)'>"*.
- ComboBox of all group names (default selection persisted per user in
  `admin_panel.user_group_selection: HashMap<u64, String>`, pruned for
  disconnected users) + **+ Add** (or **− Remove** if already a
  member) button → `Command::SetUserGroup { target_user_id, group, add,
  expires_at: 0 }`.

The proto supports `expires_at` per `acl-ui-plan.md`, but the UI today
always sends `expires_at: 0` (permanent). Timed memberships are only
configurable via the Ban modal.

---

## 26. Permission Flags

From `crates/rumble-protocol/src/permissions.rs`. 16 flags total.

- **Room-scoped (10)**: TRAVERSE, ENTER, SPEAK, TEXT_MESSAGE,
  SHARE_FILE, MUTE_DEAFEN, MOVE_USER, MAKE_ROOM, MODIFY_ROOM, WRITE.
- **Server-scoped (6)**: KICK, BAN, REGISTER, SELF_REGISTER, MANAGE_ACL,
  SUDO.

WRITE is room-scoped (edit room ACLs), not "implies all"; the `admin`
group just has all bits set. Username-as-group: every registered user
has an implicit group equal to their username; group names must not
collide with usernames. See `MEMORY.md` ACL section for details.

---

## 27. SFX

`crates/rumble-protocol/src/types.rs:907-942` defines `SfxKind`:

- `UserJoin`, `UserLeave` — fire when a user enters/leaves the local
  user's current room.
- `Connect`, `Disconnect` — connection state transitions (see §3.6).
- `Mute`, `Unmute` — own self-mute transitions.
- `Message` — new chat message arrived (see §12.7).

All driven by `play_sfx(kind)` (`app.rs:994-1004`), which sends
`Command::PlaySfx { kind, volume }` after consulting
`persistent_settings.sfx { enabled, volume, disabled_sounds }`.

---

## 28. Drag-and-Drop

Two surfaces:

- **Tree drag-drop** — re-parent rooms (see §8.5). Confirmed via the
  Move-Room modal.
- **Window drag-drop** — share dropped files (see §10.4) with the
  full-window blue overlay (§10.5).

---

## 29. Persistence

### 29.1 Two stores running side by side

- **`PersistentSettings`** — egui-only — `<config_dir>/settings.json`.
  Defined in `crates/rumble-egui/src/settings.rs:262-306`.
- **`SettingsStore` / `Settings`** — shared between rumble-egui and
  rumble-next — `<config_dir>/desktop-shell.json`. Defined in
  `crates/rumble-desktop-shell/src/settings.rs:357-397`. Holds
  `recent_servers`, `accepted_certificates`, `auto_connect_addr`.
  Round-trips unknown fields via `_extra: HashMap<String, Value>` for
  forward-compat.

Both are written when settings are saved. Cert acceptance writes both;
recent-server tracking writes only the shared store.

### 29.2 `<config_dir>/identity.json`

Independent file managed by `KeyManager` — see §2.2.

### 29.3 Resolution

`directories::ProjectDirs::from("com", "rumble", "Rumble")`, overridable
via `--config-dir <path>`.

### 29.4 `PersistentSettings` schema (canonical)

```rust
pub struct PersistentSettings {
    pub server_address: String,
    pub server_password: String,
    pub trust_dev_cert: bool,
    pub custom_cert_path: Option<String>,
    pub client_name: String,
    pub accepted_certificates: Vec<AcceptedCertificate>, // {server_name, fingerprint_hex, certificate_der_base64}
    pub identity_private_key_hex: Option<String>, // legacy migration only
    pub autoconnect_on_launch: bool,
    pub audio: PersistentAudioSettings, // {bitrate, encoder_complexity, jitter_buffer_delay_packets, fec_enabled, packet_loss_percent, tx_pipeline: Option<PipelineConfig>}
    pub voice_mode: PersistentVoiceMode, // PushToTalk | Continuous
    pub input_device_id: Option<String>,
    pub output_device_id: Option<String>,
    pub show_chat_timestamps: bool,
    pub chat_timestamp_format: TimestampFormat, // Time24h | Time12h | DateTime24h | DateTime12h | Relative
    pub file_transfer: FileTransferSettings, // {auto_download_enabled, auto_download_rules, download_speed_limit, upload_speed_limit, seed_after_download, cleanup_on_exit, auto_sync_history}
    pub keyboard: KeyboardSettings, // {ptt_hotkey, toggle_mute_hotkey, toggle_deafen_hotkey, global_hotkeys_enabled}
    pub sfx: PersistentSfxSettings, // {enabled, volume: 0..1, disabled_sounds: HashSet<SfxKind>}
    // runtime-only:
    pub config_dir_override: Option<PathBuf>,
}
```

### 29.5 Chat-log retention

In-memory only. `state.chat_messages: Vec<ChatMessage>` is not persisted
across restarts. Cross-peer sync uses
`Command::RequestChatHistory` (see `crates/rumble-protocol/src/types.rs`
`CHAT_HISTORY_MIME` and `ChatHistoryRequestMessage`).

---

## 30. CLI Arguments

`crates/rumble-egui/src/settings.rs:21-57` (`Args`,
`#[derive(Parser, Debug, Clone, Default)]`).

| Flag | Type | Effect |
|---|---|---|
| `-s, --server <addr>` | `Option<String>` | Override persisted server address; auto-connects if first-run is complete. |
| `-n, --name <name>` | `Option<String>` | Override stored display name. |
| `-p, --password <pw>` | `Option<String>` | Override stored server password. |
| `--trust-dev-cert` | `bool`, default `true` | Trust `dev-certs/server-cert.der`. |
| `--cert <path>` | `Option<String>` | Trust an additional CA/server cert (DER or PEM). |
| `--rpc <command>` (Unix) | `Option<String>` | Send a single command to a running instance over Unix socket and exit. See §32. |
| `--rpc-socket <path>` | `Option<String>` | Override the default RPC socket path (`rumble_client::rpc::default_socket_path()`). |
| `--rpc-server` | `bool` | Start the RPC server inside the running GUI. |
| `--config-dir <path>` | `Option<String>` | Override platform-default config dir. |

Plus the env var `RUMBLE_SERVER_CERT_PATH` — additional cert path beyond
CLI/persisted settings.

CLI overrides do **not** mutate the persisted values.

---

## 31. TestHarness API

`crates/rumble-egui/src/harness.rs`. Library-mode entry point used by
the in-tree tests and (indirectly) by harness-cli. Two flavours selected
by Cargo feature `test-harness`:

- **With `test-harness`**: wraps `egui_kittest::Harness<'static,
  HarnessState>` — gives AccessKit-based widget queries.
- **Without**: wraps a bare `egui::Context` + `RumbleApp` and replays
  raw input.

Public surface (non-exhaustive — see file for full signatures):

- `TestHarness::new()` / `with_args(Args)` — build harness; spawns a
  1-worker tokio runtime, builds `RumbleApp`, wires it.
- `run()` — run frames until UI animations complete and no repaints
  pending.
- `run_frame()`, `run_frames(count)`.
- `kittest()` / `kittest_mut()` — access the inner kittest harness.
- `click_widget(label)` / `try_click_widget(label) -> bool` /
  `has_widget(label) -> bool` / `widget_rect(label) -> Option<Rect>`.
- `type_into_focused(text)`.
- `key_press(Key)` / `key_release(Key)` / `key_tap(Key)`.
- `click(Pos2)` (full press+release).
- `mouse_move(Pos2)`.
- `type_text(&str)` (one-step variant of `type_into_focused`).
- `app() -> &RumbleApp` / `app_mut() -> &mut RumbleApp` — direct access.
- `is_connected() -> bool`.
- `ctx() -> &egui::Context`.
- `runtime() -> &tokio::runtime::Runtime`.
- `output() -> &FullOutput` (kittest only — used by daemon for
  screenshots and AccessKit updates).

Tests in `crates/rumble-egui/tests/hotkey_tests.rs` exercise:

- Default PTT key configuration.
- `HotkeyManager::key_string_to_egui_key` / `egui_key_to_string`
  round-trip.
- `HotkeyBinding::display()`.
- PTT / mute / deafen key paths via `kittest_mut().key_down/up`.
- `app_mut().handle_hotkey_event(HotkeyEvent::*)` and asserting
  `backend().state().audio.{self_muted, self_deafened}` doesn't
  change while disconnected.

A port must reproduce: a label-queryable widget surface, raw-input
synthesis, idle-until-stable loop, and direct app access.

---

## 32. RPC Interface

`crates/rumble-egui/src/rpc_client.rs`. Client side; the matching
server lives in `rumble_client::rpc` (gated `#[cfg(unix)]`) and is
opt-in via `--rpc-server`.

Transport: connect to a Unix socket, write one line of JSON, read one
line back, print, exit. Default socket
`rumble_client::rpc::default_socket_path()`; overridable with
`--rpc-socket`.

| CLI string | JSON body | Purpose |
|---|---|---|
| `status` | `{"method":"get_status"}` | Status query. |
| `state` | `{"method":"get_state"}` | Full backend state dump. |
| `mute` / `unmute` | `{"method":"set_muted","muted":<bool>}` | Toggle self-mute. |
| `deafen` / `undeafen` | `{"method":"set_deafened","deafened":<bool>}` | Toggle self-deafen. |
| `disconnect` | `{"method":"disconnect"}` | Drop the connection. |
| `start-transmit` / `stop-transmit` | `{"method":"start_transmit"}` / `{"method":"stop_transmit"}` | External PTT. |
| `join-room <uuid>` | `{"method":"join_room","room_id":<uuid>}` | Move to room. |
| `send-chat <text>` | `{"method":"send_chat","text":<...>}` | Send chat message. |
| `create-room <name>` | `{"method":"create_room","name":<...>}` | Create room. |
| `delete-room <uuid>` | `{"method":"delete_room","room_id":<...>}` | Delete room. |
| `share-file <path>` | `{"method":"share_file","path":<...>}` | Share file via plugin. |
| `download <magnet>` | `{"method":"download_file","magnet":<...>}` | Download by magnet. |
| `mute-user <u64>` / `unmute-user <u64>` | `{"method":"mute_user","user_id":<...>}` etc | Per-user server-side mute. |

A port must reproduce both the CLI string→JSON mapping and the socket-
path resolution.

---

## 33. harness-cli Integration

`crates/harness-cli/`. Binary name `rumble-harness`. Daemon-based test
driver.

The daemon (`daemon::Daemon`) holds a `HashMap<u32, ClientInstance>`,
optional `ServerHandle`, `next_client_id: AtomicU32`. Default socket:
`$XDG_RUNTIME_DIR/rumble-harness.sock`.

`ClientInstance` directly instantiates `rumble_egui::RumbleApp` (it
does **not** use `TestHarness`). It owns its own `tokio::runtime::Runtime`,
its own `egui::Context` (with `enable_accesskit()` set), a fixed
1280×720 viewport, an `egui_kittest::wgpu::WgpuTestRenderer`, and a
`kittest::State` updated from `output.platform_output.accesskit_update`
each frame.

### 33.1 CLI subcommands

Top level: `daemon`, `server`, `client`, `status`, `up`, `down`,
`iterate`. Global `--socket <path>`.

- `daemon start [--background]` / `daemon stop` / `daemon status`.
- `server start [--port 5000]` (daemon spawns `server::Server` with a
  self-signed cert + `server::FileTransferRelayPlugin`) / `server stop`.
- `client new [--name] [--server]` / `client list` / `client close <id>`.
- `client screenshot <id> [--output] [--crop x,y,w,h]` — frame, render
  to PNG via `image::codecs::png::PngEncoder`, optional crop via
  `image::imageops::crop_imm`.
- `client click <id> <x> <y>` / `mouse-move` / `key-press` / `key-release`
  / `key-tap` / `type` / `frames` / `connected` / `state`.
- `client click-widget <id> <label>` / `has-widget <id> <label>` /
  `widget-rect <id> <label>` — AccessKit-based queries (using
  `kittest::State::root()` + `query_by_label`).
- `client run <id>` — `run_until_stable` (max 500 frames, 3 consecutive
  stable frames; "stable" = `viewport_output.repaint_delay >= 1s`).
- `client set-auto-download` / `set-auto-download-rules` /
  `get-file-transfer-settings` — currently stubbed.
- `client set-hotkey <id> <ptt|mute|deafen> [key]` — mutates
  `app.persistent_settings_mut().keyboard.*`.
- `client share-file <id> <path>` — currently returns
  *"File sharing not yet available in harness"*.
- `client get-file-transfers` / `show-transfers <id> <bool>` —
  `app.set_show_transfers(show)`.

Top-level shortcuts:

- `up` — start daemon → server → client, optionally screenshot.
- `down` — close clients → stop server → shut daemon.
- `iterate` — close client → `cargo build -p rumble-egui` → reopen
  client → wait for stabilize → screenshot.

### 33.2 Wire protocol

`crates/harness-cli/src/protocol.rs`. `Command` is a tagged enum with
`#[serde(tag = "type", rename_all = "snake_case")]`; `Response` is
`{status: ok|error, data | message}`.

### 33.3 What a port must expose

A port that wants to keep harness-cli intact only needs to re-implement
the `RumbleApp` surface that `ClientInstance` uses:

- `RumbleApp::new(ctx, runtime_handle, args)`.
- `app.render(ctx)`.
- `app.is_connected()`.
- `app.backend().send(Command::*)`.
- `app.backend().state()`.
- `app.persistent_settings_mut()`.
- `app.set_show_transfers(bool)`.

Plus the same `accesskit_update` egress for label queries.

---

## 34. State Surface (BackendHandle / State)

These are the fields a UI port consumes.

### 34.1 `state.connection: ConnectionState`

5 variants: `Disconnected`, `Connecting { server_addr }`,
`Connected { server_name, user_id }`, `ConnectionLost { error }`,
`CertificatePending { cert_info }`.

### 34.2 `state.audio: AudioState`

- `self_muted`, `self_deafened`, `is_transmitting`.
- `voice_mode: VoiceMode` (PushToTalk | Continuous).
- `talking_users: HashSet<u64>`.
- `muted_users: HashSet<u64>` (locally muted).
- `per_user_rx: HashMap<u64, PerUserRx { volume_db, ... }>`.
- `selected_input`, `selected_output`, `input_devices`,
  `output_devices: Vec<AudioDeviceInfo { id, name, pipeline,
  is_default }>`.
- `tx_pipeline: PipelineConfig`, `rx_pipeline_defaults`.
- `settings: AudioSettings`.
- `stats: AudioStats { actual_bitrate_bps, avg_frame_size_bytes,
  packets_sent, packets_received, packets_lost,
  packets_recovered_fec, frames_concealed, playback_buffer_packets }`.
- `input_level_db: Option<f32>`.

### 34.3 Rooms / users

- `rooms: HashMap<Uuid, Room>`.
- `room_tree: { nodes, roots, ancestors, children }`.
- `users: Vec<User { user_id, username, current_room, is_muted,
  is_deafened, server_muted, is_elevated, groups: Vec<String> }>`.
- `users_in_room(uuid)` helper.
- `my_room_id: Option<Uuid>`, `my_user_id: Option<u64>`.

### 34.4 Permissions

- `effective_permissions: u32` (current room).
- `per_room_permissions: HashMap<Uuid, u32>`.
- `permission_denied: Option<String>` (consumed/taken per frame).
- `kicked: Option<String>` (consumed/taken per frame).

### 34.5 Chat

- `chat_messages: Vec<ChatMessage>`.

### 34.6 ACL groups

- `group_definitions: Vec<GroupInfo { name, permissions, is_builtin }>`.

---

## 35. Commands Surface (`rumble_client::Command`)

Variants the UI emits today:

- `Connect`, `Disconnect`, `AcceptCertificate`, `RejectCertificate`.
- `JoinRoom`, `RequestChatHistory`, `LocalMessage`.
- `SendChat`, `SendDirectMessage`, `SendTreeChat`.
- `StartTransmit`, `StopTransmit`.
- `SetMuted`, `SetDeafened`, `SetVoiceMode`.
- `MuteUser`, `UnmuteUser`, `SetUserVolume`, `SetServerMute`.
- `KickUser`, `BanUser`, `RegisterUser`, `UnregisterUser`, `Elevate`.
- `CreateRoom`, `RenameRoom`, `MoveRoom`, `DeleteRoom`,
  `SetRoomDescription`, `SetRoomAcl`.
- `CreateGroup`, `ModifyGroup`, `DeleteGroup`, `SetUserGroup`.
- `RefreshAudioDevices`, `SetInputDevice`, `SetOutputDevice`,
  `UpdateAudioSettings`, `UpdateTxPipeline`, `ResetAudioStats`.
- `ShareFile`, `PlaySfx`.

`Command::DownloadFile { share_data }` is defined in the protocol but
is **not invoked anywhere in `rumble-egui`** — confirms there is no
incoming-file-offer UI here.

---

## 36. rumble-widgets (parallel widget kit, used by rumble-next)

Located at `crates/rumble-widgets/`. Custom widget set with three
concrete themes and a token system. Modules:

| Module | Purpose |
|---|---|
| `tokens.rs` | Theme-agnostic primitives — colors / spacing / text roles. |
| `theme.rs` | `Theme` trait + `Ui` extension methods. |
| `pressable.rs` | Base button primitive. |
| `surface.rs` | Background surface primitive. |
| `combo_box.rs` | Drop-down. |
| `group_box.rs` | Bordered group. |
| `level_meter.rs` | VU meter. |
| `presence.rs` | User presence indicator. |
| `radio.rs` | Radio button. |
| `slider.rs` | Slider. |
| `text_input.rs` | Text input. |
| `toggle.rs` | On/off toggle. |
| `tree.rs` | Drag-and-drop tree (room hierarchy). |
| `modern.rs` | `ModernTheme`. |
| `mumble.rs` | `MumbleLiteTheme`. |
| `luna.rs` | `LunaTheme`, with widget overrides for pixel-snapping. |
| `gallery.rs` | demo gallery used by `bin/gallery.rs`. |

`rumble-widgets` is **not** consumed by `rumble-egui` — egui uses stock
widgets. It is consumed by `rumble-next`. For an aetna port the
question is whether to keep this widget kit (and re-skin it on aetna)
or rebuild it from scratch.

---

## 37. Known Gaps / Latent Code (port wishlist)

These are NOT user-visible features today, but represent intent or
half-built scaffolding. Worth knowing during the port.

- **No unlock-encrypted-key UI** — `LocalEncrypted` keys can be
  persisted, but there is no place to enter the password on launch.
  `connect()` will fail with a misleading
  *"Please unlock it in settings."*
- **No saved-server picker UI** — `recent_servers` is populated and
  used as a default address only; no list view, no rename, no
  switch-to dropdown.
- **No password masking in connect modal** — the server password field
  is plain `text_edit_singleline`.
- **No identity-import UI** — only legacy migration imports a key.
- **Connect dialog has no "Save as favorite" / label option** even
  though `RecentServer.label` exists.
- **No transfers window in `rumble-egui`** — `show_transfers` toggles
  for external consumers; no internal renderer.
- **No file-offer rendering in chat** — `ChatMessage::attachment` is on
  the wire and parsed but ignored by the chat panel. (rumble-next
  renders it.)
- **Image-view modal is unreachable** in `rumble-egui` — fully
  implemented but never opened from any code path.
- **No drag-reorder for processors**, **no preset system**.
- **No RX pipeline editor** despite `state.audio.rx_pipeline_defaults`.
- **No room-ACL entry reorder UI** (no up/down arrows).
- **No timed group memberships** — `expires_at` always 0 from the UI;
  Ban modal is the only place that sets a duration.
- **No theme / appearance / density / font-scale page**.
- **No About / Version page** in settings.
- **No log viewer / network diag / mic test**; only audio stats.
- **No tray icon, no minimize-to-tray, no always-on-top**.
- **No `/me` or other slash commands beyond `/msg` and `/tree`**.
- **No `@mention`, no link parsing, no markdown, no code blocks**, no
  per-message context menu, no edit / delete / react.
- **No unread badges in the room tree, no jump-to-bottom button**.
- **No talk-start SFX** — only `Mute`/`Unmute`/`Connect`/`Disconnect`/
  `UserJoin`/`UserLeave`/`Message`.
- **No hotkey bindings beyond PTT / ToggleMute / ToggleDeafen**.
- **`fonts/NotoColorEmoji-Regular.ttf` is shipped but unused** by the
  egui binary.
- **Bandwidth caps / seed / cleanup-on-exit settings exist in
  `rumble-egui` but aren't enforced by the relay plugin** — see
  `docs/rumble-next-bringup.md`.

---

## 38. File Reference Index

Absolute paths to the canonical sources:

- `crates/rumble-egui/src/app.rs` — main UI (5535 lines).
- `crates/rumble-egui/src/main.rs` — eframe runner.
- `crates/rumble-egui/src/lib.rs` — re-exports.
- `crates/rumble-egui/src/settings.rs` — `Args`, `PersistentSettings`,
  `TimestampFormat`, `FileTransferSettings`, `AcceptedCertificate`.
- `crates/rumble-egui/src/first_run.rs` — first-run state machine.
- `crates/rumble-egui/src/harness.rs` — `TestHarness`.
- `crates/rumble-egui/src/rpc_client.rs` — RPC client CLI.
- `crates/rumble-egui/Cargo.toml`.
- `crates/rumble-egui/fonts/NotoColorEmoji-Regular.ttf` (unused).
- `crates/rumble-egui/tests/hotkey_tests.rs`.
- `crates/rumble-desktop-shell/src/identity/key_manager.rs`.
- `crates/rumble-desktop-shell/src/hotkeys/mod.rs`.
- `crates/rumble-desktop-shell/src/toasts.rs`.
- `crates/rumble-desktop-shell/src/settings.rs` — `SettingsStore`,
  `RecentServer`, `auto_connect_addr`.
- `crates/rumble-protocol/src/types.rs` — `ChatMessage`, `ChatAttachment`,
  `FileOfferInfo`, `Command`, `ConnectionState`, `SfxKind`,
  `CHAT_HISTORY_MIME`.
- `crates/rumble-protocol/src/permissions.rs`.
- `crates/rumble-client/src/handle.rs` — backend command loop.
- `crates/rumble-client/src/processors/mod.rs` — built-in processor
  registration / default TX pipeline.
- `crates/harness-cli/src/main.rs`, `daemon.rs`, `protocol.rs`,
  `renderer.rs`.
- `crates/rumble-widgets/src/lib.rs` — module index.
- `docs/acl-ui-plan.md` — ACL UI design.
- `docs/rumble-next-bringup.md` — known parity gaps.
- `docs/v2-architecture.md` — backend platform abstraction.
- `MEMORY.md` — ACL / bridge / project notes.

---

## 39. Suggested Port Plan (informational)

A reasonable order for an aetna port, in light of the inventory:

1. **Bring up the runtime composition** (§1.3) — env-filter, tokio
   runtime, hotkey manager on the main thread, image decoders, portal
   init order. Don't try to render anything yet.
2. **Static state plumbing** — get `RumbleApp::new(ctx, runtime, args)`
   compiling against aetna's context type; wire up `BackendHandle` and
   the repaint callback.
3. **First-run + Connect + TOFU modal** — no main window UI yet; just
   prove the auth/identity surface. (§2-§4)
4. **Top-level layout shell** (§5) and **Connection-status pill** (§3.5)
   so transition feedback works.
5. **Room tree + user list** (§8, §9) with stub context menus.
6. **Voice toolbar + hotkeys** (§7, §10, §15) — get talking.
7. **Chat panel** (§12) — match rumble-next's expanded chat UX (file
   cards, lightbox) rather than rumble-egui's sparse renderer.
8. **Settings modal scaffold** (§14) with one panel at a time:
   Connection (§15), Devices (§16), Voice (§17), Sounds (§18),
   Encoder (§20), Statistics (§24), Chat (§21).
9. **Audio pipeline editor** (§19) — JSON-schema-driven forms.
10. **File transfer settings + transfers window** (§22, §6.3) — and
    actually wire `show_transfers` to a renderer this time.
11. **Hotkey settings with capture + portal integration** (§23).
12. **Admin panel + room ACL editor + user kick/ban/elevate** (§25,
    §13.7-§13.10, §7.4) — biggest single piece, leave for last.
13. **Test harness + RPC + harness-cli integration** (§31-§33) — these
    matter for CI/agent loops; reproduce the input/AccessKit query
    surface.
14. **Tackle the known-gap wishlist** (§37) opportunistically.
