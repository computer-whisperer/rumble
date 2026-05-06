# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rumble is a voice chat application (similar to Discord/Mumble) written in Rust. Users can join hierarchical rooms and communicate via voice and text chat. The application uses a client-server architecture with QUIC transport, Ed25519 authentication, and Opus audio codec.

## Build Commands

```bash
cargo build                    # Build all crates
cargo run --bin server         # Run the server
cargo run -p rumble-aetna      # Run the GUI client (active)
cargo test                     # Run all tests
cargo +nightly fmt             # Format code
RUST_LOG=debug cargo run -p rumble-aetna  # Run with debug logging
```

When fixing build issues, run `cargo build -p rumble-aetna` and address the **first** error (later errors are often cascading).

## Crate Architecture

```
        Active client                   Reference-only (deprecated)
┌─────────────────────────┐    ┌─────────────────────┬─────────────────────┐
│   rumble-aetna (GUI)    │    │ rumble-egui (ref)   │ rumble-next (ref)   │
│  aetna-core / winit-wgpu│    │ feature-polished    │ theming experiment  │
└───────────┬─────────────┘    └──────────┬──────────┴──────────┬──────────┘
            │                             │                     │
            └──────── all consume ────────┴─────────────────────┘
                                  │
                                  ▼
            ┌─────────────────────────────────────────────┐
            │       rumble-desktop-shell                  │
            │  settings store, identity files (Argon2 +   │
            │  ChaCha20Poly1305), ssh-agent, global       │
            │  hotkeys, XDG GlobalShortcuts portal        │
            └─────────────────────┬───────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────┐
│              rumble-client (Client Library)                 │
│   BackendHandle with Arc<RwLock<State>>                     │
│   ┌─────────────────┐  ┌────────────────────┐               │
│   │ Connection Task │  │ Audio Task         │               │
│   │ - QUIC streams  │  │ - QUIC datagrams   │               │
│   │ - Protocol msgs │  │ - cpal I/O         │               │
│   │ - State sync    │  │ - Opus encode/dec  │               │
│   └─────────────────┘  └────────────────────┘               │
│   Platform trait via rumble-client-traits                   │
│   Desktop impl provided by rumble-desktop                   │
└───────────────────────┬─────────────────────────────────────┘
                        │
         ┌──────────────┼──────────────┐
         ▼              ▼              ▼
┌──────────────┐ ┌─────────────┐ ┌────────────┐
│rumble-protocol│ │ rumble-audio│ │   server   │
│    proto     │ │   audio     │ │  handlers  │
│    types     │ │   procs     │ │  state     │
└──────────────┘ └─────────────┘ └────────────┘
                                      ▲
                                      │ Bridge protocol
                               ┌──────┴──────┐
                               │mumble-bridge│
                               │ Mumble↔     │
                               │ Rumble proxy│
                               └─────────────┘
```

### Client Status

- **rumble-aetna** is the active client going forward. Built on the aetna UI library (vendored at `vendor/aetna/`, uses winit + wgpu directly), giving proper SVG/icon support, color emoji, and a smaller widget surface than egui.
- **rumble-egui** and **rumble-next** are **deprecated** but kept around as references:
  - `rumble-egui` — most feature-polished implementation; consult it when porting functionality (settings UI, file transfer flows, hotkey config, ACL editor, etc.) to aetna.
  - `rumble-next` — testbed for a significant theming/visual-redesign pass over the egui stack. Useful as a reference for visual design intent, less so for feature breadth.
- New feature work belongs in `rumble-aetna`. Do not add features to `rumble-egui` or `rumble-next` unless explicitly asked.

### Crate Responsibilities

- **rumble-protocol**: Protocol Buffers definitions (`proto/api.proto`), message framing, BLAKE3 state hashing, shared types (`State`, `Command`, `ConnectionState`, etc.)
- **rumble-client**: Client engine — QUIC connection, audio I/O, Opus codec, jitter buffers; depends only on `rumble-client-traits` (no platform code)
- **rumble-client-traits**: Platform-agnostic client traits (transport, audio, codec, keys, storage)
- **rumble-desktop**: Native desktop Platform implementation (quinn, cpal, opus, ed25519)
- **rumble-desktop-shell**: Shared shell-level concerns for desktop clients — persistent settings store, identity-file management (encrypted-at-rest via Argon2 + ChaCha20Poly1305), ssh-agent identity, cross-platform global hotkeys, XDG GlobalShortcuts portal. Consumed by all three desktop clients (`rumble-aetna`, `rumble-egui`, `rumble-next`).
- **rumble-audio**: Pluggable audio processor framework (denoise, VAD, gain control)
- **rumble-aetna**: Active GUI client. Built on aetna-core + aetna-winit-wgpu. `App` impl projects `(state, ui_state) → El` tree per frame; `UiBackend` adapter wraps `BackendHandle`.
- **rumble-egui**: *(deprecated, reference)* egui-based GUI client; most feature-complete implementation. Also exports `TestHarness` for programmatic UI control.
- **rumble-next**: *(deprecated, reference)* egui-based GUI client experimenting with a theme/visual redesign on top of `rumble-widgets`.
- **rumble-widgets**: Custom egui widget library (radio, toggle, slider, tree, combo box, Luna theme overrides). Consumed by `rumble-next`. See pixel-snapping section below.
- **server**: Server binary — room management, user auth, message relay, persistence (sled), ACL system
- **harness-cli**: Daemon-based CLI for automated GUI testing with screenshots and input injection (currently targets `rumble-egui` via its `TestHarness`)
- **mumble-bridge**: Bidirectional bridge between Mumble and Rumble servers, proxying voice and chat

## Key Architecture Patterns

### State-Driven UI
The client exposes a shared `State` via `Arc<RwLock<State>>`. The UI reads state directly for rendering and sends fire-and-forget commands. Client updates state and calls repaint callback to notify UI.

### Two-Task Client Design
1. **Connection Task**: QUIC reliable streams for protocol messages and state sync
2. **Audio Task**: QUIC unreliable datagrams for voice, cpal streams for audio I/O

### Lock-Free Server
- `AtomicU64` for user ID generation
- `DashMap` for per-client lock-free access
- Single `RwLock<StateData>` for rooms/memberships
- Voice relay uses snapshots to avoid holding locks during I/O

### State Synchronization
Server sends incremental `StateUpdate` messages with BLAKE3 hash. Client verifies hash after applying; requests full resync on mismatch.

## Protocol Details

- **Transport**: QUIC (quinn) - reliable streams for control, unreliable datagrams for voice
- **Serialization**: Protocol Buffers (prost) - see `crates/rumble-protocol/proto/api.proto`
- **Audio Format**: Opus at 48kHz, 20ms frames (960 samples)
- **Authentication**: Ed25519 signatures with optional SSH agent support
- **File Sharing**: Server relay (with plugin architecture for alternative backends)

## Audio: Opus Decoder Lifetime (important)

Each remote peer must have a **long-lived Opus decoder instance** that persists across talk spurts. It should only be dropped when the peer leaves the room/session (or after a very long TTL GC fallback). Re-initializing decoders per received packet/talkspurt will cause `rumble_client::codec: codec: decoder initialized` spam and audible crackle/pop at start of speech.

## Formatting

Uses `imports_granularity = "Crate"` in rustfmt.toml - group imports by crate.

## rumble-widgets: pixel snapping

egui's tessellator already pixel-snaps `RectShape`, line segments, and text by default (`TessellationOptions::round_{rects,line_segments,text}_to_pixels = true`). **Circles and `Shape::Path` (e.g. `convex_polygon`) are NOT snapped** — the tessellator uses their coordinates verbatim.

That means if a widget composes an outer rect with an inner circle/polygon, and the outer rect has sub-pixel coordinates from the layout (very common — `ui.horizontal()` + `allocate_exact_size` often returns fractional `left()` / `center().y`), the outer shape snaps one way at tessellate time while the inner shape stays sub-pixel — and they stop agreeing on "center". Visible symptom: radio/switch dots look off-center, slider thumbs drift, pentagon/triangle glyphs look asymmetric. This is invisible at `pixels_per_point=2.0` (screenshot default) because sub-pixel logical coords land on integer physical pixels; it only shows up at `ppp=1.0` which is what `cargo run --bin gallery` actually uses.

**Rule**: at the point in a widget's `paint()` where you derive an indicator/thumb/avatar rect from the outer allocated rect, pixel-snap it before computing any `.center()` or inner geometry:

```rust
use eframe::egui::emath::GuiRounding;

let ppp = ui.ctx().pixels_per_point();
let indicator_rect = Rect::from_min_size(...).round_to_pixels(ppp);
// Inner geometry derived from indicator_rect now co-aligns with the tessellator's snap.
```

Use `round_to_pixels` for filled shapes / even-pixel strokes, `round_to_pixel_center` for explicit 1-px-wide line segments that you want to land on a pixel row.

Sites in `crates/rumble-widgets` that need snapping: any `paint()` or `paint_caret` / `paint_arrow` that composes a circle or `convex_polygon` inside an allocated rect. Current applied sites: `radio.rs`, `toggle.rs`, `slider.rs`, `tree.rs` (DefaultTree caret), `combo_box.rs` (arrow), and the Luna overrides in `luna.rs` (`LunaToggle`, `LunaSlider`, `LunaPresence`, `LunaTree::paint_caret`).

## Vendored Dependencies

Located in `vendor/`. Used primarily for reference; code links against GitHub versions if modified from upstream.

- `egui_ltreeview` - Tree view widget for room hierarchy
- `opus-rs` - Opus audio codec bindings

## GUI Test Harness

The `rumble-egui` crate is structured as both a library and binary, enabling programmatic control of the GUI for agents and integration tests. See [docs/test-harness.md](docs/test-harness.md) for API details and code examples. **Note**: this harness drives the deprecated egui client — it has not yet been ported to `rumble-aetna`.

## Harness CLI (for agent iteration loops)

Daemon-based CLI for automated GUI testing. Currently targets `rumble-egui` (deprecated). See [crates/harness-cli/README.md](crates/harness-cli/README.md) for full documentation.

```bash
# Start everything (daemon + server + client) and take screenshot
cargo run -p harness-cli -- up --screenshot /tmp/ui.png

# After code changes, rebuild and screenshot in one command
cargo run -p harness-cli -- iterate -o /tmp/ui.png

# Clean teardown
cargo run -p harness-cli -- down
```

## Emoji

egui only supports a small range of emoji. `supported_emoji.md` lists them all. Use grep to search it before using a new emoji in the project.
