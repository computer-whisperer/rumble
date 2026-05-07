# §21 — Settings — Chat

`render_settings_chat`, `app.rs:1959-2010`.

- **Show timestamps** — checkbox.
- **Timestamp format** — ComboBox enabled only when timestamps are
  shown. Options match `TimestampFormat::all()` (see §12.6):
  - `Time24h` — *"24-hour (14:30:05)"*.
  - `Time12h` — *"12-hour (2:30:05 PM)"*.
  - `DateTime24h`, `DateTime12h`.
  - `Relative` — *"5m ago"*.

There is no theme / font scale / density / appearance panel. Theme is
egui's built-in default; styling overrides are done inline.
