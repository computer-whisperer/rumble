# §12 — Chat Panel

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

## 12.1 Composer

- Single-line `TextEdit::singleline` bound to `chat_input`.
- Send triggers: Enter (focus-lost on press) or **Send** button.
- On send, clears the input.
- **No multiline support** (no Shift+Enter newline).
- Disabled with hint *"Connect to a server to chat"* when disconnected,
  or *"You don't have permission to chat in this room"* when lacking
  `Permissions::TEXT_MESSAGE`.

## 12.2 Slash commands

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

## 12.3 Paste-image button (📋)

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

## 12.4 Sync (request chat history) button

**↻ Sync** sends `Command::RequestChatHistory` to ask peers in the
current room for their chat history. The backend posts a local
*"Requesting chat history from peers..."* status line (see
`crates/rumble-client/src/handle.rs:1179-1188`).

## 12.5 History rendering

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

## 12.6 Optional timestamps

`persistent_settings.show_chat_timestamps == true` prefixes every line
with `[<formatted-time>] `. Format from `TimestampFormat::all()`:

- `Time24h` — *"24-hour (14:30:05)"*.
- `Time12h` — *"12-hour (2:30:05 PM)"*.
- `DateTime24h`, `DateTime12h`.
- `Relative` — *"5m ago"*.

## 12.7 New-message SFX

`SfxKind::Message` plays when `state.chat_messages.len()` increases
between frames and at least one new message has `is_local == false`.
Tracked via `prev_chat_count` (`app.rs:548`).

## 12.8 Message types & attachments (wire)

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

## 12.9 System messages from the client

Local-only `is_local=true` messages injected for: client banner
*"Rumble Client v…"*, client name, *"Connecting to <addr>..."*,
*"Auto-connecting..."*, *"Reconnecting to <addr>..."*, *"Settings
saved."* / *"Failed to save settings: …"*, key-status messages, slash-
command usage errors, *"Requesting chat history from peers..."*. Joins
/ leaves / topic changes / mutes are **not** written to chat — those
surface via tree state, toasts, and SFX.

## 12.10 Per-room vs DM

Single shared `Vec<ChatMessage>`. There is no per-DM tab or per-room
chat tab; DMs and tree broadcasts are interleaved with room chat and
distinguished only by prefix and color.

## 12.11 Latent: image-view modal

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
