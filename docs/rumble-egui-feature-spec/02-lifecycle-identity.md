# §2 — Application Lifecycle & First Run

## 2.1 First-run identity wizard

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

## 2.2 Identity storage (`KeyManager`)

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

## 2.3 Settings → Connection: Identity panel

`app.rs:1333-1374`. The in-settings surface for identity:

- **Public Key** — truncated `xxxxxxxx...xxxxxxxx` in a code block, plus
  a 📋 copy button (full hex via `ui.ctx().copy_text`).
- **Storage** — human label: *Local (unencrypted)*, *Local (password
  protected)*, or *SSH Agent ({comment})*.
- **🔑 Generate New Identity...** — re-enters first-run flow at
  `SelectMethod`. *No confirmation prompt; will replace the current key.*
- When `KeyManager` has no config (shouldn't happen normally): yellow
  *"⚠ No identity key configured"* + **🔑 Configure Identity...** button.

## 2.4 No identity-import UI

`KeyManager::import_signing_key` is only used for legacy migration. There
is no user-facing import-by-paste or import-from-file UI today.
