# §11 — Toasts

Implemented by `ToastManager` in
`crates/rumble-desktop-shell/src/toasts.rs`. Rendered as the very last
thing in `RumbleApp::render` (`app.rs:5533`) so it overlays everything.

## 11.1 Placement / styling

- Bottom-right, stacked vertically (newest at the bottom).
- 12px screen margin, 320px width, 6px spacing.
- Each toast is its own foreground `egui::Area`, non-interactable;
  rounded-rect (radius 6) background + white text.
- Severity colors:
  - Success — `#4CAF50`.
  - Error — `#F44336`.
  - Info — `#2196F3`.
  - Warning — `#FF9800`.
- Default durations: Success/Info 4s; Error/Warning 6s. Last 1s fades
  alpha 255 → 0 linearly.
- While any toast is alive: `request_repaint_after(50ms)`.

## 11.2 Trigger sites

- Connection-state transitions (§3.6).
- `state.permission_denied` → error toast.
- `state.kicked` → error toast.
- *"Settings saved"* on Apply.
- Clipboard image paste path: warning/error/success outcomes.

## 11.3 No per-message chat toast

New chat messages do **not** raise a toast — only the
`SfxKind::Message` SFX (see §27).
