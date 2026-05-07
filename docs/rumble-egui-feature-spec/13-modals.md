# §13 — Modals (application-level dialogs)

All are `egui::Modal` (single-screen overlays), not floating windows.

| Modal | Where | Trigger |
|---|---|---|
| First-run identity setup | `app.rs:2817-3243` | Auto on launch when `KeyManager::needs_setup()` |
| Connect | `app.rs:4769-4799` | Server → Connect... |
| Untrusted certificate | `app.rs:4660-4767` | `state.connection == CertificatePending` |
| Settings | `app.rs:4801-4979` | Toolbar gear / Settings menu |
| Rename room | `app.rs:4982-5014` | Room context menu |
| Edit room description | `app.rs:5016-5051` | Room context menu |
| Move-room confirmation | `app.rs:5054-5091` | Drag-drop in tree |
| Delete-room confirmation | `app.rs:5094-5133` | Room context menu |
| Kick user | `app.rs:5136-5169` | User context menu |
| Ban user | `app.rs:5172-5220` | User context menu |
| Elevate (sudo) | `app.rs:5223-5252` | 🔑 toolbar button |
| Delete-group confirmation | `app.rs:5254-5285` | Admin settings |
| Room ACL editor | `app.rs:5287-5387` | Room context menu |
| Image-view (lightbox, latent) | `app.rs:5389-5530` | unreachable in egui crate |

## 13.1 Connect — see §3.1.

## 13.2 Untrusted certificate — see §4.2.

## 13.3 Rename room

Single text field. Sends `Command::RenameRoom { room_id, new_name }`.

## 13.4 Edit room description

4-row multiline `TextEdit`. Sends
`Command::SetRoomDescription { room_id, description }`.

## 13.5 Move-room confirmation

Confirms a tree-drag reparenting; sends `Command::MoveRoom`.

## 13.6 Elevate (sudo)

Width 280px. **Properly masked** password field
(`TextEdit::singleline(...).password(true)`). Buttons **Elevate**
(sends `Command::Elevate { password }`, closes) and **Cancel**.

## 13.7 Kick

Width adapted to contents. Reason text input, red **Kick** button →
`Command::KickUser { target_user_id, reason }`. State:
`KickModalState { open, target_user_id, target_username, reason }`.

## 13.8 Ban

Reason text input + duration ComboBox indexed into `BAN_DURATIONS`:

- *Permanent* (0s).
- *1 hour* (3600).
- *1 day* (86400).
- *1 week* (604800).
- *30 days* (2592000).

Red **Ban** → `Command::BanUser { target_user_id, reason,
duration_seconds }`.

## 13.9 Delete-room confirmation

Red **Delete** button → `Command::DeleteRoom { room_id }`.

## 13.10 Room ACL editor

Header *"ACLs: <room name>"*.

- **Inherit from parent** checkbox → `inherit_acl: bool`.
- For each ACL entry (in order):
  - Group ComboBox populated from `state.group_definitions`.
  - **Here** checkbox (`apply_here`).
  - **Subs** checkbox (`apply_subs`).
  - **X** small button to remove the entry.
  - **Grant:** row with the compact 10-permission checklist
    (Traverse, Enter, Speak, Text, Files, Mute, Move, Create Rm, Mod
    Rm, Edit ACL — server-scoped flags are NOT exposed in the ACL
    editor; only the 10 room-scoped ones).
  - **Deny:** row with the same checklist.
- **+ Add Entry** appends a new entry: `group = "default"`, no
  grants/denies, both apply flags true.
- **Save** → `Command::SetRoomAcl { room_id, inherit_acl, entries:
  Vec<RoomAclEntry { group, grant, deny, apply_here, apply_subs }> }`.

**No reorder UI** (no up/down arrows) and **no per-entry tooltip beyond
the checkbox hover** — the design doc calls for these, but they're not
implemented.

## 13.11 Delete group

Modal: *"Are you sure you want to delete group '<name>'?"* → sends
`Command::DeleteGroup { name }`.
