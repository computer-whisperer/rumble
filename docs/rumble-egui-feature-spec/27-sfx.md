# §27 — SFX

`crates/rumble-protocol/src/types.rs:907-942` defines `SfxKind`:

- `UserJoin`, `UserLeave` — fire when a user enters/leaves the local
  user's current room.
- `Connect`, `Disconnect` — connection state transitions (see §3.6).
- `Mute`, `Unmute` — own self-mute transitions.
- `Message` — new chat message arrived (see §12.7).

All driven by `play_sfx(kind)` (`app.rs:994-1004`), which sends
`Command::PlaySfx { kind, volume }` after consulting
`persistent_settings.sfx { enabled, volume, disabled_sounds }`.
