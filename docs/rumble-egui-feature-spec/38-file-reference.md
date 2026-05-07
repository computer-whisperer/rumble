# §38 — File Reference Index

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
