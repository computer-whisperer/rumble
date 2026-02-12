# GUI Test Harness

The `egui-test` crate is structured as both a library and binary, enabling programmatic control of the GUI for agents and integration tests.

## Architecture

```
crates/egui-test/src/
├── lib.rs              # Library exports: RumbleApp, TestHarness, Args
├── main.rs             # Thin eframe wrapper (human-facing app)
├── app.rs              # RumbleApp - core application logic
├── harness.rs          # TestHarness for programmatic control
├── hotkeys.rs          # Global hotkey handling
├── key_manager.rs      # Ed25519 key management
├── portal_hotkeys.rs   # Portal-based global hotkeys (Linux)
├── rpc_client.rs       # RPC client for inter-process communication
├── settings.rs         # Persistent settings types
└── toasts.rs           # Toast notification system
```

The key separation: `RumbleApp` contains all UI logic and is independent of eframe. The desktop app wraps it in an eframe runner, while tests/agents use `TestHarness` directly.

## Using TestHarness

```rust
use egui_test::{TestHarness, Args};

// Create harness with default settings
let mut harness = TestHarness::new();

// Or with custom args
let args = Args {
    server: Some("127.0.0.1:5000".to_string()),
    name: Some("test-bot".to_string()),
    ..Default::default()
};
let mut harness = TestHarness::with_args(args);

// Run frames to advance the UI
harness.run_frame();       // Single frame
harness.run_frames(10);    // Multiple frames

// Inject input events
harness.key_press(egui::Key::Space);   // Push-to-talk
harness.key_release(egui::Key::Space);
harness.click(egui::pos2(100.0, 200.0));
harness.type_text("Hello, world!");

// Introspect state
let connected = harness.is_connected();
let app = harness.app();  // Access RumbleApp directly
let backend_state = app.backend().state();  // Full backend state
```

## RumbleApp API

The core application exposes:

```rust
impl RumbleApp {
    /// Create with egui context, tokio handle, and CLI args
    pub fn new(ctx: egui::Context, runtime_handle: Handle, args: Args) -> Self;

    /// Render one frame (called by runner each frame)
    pub fn render(&mut self, ctx: &egui::Context);

    /// Access the backend handle for state/commands
    pub fn backend(&self) -> &BackendHandle;

    /// Check connection status
    pub fn is_connected(&self) -> bool;
}
```

## Writing Agent Tests

```rust
#[test]
fn test_agent_can_connect() {
    let mut harness = TestHarness::with_args(Args {
        server: Some("127.0.0.1:5000".to_string()),
        name: Some("agent".to_string()),
        trust_dev_cert: true,
        ..Default::default()
    });

    // Run frames to let connection establish
    harness.run_frames(100);

    // Check connection state
    assert!(harness.is_connected());

    // Interact with rooms, chat, etc. via backend
    let state = harness.app().backend().state();
    assert!(!state.rooms.is_empty());
}
```
