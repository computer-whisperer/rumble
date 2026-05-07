# §20 — Settings — Encoder

`render_settings_encoder`, `app.rs:1793-1897`.

- **Enable Forward Error Correction** — checkbox, hover *"Add
  redundancy for packet loss recovery"*.
- **Encoder Bitrate** — four `selectable_label` buttons: 24 kbps (LOW)
  / 32 kbps (MEDIUM) / 64 kbps (HIGH, default) / 96 kbps (VERY_HIGH).
- **Encoder Complexity** — 0–10 slider, hover *"Higher = better quality
  but more CPU"*.
- **Jitter Buffer Delay** — 1–10 packet slider; below shows
  *"Playback delay: ~{n*20}ms"*.
- **Expected Packet Loss** — 0–25 % slider.

Apply sends `Command::UpdateAudioSettings { settings: AudioSettings {
bitrate, encoder_complexity, jitter_buffer_delay_packets, fec_enabled,
packet_loss_percent } }`.
