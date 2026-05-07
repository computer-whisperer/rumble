# §1 — Top-Level Structure

## 1.1 Crate layout

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

## 1.2 Window / viewport

Defined in `main.rs:59-64`.

- Title: `"Rumble"`.
- Initial inner size: 1000 × 700.
- Minimum inner size: 800 × 500.
- No tray icon, no minimize-to-tray, no always-on-top.

## 1.3 Runtime composition (port checklist)

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

## 1.4 Logging

All tracing goes through `tracing_subscriber::fmt()` with the
`RUST_LOG`-driven env filter plus the clipboard suppression directive.
User-facing errors are surfaced two ways:

- **Toasts** via `ToastManager` (auto-hiding overlays, see §11).
- **Local-only chat lines** via `Command::LocalMessage { text }`
  (rendered inline in the chat panel as gray italic system messages).
