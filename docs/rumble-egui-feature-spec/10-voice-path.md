# §10 — Voice Path

## 10.1 Self transmit indicator

Driven by `state.audio.is_transmitting`. Visible only via the green mic
icon next to the local user in the tree.

## 10.2 PTT — global hotkey path

`HotkeyManager::poll_events()` is drained every frame in
`EframeWrapper::update`; events route to
`RumbleApp::handle_hotkey_event(...)` (`app.rs:1162-1205`).

- `HotkeyEvent::PttPressed` → `Command::StartTransmit`.
- `HotkeyEvent::PttReleased` → `Command::StopTransmit`.
- `HotkeyEvent::ToggleMute` → `Command::SetMuted { !current }`.
- `HotkeyEvent::ToggleDeafen` → `Command::SetDeafened { !current }`.

Works when the window is unfocused (Win/macOS/X11 via `global-hotkey`;
Wayland via the XDG portal — see §23).

## 10.3 PTT — window-focused fallback

`app.rs:3496-3534`. Each frame, polls the configured PTT key via
`ctx.input(|i| i.key_down(...))`. Suppressed when a text input has
focus (`ctx.wants_keyboard_input()`). Releases on key-up.

The toggle-mute / toggle-deafen hotkeys have an analogous fallback at
`app.rs:3537-3591` using `key_pressed` semantics.

## 10.4 Drag-and-drop file sharing (window-level)

Files dropped onto the window iterate
`ctx.input(|i| i.raw.dropped_files.clone())` and send
`Command::ShareFile { path }` per file. Sets `show_transfers = true`.

## 10.5 Drag-hover overlay

While files are being dragged over the window, a translucent blue
full-window overlay (`Color32::from_rgba_unmultiplied(0,100,200,100)`)
is drawn with centered heading *"Drop files here to share"*. Gated on
`state.connection.is_connected()`.
