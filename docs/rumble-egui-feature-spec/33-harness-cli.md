# §33 — harness-cli Integration

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

## 33.1 CLI subcommands

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

## 33.2 Wire protocol

`crates/harness-cli/src/protocol.rs`. `Command` is a tagged enum with
`#[serde(tag = "type", rename_all = "snake_case")]`; `Response` is
`{status: ok|error, data | message}`.

## 33.3 What a port must expose

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
