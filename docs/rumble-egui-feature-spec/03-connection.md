# §3 — Connection / Servers

## 3.1 Connect dialog

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

## 3.2 `connect()` flow

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

## 3.3 Auto-connect on launch

Triggers (mutually exclusive with first-run gate):

- `--server <addr>` non-empty AND first-run not needed → connect
  immediately.
- `persistent_settings.autoconnect_on_launch == true` AND
  `connect_address` is non-empty AND first-run not needed → emit
  *"Auto-connecting..."* and connect.

## 3.4 Server menu

`app.rs:3634-3652`.

- **Connect...** — opens connect modal.
- **Disconnect** (only when connected) — sends `Command::Disconnect`.
- **Reconnect** (only when not connected and `connect_address`
  non-empty) — `RumbleApp::reconnect()` posts *"Reconnecting to
  {addr}..."* and re-runs `connect()`.

`Drop` impl on `RumbleApp` sends `Command::Disconnect` if still
connected, ensuring clean shutdown.

## 3.5 Connection-status pill (top-right of menu bar)

Reads `state.connection: ConnectionState` (5 variants). At
`app.rs:3704-3725`.

- **Disconnected** → gray *"○ Disconnected"*.
- **Connecting { server_addr }** → yellow *"⚫ Connecting..."*.
- **Connected { server_name, user_id }** → green *"⚫ Connected"*.
- **ConnectionLost { error }** → red *"⚫ Connection Lost: {error}"* +
  **⟳ Reconnect** button.
- **CertificatePending { cert_info }** → yellow
  *"⚠ Certificate Verification"*.

## 3.6 Connection state-transition effects

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

## 3.7 Recent servers (latent — populated, not surfaced)

`shared_settings.recent_servers: Vec<RecentServer { addr, label,
username, last_used_unix }>` is populated on every successful connect
and used to pre-fill the connect dialog with the most-recently-used
server (sorted by `last_used_unix`). **No user-visible saved-server
picker, no rename, no "switch to" dropdown today.** The `label` field is
defined but never set or displayed.

## 3.8 Auto-connect address (shared store)

`shared_settings.auto_connect_addr: Option<String>` tracks which recent
server to auto-connect to. It is set/cleared inside
`remember_shared_server` based on `autoconnect_on_launch`.
