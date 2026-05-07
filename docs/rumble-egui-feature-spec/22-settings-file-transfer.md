# §22 — Settings — File Transfer

`render_settings_file_transfer`, `app.rs:2012-2186`.

- **Enable auto-download** — checkbox; gates the rules table.
- **Auto-download rules table** — striped 3-column grid:
  - **MIME Pattern** — single-line text edit, e.g. `image/*`.
  - **Max Size** — `DragValue` 0–1000 MB; 0 = disabled for that pattern.
  - **Remove** button per row.
  - **Add Rule** button below appends a new row (default 10 MB, empty
    pattern).
  - Defaults: `image/* ≤ 10 MB`, `audio/* ≤ 50 MB`, `text/* ≤ 1 MB`.
- **Download limit** — `DragValue` in KB/s, range 0–100 000; 0 =
  *"(unlimited)"*. Stored internally in bytes.
- **Upload limit** — same.
- **Continue seeding after download** — checkbox.
- **Clean up downloaded files on exit** — checkbox.
- **Chat History** sub-section:
  - **Auto-sync history on room join** — when enabled, every
    `Command::JoinRoom` is followed by `Command::RequestChatHistory`.

There is no download-directory picker. The bandwidth caps and seed /
cleanup options are present in `rumble-egui` even though
`rumble-next` hides them (per `docs/rumble-next-bringup.md`) because
the `rumble-next` relay plugin doesn't enforce them yet.
