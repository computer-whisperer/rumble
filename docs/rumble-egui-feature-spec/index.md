# rumble-egui Feature Specification — Index

This folder is a section-by-section split of the original `rumble-egui-feature-spec.md`.

Each file covers one major numbered section. The spec exists so that a port to a different
GUI library (e.g. *aetna*) can be done with eyes open: every user-visible feature, every
latent stub, and every rumble-next parity gap is inventoried here.

Conventions (apply to all files):
- File paths are absolute from the repo root.
- Line references are to `crates/rumble-egui/src/app.rs` unless noted.
- "Backend" / "BackendHandle" refers to `rumble_client::handle::BackendHandle`.
- "Local UI state" lives on `RumbleApp` itself (not in the backend).
- "Commands" are variants of `rumble_client::Command` that the UI emits.

---

## Sections

| File | Section | Title |
|---|---|---|
| [01-top-level-structure.md](01-top-level-structure.md) | §1 | Top-Level Structure |
| [02-lifecycle-identity.md](02-lifecycle-identity.md) | §2 | Application Lifecycle & First Run |
| [03-connection.md](03-connection.md) | §3 | Connection / Servers |
| [04-certificate-trust.md](04-certificate-trust.md) | §4 | Certificate Trust (TOFU) |
| [05-window-layout.md](05-window-layout.md) | §5 | Window Layout |
| [06-menu-bar.md](06-menu-bar.md) | §6 | Top Menu Bar |
| [07-toolbar.md](07-toolbar.md) | §7 | Toolbar |
| [08-room-tree.md](08-room-tree.md) | §8 | Room Tree (Central Panel) |
| [09-user-list.md](09-user-list.md) | §9 | User List (inline in tree) |
| [10-voice-path.md](10-voice-path.md) | §10 | Voice Path |
| [11-toasts.md](11-toasts.md) | §11 | Toasts |
| [12-chat-panel.md](12-chat-panel.md) | §12 | Chat Panel |
| [13-modals.md](13-modals.md) | §13 | Modals (application-level dialogs) |
| [14-settings-modal.md](14-settings-modal.md) | §14 | Settings Modal (layout & shared mechanics) |
| [15-settings-connection.md](15-settings-connection.md) | §15 | Settings — Connection |
| [16-settings-devices.md](16-settings-devices.md) | §16 | Settings — Devices (audio I/O) |
| [17-settings-voice.md](17-settings-voice.md) | §17 | Settings — Voice |
| [18-settings-sounds.md](18-settings-sounds.md) | §18 | Settings — Sounds (SFX) |
| [19-settings-processing.md](19-settings-processing.md) | §19 | Settings — Processing (audio pipeline editor) |
| [20-settings-encoder.md](20-settings-encoder.md) | §20 | Settings — Encoder |
| [21-settings-chat.md](21-settings-chat.md) | §21 | Settings — Chat |
| [22-settings-file-transfer.md](22-settings-file-transfer.md) | §22 | Settings — File Transfer |
| [23-settings-keyboard.md](23-settings-keyboard.md) | §23 | Settings — Keyboard / Hotkeys |
| [24-settings-statistics.md](24-settings-statistics.md) | §24 | Settings — Statistics |
| [25-settings-admin.md](25-settings-admin.md) | §25 | Settings — Admin |
| [26-permissions.md](26-permissions.md) | §26 | Permission Flags |
| [27-sfx.md](27-sfx.md) | §27 | SFX |
| [28-drag-drop.md](28-drag-drop.md) | §28 | Drag-and-Drop |
| [29-persistence.md](29-persistence.md) | §29 | Persistence |
| [30-cli-args.md](30-cli-args.md) | §30 | CLI Arguments |
| [31-test-harness.md](31-test-harness.md) | §31 | TestHarness API |
| [32-rpc.md](32-rpc.md) | §32 | RPC Interface |
| [33-harness-cli.md](33-harness-cli.md) | §33 | harness-cli Integration |
| [34-state-surface.md](34-state-surface.md) | §34 | State Surface (BackendHandle / State) |
| [35-commands-surface.md](35-commands-surface.md) | §35 | Commands Surface |
| [36-rumble-widgets.md](36-rumble-widgets.md) | §36 | rumble-widgets |
| [37-known-gaps.md](37-known-gaps.md) | §37 | Known Gaps / Latent Code |
| [38-file-reference.md](38-file-reference.md) | §38 | File Reference Index |
| [39-port-plan.md](39-port-plan.md) | §39 | Suggested Port Plan |
