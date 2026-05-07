# §17 — Settings — Voice

`render_settings_voice`, `app.rs:1509-1612`.

- **Voice Mode** selector — two `selectable_label` toggles:
  - **🎤 Push-to-Talk** (hover *"Hold SPACE to transmit"*).
  - **📡 Continuous** (hover advises enabling VAD processor for voice
    activation).
- **Quick mute** — immediate `Command::SetMuted`.
- **Quick deafen** — immediate `Command::SetDeafened`.
- **Status read-out**: *"Muted"* / PTT-or-Continuous hint, deafened
  banner, green *"🎤 Transmitting…"* if `state.audio.is_transmitting`.
