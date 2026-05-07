# §39 — Suggested Port Plan (informational)

A reasonable order for an aetna port, in light of the inventory:

1. **Bring up the runtime composition** (§1.3) — env-filter, tokio
   runtime, hotkey manager on the main thread, image decoders, portal
   init order. Don't try to render anything yet.
2. **Static state plumbing** — get `RumbleApp::new(ctx, runtime, args)`
   compiling against aetna's context type; wire up `BackendHandle` and
   the repaint callback.
3. **First-run + Connect + TOFU modal** — no main window UI yet; just
   prove the auth/identity surface. (§2–§4)
4. **Top-level layout shell** (§5) and **Connection-status pill** (§3.5)
   so transition feedback works.
5. **Room tree + user list** (§8, §9) with stub context menus.
6. **Voice toolbar + hotkeys** (§7, §10, §15) — get talking.
7. **Chat panel** (§12) — match rumble-next's expanded chat UX (file
   cards, lightbox) rather than rumble-egui's sparse renderer.
8. **Settings modal scaffold** (§14) with one panel at a time:
   Connection (§15), Devices (§16), Voice (§17), Sounds (§18),
   Encoder (§20), Statistics (§24), Chat (§21).
9. **Audio pipeline editor** (§19) — JSON-schema-driven forms.
10. **File transfer settings + transfers window** (§22, §6.3) — and
    actually wire `show_transfers` to a renderer this time.
11. **Hotkey settings with capture + portal integration** (§23).
12. **Admin panel + room ACL editor + user kick/ban/elevate** (§25,
    §13.7–§13.10, §7.4) — biggest single piece, leave for last.
13. **Test harness + RPC + harness-cli integration** (§31–§33) — these
    matter for CI/agent loops; reproduce the input/AccessKit query
    surface.
14. **Tackle the known-gap wishlist** (§37) opportunistically.
