# §32 — RPC Interface

`crates/rumble-egui/src/rpc_client.rs`. Client side; the matching
server lives in `rumble_client::rpc` (gated `#[cfg(unix)]`) and is
opt-in via `--rpc-server`.

Transport: connect to a Unix socket, write one line of JSON, read one
line back, print, exit. Default socket
`rumble_client::rpc::default_socket_path()`; overridable with
`--rpc-socket`.

| CLI string | JSON body | Purpose |
|---|---|---|
| `status` | `{"method":"get_status"}` | Status query. |
| `state` | `{"method":"get_state"}` | Full backend state dump. |
| `mute` / `unmute` | `{"method":"set_muted","muted":<bool>}` | Toggle self-mute. |
| `deafen` / `undeafen` | `{"method":"set_deafened","deafened":<bool>}` | Toggle self-deafen. |
| `disconnect` | `{"method":"disconnect"}` | Drop the connection. |
| `start-transmit` / `stop-transmit` | `{"method":"start_transmit"}` / `{"method":"stop_transmit"}` | External PTT. |
| `join-room <uuid>` | `{"method":"join_room","room_id":<uuid>}` | Move to room. |
| `send-chat <text>` | `{"method":"send_chat","text":<...>}` | Send chat message. |
| `create-room <name>` | `{"method":"create_room","name":<...>}` | Create room. |
| `delete-room <uuid>` | `{"method":"delete_room","room_id":<...>}` | Delete room. |
| `share-file <path>` | `{"method":"share_file","path":<...>}` | Share file via plugin. |
| `download <magnet>` | `{"method":"download_file","magnet":<...>}` | Download by magnet. |
| `mute-user <u64>` / `unmute-user <u64>` | `{"method":"mute_user","user_id":<...>}` etc | Per-user server-side mute. |

A port must reproduce both the CLI string→JSON mapping and the socket-
path resolution.
