# Rumble icons

SVGs lifted directly from
`/home/ben/Documents/programmingPlay/mumble/mumble/themes/Default/`
(LGPL-3.0-or-later, same license as Mumble itself; redistributable
under the GPL-2.0+ in the rumble project).

These are not yet wired into the renderer. Aetna's `icon()` builder
takes an `IconName` enum, which is closed inside `aetna-core`. To use
custom SVG icons we need to either:

1. **Extend the vendored aetna `IconName`.** Add new variants in
   `vendor/aetna/crates/aetna-core/src/tree/types.rs::IconName`, parse
   the SVG body into the matching `&[IconStroke]` plus
   `parse_current_color_svg_asset(...)` `VectorAsset`, and add the
   variant to `all_icon_names()` / `icon_strokes()` / `icon_path()` in
   `aetna-core/src/icons.rs`. This is the most straightforward path —
   every icon now flows through the same `text_color`-tinted pipeline as
   `IconName::Folder` / `IconName::Users`.

2. **Add an open-ended `register_icon(name, svg)` API to aetna.** The
   internals already cache `VectorAsset`s in a `OnceLock<Vec<...>>` so
   this is feasible but requires a real change to aetna's public
   surface.

Approach (1) is what we'll pursue first if `rumble-aetna` reaches
enough parity to need them. Until then we use aetna's stock lucide-ish
vocabulary (`Folder`, `Users`, `Activity`, `AlertCircle`, …).

## Index

- `talking_on.svg` / `talking_off.svg` — speaking indicators (replace stock `Activity`).
- `muted_self.svg` / `muted_pushtomute.svg` / `muted_suppressed.svg` — mic-off variants.
- `muted_server.svg` — server-imposed mute (red lock icon).
- `muted_local.svg` — locally muted other user (yellow bell).
- `deafened_self.svg` / `deafened_server.svg` / `self_undeafened.svg` — deafen states.
- `channel.svg` / `channel_active.svg` — room/channel folder glyphs.
- `authenticated.svg` — registered-user indicator.
- `priority_speaker.svg` — priority-speaker badge.
- `filter_on.svg` / `filter_off.svg` — chat/user-list filter toggle.
- `disconnect.svg` — disconnect button.
- `comment.svg` / `comment_seen.svg` — chat / unread indicator.
- `lock_locked.svg` / `lock_unlocked.svg` — ACL / privacy.
- `arrow_left.svg` — back / collapse.
- `pin.svg` — pinned message / room.
