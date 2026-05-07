# §36 — rumble-widgets (parallel widget kit, used by rumble-next)

Located at `crates/rumble-widgets/`. Custom widget set with three
concrete themes and a token system. Modules:

| Module | Purpose |
|---|---|
| `tokens.rs` | Theme-agnostic primitives — colors / spacing / text roles. |
| `theme.rs` | `Theme` trait + `Ui` extension methods. |
| `pressable.rs` | Base button primitive. |
| `surface.rs` | Background surface primitive. |
| `combo_box.rs` | Drop-down. |
| `group_box.rs` | Bordered group. |
| `level_meter.rs` | VU meter. |
| `presence.rs` | User presence indicator. |
| `radio.rs` | Radio button. |
| `slider.rs` | Slider. |
| `text_input.rs` | Text input. |
| `toggle.rs` | On/off toggle. |
| `tree.rs` | Drag-and-drop tree (room hierarchy). |
| `modern.rs` | `ModernTheme`. |
| `mumble.rs` | `MumbleLiteTheme`. |
| `luna.rs` | `LunaTheme`, with widget overrides for pixel-snapping. |
| `gallery.rs` | demo gallery used by `bin/gallery.rs`. |

`rumble-widgets` is **not** consumed by `rumble-egui` — egui uses stock
widgets. It is consumed by `rumble-next`. For an aetna port the
question is whether to keep this widget kit (and re-skin it on aetna)
or rebuild it from scratch.
