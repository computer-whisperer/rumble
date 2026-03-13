# ACL System Implementation Plan

## Overview

Implement a hybrid ACL system modeled after Mumble: server-wide permission groups as the base layer, with per-room grant/deny ACL overrides. Includes kick/ban, server mute, and sudo elevation.

---

## Permission Flags

```rust
bitflags! {
    pub struct Permissions: u32 {
        // Room-scoped
        const TRAVERSE      = 0x001;
        const ENTER         = 0x002;
        const SPEAK         = 0x004;
        const TEXT_MESSAGE   = 0x008;
        const SHARE_FILE    = 0x010;
        const MUTE_DEAFEN   = 0x020;
        const MOVE_USER     = 0x040;
        const MAKE_ROOM     = 0x080;
        const MODIFY_ROOM   = 0x100;

        const WRITE          = 0x200;  // Edit ACL entries for this room

        // Server-scoped (root only)
        const KICK           = 0x10000;
        const BAN            = 0x20000;
        const REGISTER       = 0x40000;
        const SELF_REGISTER  = 0x80000;
        const MANAGE_ACL     = 0x100000; // Manage groups + user-group assignments
        const SUDO           = 0x200000;
    }
}
```

## Groups

- Stored in sled `groups` tree. Server-wide scope.
- Each group: `name: String` + `permissions: u32` bitmask.
- **Built-in groups (created on first run):**
  - `default` — `TRAVERSE|ENTER|SPEAK|TEXT_MESSAGE|SHARE_FILE|SELF_REGISTER`
  - `admin` — all permission bits set explicitly (no magic "implies all")
  - `bridge` — minimal permissions for bridge virtual users
- Custom groups created at runtime by users with `MANAGE_ACL`.
- Multi-group resolution: union (OR) of all group permissions.
- Everyone is implicitly in `default`.
- The `admin` group's permissions bitmask is simply the OR of every permission flag. No expansion logic — all permissions are independent.

### Username-as-Group

Every registered user has an implicit group named after their username with no base permissions. This allows per-user room ACL overrides without a separate user_id field. Username must not collide with group names (validated on registration and group creation).

## Per-Room ACL Entries

Each room can have ordered ACL entries (grant/deny deltas):

```rust
struct RoomAclEntry {
    group: String,        // group name or username
    grant: u32,           // permissions to ADD
    deny: u32,            // permissions to REMOVE
    apply_here: bool,     // applies to this room
    apply_subs: bool,     // inherited by sub-rooms
}
```

- Rooms have `inherit_acl: bool` (default true). If false, resets to base group permissions.
- New rooms start with empty ACL entries (pure inheritance from parent).
- ACL entry order matters: later entries override earlier ones for the same permission.

## Permission Evaluation Algorithm

```
effective_permissions(user, target_room):
  1. if user.is_superuser → return ALL
  2. base = union of all user's group permissions (includes default)
  3. granted = base
  4. Walk room chain root → target:
     for each room in chain:
       if room.inherit_acl == false:
         granted = base  (reset to group baseline)
       for each acl in room.acls (in order):
         if acl group not in user's groups → skip
         if this is target room and !acl.apply_here → skip
         if this is ancestor and !acl.apply_subs → skip
         granted |= acl.grant
         granted &= !acl.deny
       if TRAVERSE not in granted and room != target → ABORT (return empty)
  5. Server-scoped permissions only count when evaluated at root
  6. return granted
```

No expansion step — all permissions are independent flags. The `admin` group simply has all bits set in its base permissions. Room-level denies directly remove individual permissions from anyone, including admins. Superuser (step 1) is the only true bypass.

## Server Mute

- **Two tracking flags per user:** `server_muted: bool` (effective state) + `manually_server_muted: bool`
- **Automatic:** On room join, evaluate SPEAK permission. If denied → set `server_muted = true`. On move to room with SPEAK → clear server_muted (unless `manually_server_muted`).
- **Manual:** Users with `MUTE_DEAFEN` permission can toggle another user's `manually_server_muted`. Sets both flags.
- **Voice relay hot path:** Check `server_muted` atomic bool — no ACL evaluation per-packet.
- **Client display:** Server broadcasts `UserStatusChanged` with `server_muted` field. Client shows distinct "server muted" indicator.

## Kick & Ban

- **Kick:** Requires `KICK` (root-scoped). Sends `UserKicked` to target, disconnects. Can reconnect immediately.
- **Ban:** Requires `BAN` (root-scoped). Stores `BanEntry { public_key, reason, banned_by, expires_at: Option }` in sled `bans` tree. Rejected during auth. Duration or permanent (`expires_at = None`).

## Sudo Elevation

- **Setup:** `server set-sudo-password <password>` CLI subcommand writes hashed password to sled.
- **Elevation:** Client sends `Elevate { password }`. Server checks user has `SUDO` permission (evaluated at root) AND password matches → sets `is_superuser = true` for session.
- **Superuser:** Bypasses ALL ACL evaluation. Session-only (lost on disconnect).
- **Visibility:** `is_elevated` field on User/UserStatusChanged — clients display indicator.

## MoveUser Semantics

- Mover needs `MOVE_USER` on the **source** room (where user currently is).
- Mover needs `ENTER` on the **target** room.
- The moved user's permissions are not checked (forced move).
- Self-move = JoinRoom → checks `ENTER` on target room only.

---

## Protocol Changes (api.proto)

### Modified Messages

```protobuf
// Add fields to existing User message
message User {
  UserId user_id = 1;
  string username = 2;
  RoomId current_room = 3;
  bool is_muted = 4;
  bool is_deafened = 5;
  bool server_muted = 6;       // NEW
  bool is_elevated = 7;        // NEW
}

// Add fields to existing UserStatusChanged
message UserStatusChanged {
  UserId user_id = 1;
  bool is_muted = 2;
  bool is_deafened = 3;
  bool server_muted = 4;       // NEW
  bool is_elevated = 5;        // NEW
}
```

### New Messages

```protobuf
// Permission denied error response
message PermissionDenied {
  uint32 required_permission = 1;
  bytes room_id = 2;
  string message = 3;
}

// Kick user from server
message KickUser {
  uint64 target_user_id = 1;
  string reason = 2;
}

// Notification that a user was kicked
message UserKicked {
  uint64 user_id = 1;
  string reason = 2;
  string kicked_by = 3;
}

// Ban user
message BanUser {
  uint64 target_user_id = 1;
  string reason = 2;
  uint64 duration_secs = 3;    // 0 = permanent
}

// Unban user
message UnbanUser {
  bytes public_key = 1;
}

// Set server mute on another user
message SetServerMute {
  uint64 target_user_id = 1;
  bool muted = 2;
}

// Elevate to superuser
message Elevate {
  string password = 1;
}

// Effective permissions for a room (server → client)
message PermissionsInfo {
  bytes room_id = 1;
  uint32 effective_permissions = 2;
}

// Group management
message CreateGroup {
  string name = 1;
  uint32 permissions = 2;
}

message DeleteGroup {
  string name = 1;
}

message ModifyGroup {
  string name = 1;
  uint32 permissions = 2;
}

message SetUserGroup {
  uint64 target_user_id = 1;
  string group = 2;
  bool add = 3;                // true = add, false = remove
}

// Room ACL management
message RoomAclEntry {
  string group = 1;
  uint32 grant = 2;
  uint32 deny = 3;
  bool apply_here = 4;
  bool apply_subs = 5;
}

message SetRoomAcl {
  bytes room_id = 1;
  bool inherit_acl = 2;
  repeated RoomAclEntry entries = 3;
}

// Query permissions
message QueryPermissions {
  bytes room_id = 1;
}
```

### New Payload Fields

Add to the `oneof payload` in `Envelope`:

```protobuf
// Client → Server
KickUser kick_user = 30;
BanUser ban_user = 31;
UnbanUser unban_user = 32;
SetServerMute set_server_mute = 33;
Elevate elevate = 34;
CreateGroup create_group = 35;
DeleteGroup delete_group = 36;
ModifyGroup modify_group = 37;
SetUserGroup set_user_group = 38;
SetRoomAcl set_room_acl = 39;
QueryPermissions query_permissions = 40;

// Server → Client
PermissionDenied permission_denied = 41;
UserKicked user_kicked = 42;
PermissionsInfo permissions_info = 43;
```

---

## Persistence Changes (sled)

### New Trees

| Tree | Key | Value |
|------|-----|-------|
| `groups` | group name (UTF-8) | `PersistedGroup { permissions: u32 }` |
| `user_groups` | public key (32 bytes) | `Vec<String>` |
| `room_acls` | room UUID (16 bytes) | `PersistedRoomAcl { inherit_acl: bool, entries: Vec<AclEntry> }` |
| `bans` | public key (32 bytes) | `BanEntry { reason: String, banned_by: String, expires_at: Option<u64> }` |
| `sudo_password` | fixed key "sudo" | bcrypt/argon2 hashed password |

### Cleanup

Remove the unused `RegisteredUser.roles: Vec<String>` field from persistence.rs entirely. No migration needed — no deployed servers to maintain backwards compatibility with.

---

## Server Changes

### New Files

- **`crates/server/src/acl.rs`** — Permission evaluation, group management, ACL helpers
  - `effective_permissions(user_groups, room_chain, room_acls) -> Permissions`
  - `check_permission(state, user_id, room_id, required: Permissions) -> Result<(), PermissionDenied>`
  - Group CRUD functions
  - Ban management functions

### Modified Files

- **`crates/server/src/state.rs`**
  - `ServerState`: Add fields for ACL data (groups cache, ban list)
  - `ClientHandle`: Add `is_superuser: AtomicBool`, `server_muted: AtomicBool`, `manually_server_muted: AtomicBool`
  - `UserStatus`: Add `server_muted: bool`, `is_elevated: bool`
  - `StateData`: No change (room ACLs stored separately in sled, loaded on demand or cached)

- **`crates/server/src/handlers.rs`**
  - Add `check_permission()` call before each existing handler body
  - Add new handlers: `handle_kick_user`, `handle_ban_user`, `handle_unban_user`, `handle_set_server_mute`, `handle_elevate`, `handle_create_group`, `handle_delete_group`, `handle_modify_group`, `handle_set_user_group`, `handle_set_room_acl`, `handle_query_permissions`
  - Modify `handle_join_room`: evaluate SPEAK after join, set server_muted if denied
  - Modify voice datagram relay: check `server_muted` atomic

- **`crates/server/src/persistence.rs`**
  - New sled tree opens: `groups`, `user_groups`, `room_acls`, `bans`, `sudo_password`
  - CRUD functions for each tree
  - `ensure_default_groups()` — create default/admin groups on first run

- **`crates/server/src/main.rs`**
  - CLI subcommand: `add-admin <base64-public-key>`
  - CLI subcommand: `set-sudo-password <password>`

- **`crates/server/src/config.rs`**
  - No changes needed (groups are sled-only)

### Auth Changes

- During authentication (handlers.rs `handle_client_hello`): check ban list, reject if banned (check expiry).
- After authentication: load user's groups from sled, cache in connection state.

---

## Client Changes

### Backend (crates/backend)

- **`state.rs`**: Add `server_muted: bool`, `is_elevated: bool` to user state. Add `effective_permissions: u32` for current room.
- **`connection.rs` or message handler**: Handle `PermissionDenied`, `UserKicked`, `PermissionsInfo` messages. Update state on `UserStatusChanged` with new fields.

### GUI (crates/egui-test)

- **`app.rs`**:
  - Server mute indicator (distinct icon/color from self-mute)
  - Elevated user indicator (badge/icon next to username)
  - Grey out / hide actions based on `effective_permissions`:
    - "Create Room" hidden without `MAKE_ROOM`
    - "Delete/Rename Room" hidden without `MODIFY_ROOM`
    - Context menu items gated by permissions
    - File share button hidden without `SHARE_FILE`
  - `PermissionDenied` → toast notification
  - `UserKicked` → disconnect + toast with reason
  - Mute button shows "Server Muted" state (non-toggleable by user)

---

## Operation → Permission Check Matrix

| Operation | Permission | Scope | Notes |
|-----------|-----------|-------|-------|
| JoinRoom | ENTER on target + TRAVERSE on path | Room | Auto server-mute if no SPEAK |
| Speak (voice) | server_muted check | Hot path | Cached flag, not evaluated per-packet |
| ChatMessage | TEXT_MESSAGE in room | Room | |
| ShareFile | SHARE_FILE in room | Room | |
| CreateRoom | MAKE_ROOM in parent | Room | |
| DeleteRoom | MODIFY_ROOM on room | Room | Still can't delete root |
| RenameRoom | MODIFY_ROOM on room | Room | |
| MoveRoom | MODIFY_ROOM on room + MAKE_ROOM on new parent | Room | |
| SetRoomDescription | MODIFY_ROOM on room | Room | |
| MoveUser | MOVE_USER on source + ENTER on target | Room | Target user not checked |
| SetServerMute | MUTE_DEAFEN in user's room | Room | |
| KickUser | KICK | Root | |
| BanUser | BAN | Root | |
| RegisterUser (other) | REGISTER | Root | |
| RegisterUser (self) | SELF_REGISTER | Root | |
| UnregisterUser | REGISTER | Root | |
| CreateGroup | MANAGE_ACL | Root | Name != any username |
| DeleteGroup | MANAGE_ACL | Root | Can't delete builtins |
| ModifyGroup | MANAGE_ACL | Root | |
| SetUserGroup | MANAGE_ACL | Root | |
| SetRoomAcl | WRITE on the room | Room | Room-scoped ACL editing |
| Elevate | SUDO | Root | + correct password |

---

## AFK Channel Example

Setup: One ACL entry on the AFK room:
```
group: "default", grant: 0, deny: SPEAK | MOVE_USER, apply_here: true, apply_subs: true
```

- Users can't talk → SPEAK denied, auto server-muted
- Can be moved in → mover has MOVE_USER in source room + ENTER on AFK
- Only you can leave → JoinRoom checks ENTER on target (you have it), but MOVE_USER denied in AFK prevents others from moving you out
- Sub-rooms inherit → apply_subs: true
- Even admins can't move users out → room deny removes MOVE_USER from admin's base permissions like anyone else
- Admin can change the rules → still has MANAGE_ACL (server-scoped, unaffected by room ACLs)
- Only a superuser (elevated via sudo) bypasses the AFK restrictions

---

## Sprint Structure

### Phase 1: Foundation (single branch, merge first)

**Branch: `acl-api-types`**
- Proto message definitions (all new messages + modified User/UserStatusChanged)
- `crates/api/src/permissions.rs` — Permissions bitflags, evaluation function, helpers
- Export from `crates/api/src/lib.rs`
- ~200-300 lines, quick to implement and merge

### Phase 2: Parallel worktrees (branch from merged Phase 1)

**Worktree A: `acl-server-core`**
- Files: `acl.rs` (NEW), `persistence.rs`, `state.rs`, `main.rs`
- ACL module with evaluation wrapper, group CRUD, ban management
- Sled persistence for groups, user_groups, room_acls, bans, sudo_password
- State extensions (ClientHandle fields, group cache)
- CLI subcommands (add-admin, set-sudo-password)
- ensure_default_groups() on startup

**Worktree B: `acl-server-handlers`**
- Files: `handlers.rs` (primary)
- Permission guards on all existing handlers
- New handlers: kick, ban, unban, server mute, elevate, group mgmt, room ACL mgmt
- Voice datagram relay: server_muted check
- JoinRoom: auto server-mute evaluation
- Auth: ban list check
- PermissionDenied response sending

**Worktree C: `acl-client-ui`**
- Files: `backend/state.rs`, `backend/connection.rs`, `egui-test/app.rs`
- Backend state: effective_permissions, server_muted, is_elevated
- Handle new message types (PermissionDenied, UserKicked, PermissionsInfo)
- UI indicators: server mute icon, elevated badge
- Permission-gated UI elements (grey out / hide)
- Toast notifications for permission denied and kick

### Merge Order
1. Merge `acl-api-types` to master
2. Branch worktrees A, B, C from updated master
3. Worktrees develop in parallel
4. Code review each worktree
5. Merge A first (core depends on nothing else)
6. Merge B (handlers depend on A's acl.rs)
7. Merge C (client depends on proto types, independent of server internals)
