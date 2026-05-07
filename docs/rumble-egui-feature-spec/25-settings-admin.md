# §25 — Settings — Admin (gated on `MANAGE_ACL`)

`render_settings_admin`, `app.rs:2649-2813`. Hidden from the sidebar
unless `state.effective_permissions` contains `Permissions::MANAGE_ACL`.

## 25.1 Group list

3-column striped grid (Group / Permissions / Actions). Each row:

- Group name.
- Permission summary via `format_permission_summary` (e.g.
  *"Traverse, Enter, Speak"* or *"5 permissions"* if more than 4); full
  list available as hover-text via `format_permission_details`.
- Per-row **Edit** and **Delete** buttons. Built-in groups (`default`,
  `admin`) show *"(built-in)"* in place of Delete.

## 25.2 Edit-group inline form

Shows `render_permission_checkboxes` — a grouped checklist with two
sub-groups:

- **Room-Scoped**: Traverse, Enter, Speak, Text Message, Share File,
  Mute/Deafen Others, Move User, Make Room, Modify Room, Edit ACL.
- **Server-Scoped**: Kick, Ban, Register Others, Self Register, Manage
  ACLs, Sudo.

Buttons: **Save** → `Command::ModifyGroup { name, permissions }`;
**Cancel**.

## 25.3 Create group

Name text input + the same room-/server-scoped checkbox grid +
**+ Create Group** button → `Command::CreateGroup { name, permissions }`.

## 25.4 Delete group

Confirmation modal (see §13.11) → `Command::DeleteGroup { name }`.

## 25.5 User group memberships

For each user in `state.users`, two rows:

- *"<username>: <comma-separated groups or '(none)'>"*.
- ComboBox of all group names (default selection persisted per user in
  `admin_panel.user_group_selection: HashMap<u64, String>`, pruned for
  disconnected users) + **+ Add** (or **− Remove** if already a
  member) button → `Command::SetUserGroup { target_user_id, group, add,
  expires_at: 0 }`.

The proto supports `expires_at` per `acl-ui-plan.md`, but the UI today
always sends `expires_at: 0` (permanent). Timed memberships are only
configurable via the Ban modal.
