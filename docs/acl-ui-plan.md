# ACL Admin UI Implementation Plan

## Overview

Expose all ACL features to the GUI and refactor the server to use a cleaner state-sync model. Key architectural changes from the initial ACL sprint:

1. **Room ACLs ride on RoomInfo** — no separate query messages
2. **Group definitions in ServerState** — client evaluates permissions locally
3. **BANNED permission replaces separate ban system** — bans are just timed group memberships
4. **Ban is a client-side workflow** — chat message + group change + kick, no special server ban logic
5. **User group memberships synced to admins** — full sync, no round-trips

No backwards compatibility needed — no deployed servers or clients.

---

## Architecture Changes

### Remove (from initial ACL sprint)

- `bans` sled tree, `BanEntry` struct, `add_ban/remove_ban/get_ban/list_bans/is_banned` functions
- `BanUser`, `UnbanUser` proto messages and server handlers
- `QueryPermissions`, `PermissionsInfo` proto messages and handlers (client evaluates locally)
- `ban_list` state field in client backend
- `effective_permissions` state field (replaced by local evaluation)
- Ban check in auth handler (replaced by BANNED permission check)

### Add

**New permission flag:**
```rust
const BANNED = 0x400000;  // Server-scoped: prevents connection
```

**Timed group membership:**
- `SetUserGroup` proto message gets `uint64 expires_at = 4;` field (unix timestamp, 0 = no expiry)
- User-group persistence entries get `expires_at: Option<u64>`
- Server checks expiry on auth and during permission evaluation; expired memberships treated as absent

**State sync extensions:**
- `RoomInfo` proto: add `bool inherit_acl = 5;` and `repeated RoomAclEntry acls = 6;`
- `User` proto: add `repeated string groups = 8;` (populated for all users if viewer has MANAGE_ACL; always populated for self)
- New `GroupInfo` proto message for state sync
- `ServerState` proto: add `repeated GroupInfo groups` field
- New `StateUpdate` variants: `GroupChanged`, `UserGroupChanged`

### Modify

**Auth handler:** Instead of checking ban list, compute effective permissions for connecting user at root. If BANNED is present, reject.

**State sync:** `build_user_list()` populates `groups` field per-client based on viewer permissions. `build_server_state()` includes group definitions.

**Client-side permission eval:** Client imports `api::permissions::effective_permissions()` and evaluates locally using synced group definitions + room ACLs + own group list. No server round-trips for UI gating.

---

## Protocol Changes

### Modified Messages

```protobuf
message RoomInfo {
  RoomId id = 1;
  string name = 2;
  RoomId parent_id = 3;
  string description = 4;
  bool inherit_acl = 5;              // NEW
  repeated RoomAclEntry acls = 6;    // NEW (reuses existing message)
}

message User {
  UserId user_id = 1;
  string username = 2;
  RoomId current_room = 3;
  bool is_muted = 4;
  bool is_deafened = 5;
  bool server_muted = 6;
  bool is_elevated = 7;
  repeated string groups = 8;        // NEW
}

message SetUserGroup {
  uint64 target_user_id = 1;
  string group = 2;
  bool add = 3;
  uint64 expires_at = 4;             // NEW: unix timestamp, 0 = no expiry
}
```

### New Messages

```protobuf
// Group definition (for state sync)
message GroupInfo {
  string name = 1;
  uint32 permissions = 2;
  bool is_builtin = 3;
}

// State update: group created/modified/deleted
message GroupChanged {
  GroupInfo group = 1;
  bool deleted = 2;
}

// State update: user's group membership changed
message UserGroupChanged {
  uint64 user_id = 1;
  string group = 2;
  bool added = 3;
  uint64 expires_at = 4;
}
```

### New Payload Fields

```protobuf
// In StateUpdate oneof:
GroupChanged group_changed = 20;
UserGroupChanged user_group_changed = 21;

// In ServerState:
repeated GroupInfo groups = N;  // (find next available field number)
```

### Removed Messages

- `BanUser`, `UnbanUser` — replaced by SetUserGroup with "banned" group
- `QueryPermissions`, `PermissionsInfo` — replaced by client-side evaluation
- `BanList`, `ListBans`, `BanInfo` — never existed, no longer needed

---

## Ban Workflow (Client-Side)

**Ban is a client-side orchestration of existing primitives:**

1. Admin right-clicks user → "Ban" → dialog opens
2. Dialog fields:
   - Reason text input
   - Group selector (default: user's username-group, or a "banned" group)
   - Duration picker (permanent / 1h / 1d / 1w / 30d / custom)
3. On confirm, the admin's client sends (in order):
   a. `ChatMessage` to the banned user: "You have been banned: {reason}"
   b. `ModifyGroup(username_group, permissions | BANNED)` — set BANNED flag on chosen group
   c. `SetUserGroup(target, group, add=true, expires_at=timestamp)` — if using timed membership
   d. `KickUser(target, reason)` — disconnect them
4. Server handles each message independently. No special "ban" handler.

**Unban:** Admin removes BANNED flag from the group via ModifyGroup, or removes user from the group via SetUserGroup.

**Auth rejection:** Server computes effective permissions at root during auth. If BANNED flag is present, reject with "You are banned" message.

---

## UI Components

### 1. Kick Dialog (in user context menu)

Right-click user → "Kick" (requires KICK permission):
- Text: "Kick {username}?"
- Reason input (optional, single line)
- [Cancel] [Kick] buttons
- Sends `KickUser { target_user_id, reason }`

### 2. Ban Dialog (in user context menu)

Right-click user → "Ban" (requires MANAGE_ACL permission to modify groups + KICK to disconnect):
- Text: "Ban {username}"
- Reason input (sent as chat message to user)
- Group selector: dropdown of groups, default is user's username-group
- Duration: Permanent / 1 hour / 1 day / 1 week / 30 days / Custom
- [Cancel] [Ban] buttons
- Orchestrates: ChatMessage + ModifyGroup + SetUserGroup + KickUser

### 3. Elevate Dialog

Menu item or toolbar button (visible if user has SUDO permission):
- Text: "Enter sudo password"
- Password input (masked)
- [Cancel] [Elevate] buttons
- Sends `Elevate { password }`

### 4. Group Management (in admin panel)

Accessible via gear icon → Admin tab (visible if MANAGE_ACL):

- **Group list**: table with name, permission summary, member count, [Edit] [Delete]
  - Built-in groups ("default", "admin") show as non-deletable
- **Create group**: [+ New Group] → inline form with name + permission checkboxes
- **Edit group**: expand to show permission checkboxes (Room-scoped / Server-scoped sections)
- **User membership**: select user in user list → see their groups, add/remove

### 5. Room ACL Editor

Right-click room → "Edit ACLs" (requires WRITE on the room):

- **Room name** header
- **Inherit ACL** checkbox ("Inherit from parent")
- **ACL entry list** — ordered entries:
  - Group dropdown (all groups + usernames)
  - Grant checkboxes (per permission flag)
  - Deny checkboxes (per permission flag)
  - Apply Here / Apply Subs checkboxes
  - [Remove] button
  - Up/Down arrows for reordering
- **[+ Add Entry]** button
- **[Save]** → sends `SetRoomAcl`

---

## Sprint Structure

### Phase 1: Protocol + Server Refactor (single branch, merge first)

**Branch: `acl-ui-proto`**

Proto changes:
- Add `inherit_acl` + `repeated RoomAclEntry acls` to RoomInfo
- Add `repeated string groups` to User
- Add `expires_at` to SetUserGroup
- Add GroupInfo, GroupChanged, UserGroupChanged messages
- Add `repeated GroupInfo groups` to ServerState
- Add GroupChanged + UserGroupChanged to StateUpdate oneof
- Add BANNED permission flag to permissions.rs
- Remove BanUser, UnbanUser, QueryPermissions, PermissionsInfo messages + payload fields

Server changes:
- Remove ban persistence (bans sled tree, BanEntry, all ban CRUD functions)
- Remove handle_ban_user, handle_unban_user handlers
- Remove handle_query_permissions handler
- Add expires_at to user-group persistence entries
- Auth: compute effective permissions at root, reject if BANNED
- State sync: include group definitions in ServerState, populate User.groups (admin-aware)
- Broadcast GroupChanged on group CRUD, UserGroupChanged on membership changes
- Include room ACL data when building RoomInfo
- Expired membership handling (lazy cleanup on access)

### Phase 2: Parallel worktrees (branch from merged Phase 1)

**Worktree A: `acl-ui-backend`**
- Files: `backend/events.rs`, `backend/handle.rs`
- Add missing Command variants: CreateGroup, DeleteGroup, ModifyGroup, SetUserGroup (with expires_at), SetRoomAcl
- Add state fields: group_list (from ServerState sync), room ACL data (from RoomInfo sync)
- Client-side permission evaluation: use synced data + `api::permissions::effective_permissions()`
- Remove effective_permissions server-push handling, replace with local eval
- Handle GroupChanged + UserGroupChanged state updates

**Worktree B: `acl-ui-dialogs`**
- Files: `egui-test/app.rs`
- Kick dialog (reason input, in user context menu)
- Ban dialog (reason + group selector + duration, orchestrates multiple commands)
- Elevate dialog (password prompt, menu item gated on SUDO)
- Replace current bare kick/ban buttons with dialog-backed versions

**Worktree C: `acl-ui-admin-panel`**
- Files: `egui-test/app.rs`
- Admin panel accessible from gear/settings (gated on MANAGE_ACL)
- Group management: list, create, edit permissions, delete, member management
- Room ACL editor: context menu → dialog with inherit toggle, entry list, grant/deny checkboxes
- User group display: show groups in user tooltip or context menu

### Merge Order

1. Merge `acl-ui-proto` to master
2. Branch A, B, C from updated master
3. Merge A first (backend plumbing)
4. Merge B and C (UI, additive to app.rs)
