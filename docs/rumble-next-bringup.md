# rumble-next: what's left

Punch list for getting `rumble-next` to release-ready parity with
`rumble-egui`. After checking an item off, delete it if it is truly
finished and will not need more attention.

1. **RPC server.** Unix-socket remote control, mirroring
   `rumble-egui::rpc_client`. Lights up `harness-cli` automation
   against rumble-next.
2. **Harness parity.** `rumble-next` exposes an in-process
   mock-backend `TestHarness` that can arrange backend `State`, open
   settings pages, step frames, render screenshots, and inspect emitted
   commands without starting networking or the audio pipeline. It still
   lacks the fuller `rumble-egui` harness surface such as input
   injection, query-by-label, click-widget helpers, and "run until
   settled" helpers. Decide whether RPC server support makes this
   redundant; otherwise grow the harness to parity.
3. **Bandwidth caps / seed / cleanup-on-exit settings.** Currently
   hidden from the Files settings page because the relay plugin
   doesn't enforce them. Either wire them through to the plugin
   (download/upload throttles, seed-after-completion lifetime,
   delete-on-exit sweep) or accept that rumble-next will never
   surface them and prune the fields from `FileTransferSettings`.

# bugs

- files sent in chat:
  - no preview thumbnail on images (egui_extras image loaders aren't
    installed yet; needs `egui_extras::install_image_loaders` plus
    a thumb path in the file-offer card for `image/*` mimes that
    have a downloaded `local_path`)
