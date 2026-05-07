# §6 — Top Menu Bar

`app.rs:3634-3725`.

## 6.1 Server menu

- **Connect...** — open connect modal.
- **Disconnect** (when connected) — `Command::Disconnect`.
- **Reconnect** (when not connected, address non-empty) —
  `RumbleApp::reconnect()`.

## 6.2 Settings menu

- **Open Settings** — primes pending settings state from current backend
  state and opens the settings modal (same as gear icon).

## 6.3 File Transfer menu

- **Share File...** — gated on `Permissions::SHARE_FILE` and no
  in-flight dialog. Spawns
  `rfd::AsyncFileDialog::new().pick_file()` on the tokio runtime;
  pending `JoinHandle` is polled per frame and fires
  `Command::ShareFile { path }` when complete. Also flips
  `show_transfers = true`.
- **Show Transfers** — sets `self.show_transfers = true`. **Note:**
  rumble-egui itself never *renders* a transfers window from this flag;
  the bool is exposed via `set_show_transfers` / `is_transfers_visible`
  for external consumers (RPC, harness, other shells). The actual
  transfers UI lives in `rumble-next`.

## 6.4 Right side: connection-status pill

See §3.5 for details.

There is no File / Edit / View / Help equivalent.
