# rumble-next: what's left

Punch list for getting `rumble-next` to release-ready parity with
`rumble-egui`. Ordered by user-visible impact. None of these are
blocked; pick the topmost item and go.

1. **Active transfers panel.** A toggleable window listing in-flight
   uploads/downloads with per-transfer progress, cancel, and a "show
   in folder" affordance once complete. Not yet built in either
   client. Backend exposes transfer state via `state.transfers`
   (verify on touch); UI needs a fresh design.
2. **Incoming file prompts.** When a peer offers a file, surface an
   accept/deny prompt — gated by the auto-download rules already in
   `Settings → Files`. Pair with the active transfers panel (1).
   Backend will also need to emit a one-shot "file offered" event;
   not yet wired.
3. **RPC server.** Unix-socket remote control, mirroring
   `rumble-egui::rpc_client`. Lights up `harness-cli` automation
   against rumble-next.
