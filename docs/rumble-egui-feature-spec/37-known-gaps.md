# §37 — Known Gaps / Latent Code (port wishlist)

These are NOT user-visible features today, but represent intent or
half-built scaffolding. Worth knowing during the port.

- **No unlock-encrypted-key UI** — `LocalEncrypted` keys can be
  persisted, but there is no place to enter the password on launch.
  `connect()` will fail with a misleading
  *"Please unlock it in settings."*
- **No saved-server picker UI** — `recent_servers` is populated and
  used as a default address only; no list view, no rename, no
  switch-to dropdown.
- **No password masking in connect modal** — the server password field
  is plain `text_edit_singleline`.
- **No identity-import UI** — only legacy migration imports a key.
- **Connect dialog has no "Save as favorite" / label option** even
  though `RecentServer.label` exists.
- **No transfers window in `rumble-egui`** — `show_transfers` toggles
  for external consumers; no internal renderer.
- **No file-offer rendering in chat** — `ChatMessage::attachment` is on
  the wire and parsed but ignored by the chat panel. (rumble-next
  renders it.)
- **Image-view modal is unreachable** in `rumble-egui` — fully
  implemented but never opened from any code path.
- **No drag-reorder for processors**, **no preset system**.
- **No RX pipeline editor** despite `state.audio.rx_pipeline_defaults`.
- **No room-ACL entry reorder UI** (no up/down arrows).
- **No timed group memberships** — `expires_at` always 0 from the UI;
  Ban modal is the only place that sets a duration.
- **No theme / appearance / density / font-scale page**.
- **No About / Version page** in settings.
- **No log viewer / network diag / mic test**; only audio stats.
- **No tray icon, no minimize-to-tray, no always-on-top**.
- **No `/me` or other slash commands beyond `/msg` and `/tree`**.
- **No `@mention`, no link parsing, no markdown, no code blocks**, no
  per-message context menu, no edit / delete / react.
- **No unread badges in the room tree, no jump-to-bottom button**.
- **No talk-start SFX** — only `Mute`/`Unmute`/`Connect`/`Disconnect`/
  `UserJoin`/`UserLeave`/`Message`.
- **No hotkey bindings beyond PTT / ToggleMute / ToggleDeafen**.
- **`fonts/NotoColorEmoji-Regular.ttf` is shipped but unused** by the
  egui binary.
- **Bandwidth caps / seed / cleanup-on-exit settings exist in
  `rumble-egui` but aren't enforced by the relay plugin** — see
  `docs/rumble-next-bringup.md`.
