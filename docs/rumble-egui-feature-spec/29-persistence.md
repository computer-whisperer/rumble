# §29 — Persistence

## 29.1 Two stores running side by side

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

## 29.2 `<config_dir>/identity.json`

Independent file managed by `KeyManager` — see §2.2.

## 29.3 Resolution

`directories::ProjectDirs::from("com", "rumble", "Rumble")`, overridable
via `--config-dir <path>`.

## 29.4 `PersistentSettings` schema (canonical)

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

## 29.5 Chat-log retention

In-memory only. `state.chat_messages: Vec<ChatMessage>` is not persisted
across restarts. Cross-peer sync uses
`Command::RequestChatHistory` (see `crates/rumble-protocol/src/types.rs`
`CHAT_HISTORY_MIME` and `ChatHistoryRequestMessage`).
