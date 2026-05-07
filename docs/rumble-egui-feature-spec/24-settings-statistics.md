# §24 — Settings — Statistics

`render_settings_statistics`, `app.rs:1900-1956`. The only diagnostics
surface today.

- **Audio Statistics** — read-only 2-column grid of live
  `state.audio.stats` fields:
  - Actual Bitrate (kbps).
  - Avg Frame Size (bytes).
  - Packets Sent.
  - Packets Received.
  - Packet Loss — color-coded (green ≤ 1%, yellow ≤ 5%, red > 5%) +
    absolute count.
  - FEC Recovered.
  - Frames Concealed.
  - Buffer Level (packets).
- **Reset Statistics** button → `Command::ResetAudioStats`.

There is **no** log viewer, **no** general-purpose mic test, and **no**
About / Version page in settings — the version banner is only emitted
as a chat `LocalMessage` at startup (*"Rumble Client v{CARGO_PKG_VERSION}"*).
