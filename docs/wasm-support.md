# WASM Support Plan

This document outlines the changes needed to support `wasm32-unknown-unknown` target for running Rumble in web browsers.

## Overview

Adding WASM support requires platform abstraction because several core dependencies don't work in browsers:
- **quinn** - No UDP sockets in browsers
- **opus** (via audiopus_sys) - Native C library needs WASM compilation
- **cpal** - Has WASM support but requires different configuration
- **tokio** - Limited WASM support (single-threaded only)
- **rustls/aws-lc-rs** - Native crypto needs browser alternative

## Strategy

Use **feature flags** to conditionally compile platform-specific code, with a shared core that works everywhere.

```
┌─────────────────────────────────────────────────────────────────┐
│                     pipeline (unchanged)                        │
│              Pure Rust, already WASM-compatible                 │
└─────────────────────────────────────────────────────────────────┘
                              │
┌─────────────────────────────────────────────────────────────────┐
│                         backend                                  │
├─────────────────────────────────────────────────────────────────┤
│                    Shared Core                                   │
│  - State types, Commands, Events                                │
│  - Audio pipeline processing (pipeline crate)                   │
│  - Protocol message handling (api crate)                        │
├─────────────────────────────────────────────────────────────────┤
│  #[cfg(not(target_arch = "wasm32"))]  │  #[cfg(target_arch =    │
│  Native Backend                       │  "wasm32")]              │
│  - cpal audio                         │  Web Backend             │
│  - quinn QUIC                         │  - Web Audio API         │
│  - opus-rs (FFI)                      │  - WebTransport/WS       │
│  - tokio multi-thread                 │  - opus-wasm             │
│                                       │  - wasm-bindgen-futures  │
└───────────────────────────────────────┴──────────────────────────┘
```

## Detailed Changes

### 1. Workspace Configuration

Add WASM-specific dependencies and features to `Cargo.toml`:

```toml
# Root Cargo.toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.metadata.wasm]
# WASM-specific build configuration
```

### 2. Backend Crate Changes

#### 2.1 Cargo.toml

```toml
[package]
name = "backend"
version = "0.1.0"
edition = "2024"

[features]
default = ["native"]
native = ["dep:cpal", "dep:quinn", "dep:opus", "dep:tokio"]
web = ["dep:wasm-bindgen", "dep:wasm-bindgen-futures", "dep:web-sys", "dep:js-sys"]

[dependencies]
# Shared dependencies (work on all platforms)
anyhow = "1"
bytes = "1"
api = { path = "../api" }
pipeline = { path = "../pipeline" }
prost = "0.14"
tracing = "0.1"
uuid = { version = "1.19.0", features = ["v4"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
nnnoiseless = { version = "0.5.2", default-features = false }

# Native-only dependencies
[target.'cfg(not(target_arch = "wasm32"))'.dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread", "time", "io-util"] }
quinn = { version = "0.11", default-features = false, features = ["rustls-aws-lc-rs", "runtime-tokio"] }
rustls = { version = "0.23", default-features = true }
webpki-roots = "1.0.4"
cpal = "0.17.0"
opus = { git = "https://github.com/u6bkep/opus-rs.git", rev = "2c545dc" }
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# WASM-only dependencies
[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
web-sys = { version = "0.3", features = [
    "console",
    "Window",
    "Navigator",
    "MediaDevices",
    "MediaStream",
    "MediaStreamConstraints",
    "AudioContext",
    "AudioDestinationNode",
    "AudioWorklet",
    "AudioWorkletNode",
    "GainNode",
    "MediaStreamAudioSourceNode",
    "ScriptProcessorNode",
    "WebTransport",
    "WebTransportOptions",
    "WebTransportBidirectionalStream",
    "WebTransportDatagramDuplexStream",
] }
# Consider: tokio with wasm features, or use wasm-bindgen-futures directly
```

#### 2.2 Module Organization

Create platform-specific modules:

```
crates/backend/src/
├── lib.rs              # Re-exports, feature-gated
├── events.rs           # Shared (platform-agnostic)
├── bounded_voice.rs    # Shared
├── processors/         # Shared (uses nnnoiseless, pure Rust)
│
├── native/             # Native-only implementations
│   ├── mod.rs
│   ├── audio.rs        # cpal-based audio
│   ├── codec.rs        # opus-rs based codec
│   ├── audio_task.rs   # tokio + quinn datagrams
│   └── handle.rs       # tokio runtime + quinn streams
│
└── web/                # WASM-only implementations
    ├── mod.rs
    ├── audio.rs        # Web Audio API
    ├── codec.rs        # Opus WASM wrapper
    ├── transport.rs    # WebTransport or WebSocket
    └── handle.rs       # wasm-bindgen-futures based
```

### 3. Transport Abstraction

Create a trait for transport to abstract over QUIC vs WebTransport:

```rust
// crates/backend/src/transport.rs (new file)

/// Abstraction over network transport (QUIC on native, WebTransport on web)
pub trait Transport: Send + Sync {
    /// Send a reliable message on the control stream
    async fn send_reliable(&self, data: &[u8]) -> Result<()>;
    
    /// Receive a reliable message from the control stream
    async fn recv_reliable(&self) -> Result<Vec<u8>>;
    
    /// Send an unreliable datagram (voice data)
    fn send_datagram(&self, data: &[u8]) -> Result<()>;
    
    /// Receive an unreliable datagram
    async fn recv_datagram(&self) -> Result<Vec<u8>>;
    
    /// Check if connection is still alive
    fn is_connected(&self) -> bool;
    
    /// Close the connection
    async fn close(&self);
}
```

### 4. Audio Abstraction

Create a trait for audio I/O:

```rust
// crates/backend/src/audio_traits.rs (new file)

/// Abstraction over audio capture
pub trait AudioCapture: Send {
    /// Start capturing audio
    fn start(&mut self) -> Result<()>;
    
    /// Stop capturing
    fn stop(&mut self);
    
    /// Read captured samples (non-blocking)
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize;
}

/// Abstraction over audio playback
pub trait AudioPlayback: Send {
    /// Start playback
    fn start(&mut self) -> Result<()>;
    
    /// Stop playback
    fn stop(&mut self);
    
    /// Write samples for playback
    fn write_samples(&mut self, samples: &[f32]) -> Result<()>;
}

/// Audio device info (platform-agnostic)
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
```

### 5. Codec Abstraction

Create a trait for audio codec:

```rust
// crates/backend/src/codec_traits.rs (new file)

/// Abstraction over voice encoder
pub trait VoiceEncode: Send {
    fn encode(&mut self, pcm: &[f32]) -> Result<Vec<u8>>;
    fn reset(&mut self);
}

/// Abstraction over voice decoder
pub trait VoiceDecode: Send {
    fn decode(&mut self, data: &[u8]) -> Result<Vec<f32>>;
    fn decode_missing(&mut self) -> Result<Vec<f32>>; // PLC
    fn reset(&mut self);
}
```

### 6. WASM Audio Implementation

For Web Audio API support:

```rust
// crates/backend/src/web/audio.rs

use wasm_bindgen::prelude::*;
use web_sys::{AudioContext, AudioWorkletNode, MediaStream};

pub struct WebAudioCapture {
    context: AudioContext,
    // Use AudioWorklet for low-latency capture
    worklet: Option<AudioWorkletNode>,
    stream: Option<MediaStream>,
}

impl WebAudioCapture {
    pub async fn new() -> Result<Self> {
        let context = AudioContext::new()?;
        
        // Request microphone access
        let navigator = web_sys::window()
            .unwrap()
            .navigator();
        let media_devices = navigator.media_devices()?;
        
        // ... setup AudioWorklet for capture
        
        Ok(Self {
            context,
            worklet: None,
            stream: None,
        })
    }
}
```

### 7. WASM Opus Implementation

Options for Opus in WASM:
1. **libopus compiled to WASM** - Use emscripten to compile libopus
2. **opus-wasm crate** - If available
3. **JavaScript Opus library** - Call via wasm-bindgen

```rust
// crates/backend/src/web/codec.rs

use wasm_bindgen::prelude::*;

#[wasm_bindgen(module = "/js/opus.js")]
extern "C" {
    type OpusEncoder;
    
    #[wasm_bindgen(constructor)]
    fn new(sample_rate: u32, channels: u32, application: u32) -> OpusEncoder;
    
    #[wasm_bindgen(method)]
    fn encode(this: &OpusEncoder, pcm: &[f32]) -> Vec<u8>;
}
```

### 8. WebTransport Implementation

For QUIC-like transport in browsers:

```rust
// crates/backend/src/web/transport.rs

use web_sys::WebTransport;
use wasm_bindgen_futures::JsFuture;

pub struct WebTransportConnection {
    transport: WebTransport,
}

impl WebTransportConnection {
    pub async fn connect(url: &str) -> Result<Self> {
        let transport = WebTransport::new(url)?;
        JsFuture::from(transport.ready()).await?;
        
        Ok(Self { transport })
    }
    
    pub async fn send_datagram(&self, data: &[u8]) -> Result<()> {
        let writer = self.transport.datagrams().writable();
        // ... write data
        Ok(())
    }
}
```

### 9. Server Changes

The server needs to support WebTransport for browser clients:

```rust
// crates/server/src/webtransport.rs (new file)

// Use h3-webtransport or similar crate
// Server needs to handle both QUIC and WebTransport connections
```

## Build Configuration

### .cargo/config.toml

```toml
[target.wasm32-unknown-unknown]
rustflags = ["-C", "target-feature=+simd128"]

[build]
# Default to native
target = "x86_64-unknown-linux-gnu"
```

### Building for WASM

```bash
# Install wasm-pack
cargo install wasm-pack

# Build for web
wasm-pack build crates/backend --target web --features web

# Or with cargo directly
cargo build --target wasm32-unknown-unknown --no-default-features --features web
```

## Testing Strategy

1. **Unit tests** - Run on native, most logic is shared
2. **Integration tests** - Platform-specific test modules
3. **Browser tests** - Use `wasm-bindgen-test` for WASM-specific code

```rust
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
async fn test_webaudio_capture() {
    // Test Web Audio API integration
}
```

## Migration Path

### Phase 1: Abstraction (without breaking native)
1. Create trait abstractions for Transport, Audio, Codec
2. Refactor native implementations to use traits
3. Move native-specific code to `native/` module
4. Add `#[cfg]` gates but keep native as default

### Phase 2: WASM Implementation
1. Add WASM dependencies
2. Implement Web Audio capture/playback
3. Implement WebTransport connection
4. Integrate WASM Opus (or JS wrapper)

### Phase 3: Web UI
1. Create `crates/web-ui` with Yew or Leptos
2. Or use egui with `eframe`'s WASM support
3. Handle browser-specific UX (permissions, etc.)

### Phase 4: Server Support
1. Add WebTransport endpoint to server
2. Handle both QUIC and WebTransport clients
3. Consider WebSocket fallback for older browsers

## Dependencies Summary

| Component | Native | WASM |
|-----------|--------|------|
| Runtime | tokio (multi-thread) | wasm-bindgen-futures |
| Transport | quinn (QUIC) | WebTransport API |
| Audio I/O | cpal | Web Audio API |
| Codec | opus-rs (FFI) | libopus.wasm or JS |
| Crypto | rustls + aws-lc-rs | WebCrypto API |
| Denoise | nnnoiseless | nnnoiseless (works!) |
| Pipeline | pipeline crate | pipeline crate (works!) |

## Known Limitations

1. **No multi-threading** - WASM is single-threaded (SharedArrayBuffer requires COOP/COEP headers)
2. **Microphone permissions** - User must grant permission
3. **WebTransport availability** - Not all browsers support it yet
4. **Audio latency** - Web Audio has higher latency than native
5. **Background tabs** - Browser may throttle audio in background

## References

- [WebTransport API](https://developer.mozilla.org/en-US/docs/Web/API/WebTransport)
- [Web Audio API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Audio_API)
- [cpal WASM support](https://github.com/RustAudio/cpal/issues/547)
- [opus.js](https://github.com/nicebyte/nicebyte/opus.js) - Opus compiled to JS
- [eframe WASM](https://github.com/emilk/egui/tree/master/crates/eframe#wasm)
