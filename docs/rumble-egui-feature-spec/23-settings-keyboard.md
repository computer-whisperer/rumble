# §23 — Settings — Keyboard / Hotkeys

`render_settings_keyboard`, `app.rs:2189-2647`.

## 23.1 Wayland-portal vs fallback header

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

## 23.2 Per-action rows (PTT / Toggle Mute / Toggle Deafen)

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

## 23.3 Conflict warning

When the user captures a key already bound to another action, an orange
warning frame: *"This key is already bound to {Other}. Setting it here
will remove the other binding."* with **Apply anyway** (clears the
conflicting binding and applies new) and **Cancel** buttons.

## 23.4 Hotkey actions exposed

Only **PTT**, **ToggleMute**, **ToggleDeafen**. No bindings for
"join room", "open settings", "next/prev channel", etc.

## 23.5 Hotkey hints in the main shell

The main toolbar/menu does **not** display hotkey reminders. Tooltips
just say *"Mute"* / *"Unmute"* without listing the bound key.
