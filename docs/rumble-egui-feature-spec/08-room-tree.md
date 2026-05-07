# §8 — Room Tree (Central Panel)

`app.rs:4059-4654`. Uses `egui_ltreeview` from a u6bkep fork (see
`Cargo.toml:22`). Override indent: 20px.

## 8.1 Node identity

```rust
enum TreeNodeId {
    Room(Uuid),
    User { room_id: Uuid, user_id: u64 },
}
```

Users appear inline as leaf nodes inside their room — there is no
separate user-list panel.

## 8.2 Auto-expand

A room is opened by default if it or any descendant contains users
(`rooms_with_users` set, computed by walking ancestors). Empty branches
stay collapsed.

## 8.3 Room label

`📁 {name}` plus `({user_count})` suffix when non-zero, plus
`  [current]` when this is the user's current room. Empty rooms are
rendered with `weak()` styling.

## 8.4 Activation

`Action::Activate` (Enter or double-click) sends
`Command::JoinRoom { room_id }`. If `auto_sync_history` is enabled, also
fires `Command::RequestChatHistory` immediately afterward.

## 8.5 Drag-and-drop reparenting

Dragging one room onto another opens a confirmation modal; on confirm,
sends `Command::MoveRoom { source, target }`. Source and target must
both be `TreeNodeId::Room` and different. Users cannot be dragged.

## 8.6 Room context menu (right-click)

Shows room metadata (name, ID, parent UUID, italic description) followed
by:

- **Join** — always.
- **Rename...** — gated on `MODIFY_ROOM`. Opens `RenameModalState`.
- **Edit Description...** — gated on `MODIFY_ROOM`. Opens
  `DescriptionModalState`.
- **Add Child Room** — gated on `MAKE_ROOM`. Sends
  `Command::CreateRoom { name: "New Room", parent_id }` immediately.
- **Delete Room** — gated on `MODIFY_ROOM` + non-root. Confirmation
  modal.
- **Edit ACLs** — gated on `WRITE`. Opens room ACL editor (see §13.10).

## 8.7 Empty / disconnected placeholders

- Disconnected: *"Not connected. Use Server > Connect..."*.
- Connected but no rooms: *"No rooms received yet."* + **Join Root** /
  **Refresh** buttons (both `Command::JoinRoom { ROOT_ROOM_UUID }`).

## 8.8 Per-room permissions

`state.per_room_permissions: HashMap<Uuid, u32>` is consulted (with
fallback to `state.effective_permissions`) so each context-menu item is
gated correctly per room.
