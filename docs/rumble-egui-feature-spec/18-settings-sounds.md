# §18 — Settings — Sounds (SFX)

`render_settings_sounds`, `app.rs:1615-1676`. **Writes directly,
bypasses pending state.**

- **Enable sound effects** checkbox.
- **Volume** slider 0–100% with `%` suffix.
- For each `SfxKind` (`UserJoin`, `UserLeave`, `Connect`, `Disconnect`,
  `Mute`, `Unmute`, `Message`):
  - Checkbox using `kind.label()`.
  - **Preview** button → `Command::PlaySfx { kind, volume:
    max(volume, 0.3) }` regardless of disable state.

There is no file picker for custom SFX — sounds are built-in only.
There is no separate "talk start" SFX.
