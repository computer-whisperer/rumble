# rumble-next: what's left

Punch list for getting `rumble-next` to release-ready parity with
`rumble-egui`. Ordered by user-visible impact. None of these are
blocked; pick the topmost item and go. after checking an item off delete it if it is truely finished and will not need more attention.

1. **Active transfers panel.** A toggleable window listing in-flight
   uploads/downloads with per-transfer progress, cancel, and a "show
   in folder" affordance once complete. Not yet built in either
   client. Backend exposes transfer state via `state.transfers`
   (verify on touch); UI needs a fresh design.
2. **Incoming file prompt UI.** Offers that don't auto-download still
   only render as an inline "Download" button on the chat card — no
   modal/notification. Add an accept/deny prompt for offers that miss
   every auto-download rule (and ideally a toast for ones that match,
   once the transfers panel from §1 isn't carrying that load).
3. **RPC server.** Unix-socket remote control, mirroring
   `rumble-egui::rpc_client`. Lights up `harness-cli` automation
   against rumble-next.
4. **Auto-download history-replay guard.** `App::pump_auto_downloads`
   skips the very first batch (cold connect) via `prev_chat_count == 0`,
   but a mid-session reconnect that replays history will re-trigger
   downloads because `prev_chat_count` is already non-zero. Reset it on
   disconnect, or track per-`transfer_id` "already auto-handled" so
   replays are idempotent.
