# §26 — Permission Flags

From `crates/rumble-protocol/src/permissions.rs`. 16 flags total.

- **Room-scoped (10)**: TRAVERSE, ENTER, SPEAK, TEXT_MESSAGE,
  SHARE_FILE, MUTE_DEAFEN, MOVE_USER, MAKE_ROOM, MODIFY_ROOM, WRITE.
- **Server-scoped (6)**: KICK, BAN, REGISTER, SELF_REGISTER, MANAGE_ACL,
  SUDO.

WRITE is room-scoped (edit room ACLs), not "implies all"; the `admin`
group just has all bits set. Username-as-group: every registered user
has an implicit group equal to their username; group names must not
collide with usernames. See `MEMORY.md` ACL section for details.
