# §34 — State Surface (BackendHandle / State)

These are the fields a UI port consumes.

## 34.1 `state.connection: ConnectionState`

5 variants: `Disconnected`, `Connecting { server_addr }`,
`Connected { server_name, user_id }`, `ConnectionLost { error }`,
`CertificatePending { cert_info }`.

## 34.2 `state.audio: AudioState`

- `self_muted`, `self_deafened`, `is_transmitting`.
- `voice_mode: VoiceMode` (PushToTalk | Continuous).
- `talking_users: HashSet<u64>`.
- `muted_users: HashSet<u64>` (locally muted).
- `per_user_rx: HashMap<u64, PerUserRx { volume_db, ... }>`.
- `selected_input`, `selected_output`, `input_devices`,
  `output_devices: Vec<AudioDeviceInfo { id, name, pipeline,
  is_default }>`.
- `tx_pipeline: PipelineConfig`, `rx_pipeline_defaults`.
- `settings: AudioSettings`.
- `stats: AudioStats { actual_bitrate_bps, avg_frame_size_bytes,
  packets_sent, packets_received, packets_lost,
  packets_recovered_fec, frames_concealed, playback_buffer_packets }`.
- `input_level_db: Option<f32>`.

## 34.3 Rooms / users

- `rooms: HashMap<Uuid, Room>`.
- `room_tree: { nodes, roots, ancestors, children }`.
- `users: Vec<User { user_id, username, current_room, is_muted,
  is_deafened, server_muted, is_elevated, groups: Vec<String> }>`.
- `users_in_room(uuid)` helper.
- `my_room_id: Option<Uuid>`, `my_user_id: Option<u64>`.

## 34.4 Permissions

- `effective_permissions: u32` (current room).
- `per_room_permissions: HashMap<Uuid, u32>`.
- `permission_denied: Option<String>` (consumed/taken per frame).
- `kicked: Option<String>` (consumed/taken per frame).

## 34.5 Chat

- `chat_messages: Vec<ChatMessage>`.

## 34.6 ACL groups

- `group_definitions: Vec<GroupInfo { name, permissions, is_builtin }>`.
