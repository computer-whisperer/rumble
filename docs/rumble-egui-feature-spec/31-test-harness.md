# §31 — TestHarness API

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
