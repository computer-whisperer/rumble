# §9 — User List (inline in tree)

`app.rs:4264-4514`. Each user leaf shows, left-to-right:

## 9.1 Mic icon (priority order)

1. **🔒 red** — server-muted (highest priority).
2. **🎤 green** — talking (or the local user when
   `state.audio.is_transmitting`).
3. **🎤 dark-red** — self-muted.
4. **🎤 dark-gray** — idle.

## 9.2 Other status icons

- **🔇 dark-red** — user is deafened.
- **🔕 yellow** — locally muted (only shown for other users).
- **🛡 gold** — elevated/superuser, hover *"Elevated (Superuser)"*.

## 9.3 Username

Plain `Label`. **No avatars, no per-user color, no AFK indicator.**

## 9.4 Self context menu

Header rows: User name, ID, current room, optional Groups.

- **Mute / Unmute** — `Command::SetMuted`.
- **Deafen / Undeafen** — `Command::SetDeafened`.
- **Register / Unregister** — gated on `SELF_REGISTER`.
  `Command::RegisterUser` / `UnregisterUser` for own user.

## 9.5 Other-user context menu

- **🔊 Volume** slider, -40..=20 dB, 1 dB step → `Command::SetUserVolume
  { user_id, volume_db }`.
- **Reset Volume** button.
- **🔕 Mute Locally / 🔔 Unmute Locally** — `Command::MuteUser` /
  `UnmuteUser`.
- **🔒 Server Mute / 🔒 Remove Server Mute** — gated on `MUTE_DEAFEN`.
  `Command::SetServerMute`.
- **⚡ Kick** — gated on `KICK`. Opens kick modal.
- **🚫 Ban** — gated on `BAN`. Opens ban modal.
- **📝 Register / ❌ Unregister** — gated on `REGISTER`.

State touched: `state.audio.muted_users`, `state.audio.per_user_rx`,
`state.per_room_permissions`.
