# §16 — Settings — Devices (audio I/O)

`render_settings_devices`, `app.rs:1378-1506`.

- **🔄 Refresh Devices** — `Command::RefreshAudioDevices` (immediate).
- **Input Device (Microphone)** — ComboBox listing devices plus a
  *Default* entry. Each entry uses a label combining `name`, optional
  `pipeline` (e.g. ALSA endpoint), and a `(default)` marker. Pending.
- **Output Device (Speakers)** — same structure. Pending.
- **Input level meter** — 200×16 horizontal bar showing live input
  level in dB; color-coded (green ≤ −12 dB, yellow ≤ −3 dB, red > −3 dB
  / clipping). If a VAD processor is enabled in the pending pipeline,
  draws a vertical white line at its `threshold_db`. Numeric
  *"{:.0} dB"* value to the right.

There is **no** sample-rate / buffer-size picker (Opus is fixed at
48 kHz / 20 ms internally) and **no** loopback monitor button.

Apply emits `Command::SetInputDevice` / `Command::SetOutputDevice`.
