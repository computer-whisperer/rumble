# §7 — Toolbar

`app.rs:3730-3879`. Order, left-to-right:

## 7.1 Self-mute button

- **Server-muted**: red 🔒, hover *"Server muted - you cannot speak in
  this room"*, non-clickable.
- **Self-muted**: 🔇 red, hover *"Unmute"*.
- **Live**: 🎤 green, hover *"Mute"*.
- Click → `Command::SetMuted { muted }`.
- State: `state.audio.self_muted`, plus own user's `server_muted`.

## 7.2 Self-deafen button

- 🔇 red (deafened) or 🔊 green (hearing).
- Click → `Command::SetDeafened { deafened }`.
- State: `state.audio.self_deafened`.

## 7.3 Voice-mode dropdown

ComboBox showing 🎤 PTT or 📡 Continuous. Selecting persists immediately
(does not require Apply). Sends `Command::SetVoiceMode`.

## 7.4 Elevate (sudo) button

Visible only when connected and the local user is not already elevated.
🔑 button → opens the Elevate modal (see §13.6). After elevation, the
button disappears and the user gains a 🛡 badge in the user list.

## 7.5 Settings gear

⚙ button on the right. Snapshots audio/connection state into
`SettingsModalState::pending_*` fields and opens the settings modal.

## 7.6 Implicit / latent

There is **no** toolbar transmit indicator (mic flash). Self-transmit is
visible only as the green mic icon next to your username in the user
list.
