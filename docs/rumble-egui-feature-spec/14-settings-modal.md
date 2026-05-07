# §14 — Settings Modal (layout & shared mechanics)

`app.rs:4802-4979`.

## 14.1 Layout

- `egui::Modal` with min size 600×400, max 800×600. Does **not**
  auto-close on click-outside.
- Header: *"Settings"*.
- Left sidebar (`SidePanel::left`, 130px, non-resizable): vertical list
  of `SettingsCategory` labels.
- Central panel: scrollable; renders all selected categories in
  canonical order, separated by `ui.separator()`.
- Footer (`TopBottomPanel::bottom("settings_footer")`): yellow
  *"Changes not yet applied"* indicator if `dirty == true`, plus
  three buttons: **Apply**, **Cancel**, **Close**.

## 14.2 Multi-select

Plain click selects only that category. **Ctrl+click toggles** category
membership in a `HashSet<SettingsCategory>` so multiple panels can be
rendered concatenated.

## 14.3 Pending state buffer (`SettingsModalState`)

`app.rs:402-431`. Fields:

- `selected_categories: HashSet<SettingsCategory>`.
- `pending_settings: AudioSettings` (encoder/jitter knobs).
- `pending_input_device: Option<Option<String>>` (outer `Some(None)` =
  Default).
- `pending_output_device`.
- `pending_voice_mode`.
- `pending_autoconnect`.
- `pending_username`.
- `pending_tx_pipeline: PipelineConfig`.
- `pending_show_timestamps`, `pending_timestamp_format`.
- `dirty: bool`.
- `hotkey_capture_target: Option<HotkeyCaptureTarget>`.
- `hotkey_conflict_pending: Option<(target, binding, name)>`.

## 14.4 Apply / Cancel / Close semantics

- **Apply** — only enabled while `dirty`. Calls
  `apply_pending_settings()` (pushes commands to the backend) then
  `save_settings()` (writes `settings.json`) then toast
  *"Settings saved"*.
- **Cancel** — discards `SettingsModalState` (reverts pending values),
  closes modal.
- **Close** — like Apply if dirty, then closes.

## 14.5 Bypass paths

Some settings write directly to `persistent_settings` and call `save()`
on every change (do NOT participate in the pending/dirty flow):

- **Sounds** panel (every checkbox / slider).
- **File Transfer** panel (writes directly but still flips `dirty` so
  Apply persists; the writes themselves don't `save()` — Apply does).
- **Voice mode dropdown** in the toolbar (saves immediately).
- **Quick mute / deafen** buttons in the Voice settings panel (send
  commands immediately, no pending state).

## 14.6 Categories

`SettingsCategory` (`app.rs:344-392`):
`Connection`, `Devices`, `Voice`, `Sounds`, `Processing`, `Encoder`,
`Chat`, `FileTransfer`, `Keyboard`, `Statistics`, `Admin`. The **Admin**
category is hidden from the sidebar unless
`state.effective_permissions` contains `Permissions::MANAGE_ACL`.

Each category is detailed in §15–§25.
