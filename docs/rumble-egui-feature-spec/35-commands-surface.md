# §35 — Commands Surface (`rumble_client::Command`)

Variants the UI emits today:

- `Connect`, `Disconnect`, `AcceptCertificate`, `RejectCertificate`.
- `JoinRoom`, `RequestChatHistory`, `LocalMessage`.
- `SendChat`, `SendDirectMessage`, `SendTreeChat`.
- `StartTransmit`, `StopTransmit`.
- `SetMuted`, `SetDeafened`, `SetVoiceMode`.
- `MuteUser`, `UnmuteUser`, `SetUserVolume`, `SetServerMute`.
- `KickUser`, `BanUser`, `RegisterUser`, `UnregisterUser`, `Elevate`.
- `CreateRoom`, `RenameRoom`, `MoveRoom`, `DeleteRoom`,
  `SetRoomDescription`, `SetRoomAcl`.
- `CreateGroup`, `ModifyGroup`, `DeleteGroup`, `SetUserGroup`.
- `RefreshAudioDevices`, `SetInputDevice`, `SetOutputDevice`,
  `UpdateAudioSettings`, `UpdateTxPipeline`, `ResetAudioStats`.
- `ShareFile`, `PlaySfx`.

`Command::DownloadFile { share_data }` is defined in the protocol but
is **not invoked anywhere in `rumble-egui`** — confirms there is no
incoming-file-offer UI here.
