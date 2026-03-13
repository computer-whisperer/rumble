# Rumble v2 Architecture: Platform Abstraction via Traits

**Status:** Design decisions resolved — ready for implementation planning
**Motivation:** The codebase has reached feature maturity. Before building higher, we need to carve proper seams so that new platforms (WASM, mobile), new clients (terminal, alternative GUIs), and new bridges don't require duplicating protocol and business logic.

**Supersedes:** `docs/wasm-support.md` (which proposed `#[cfg]` flags inside `backend`)

## Design Principles

1. **Traits over `#[cfg]`** — Platform differences are expressed as trait implementations in separate crates, not `#[cfg(target_arch)]` branches inside shared code. This means a WASM build and a native build use the *same* backend logic with *different* injected implementations.

2. **No kitchen-sink crates** — A terminal client or mobile FFI wrapper shouldn't pull in cpal, rqbit, or libp2p just to get the `State` type and connection logic.

3. **One place for protocol knowledge** — The auth handshake, message framing, envelope construction, and state types live in shared crates. New clients and bridges import them, not copy them.

4. **Minimize API surface of traits** — Each trait captures the *smallest* platform-specific boundary. Everything that can be pure Rust stays pure Rust.

---

## Current vs Proposed Crate Layout

### Current

```
api              ← proto, framing, auth helpers, permissions
pipeline         ← audio processor framework (pure Rust, already portable)
backend          ← EVERYTHING: state types, commands, connection logic,
                   cpal audio, opus codec, quinn transport, rqbit,
                   libp2p, sound effects, jitter buffers, audio pipeline
egui-test        ← GUI + key management + settings + hotkeys
server           ← server binary
mumble-bridge    ← bridge (re-implements connection/auth from scratch)
```

### Proposed

```
api              ← proto, framing, auth helpers, permissions, state types,
                   room tree, command enum, envelope builders, shared constants
pipeline         ← audio processor framework (unchanged, already portable)

rumble-client    ← connection logic, protocol state machine, message handling
                   Generic over Platform trait. No platform-specific deps.
                   This is the "brain" that any client uses.

rumble-native    ← Platform impl for desktop: cpal audio, opus-rs, quinn,
                   tokio runtime, rqbit, libp2p, SSH agent
rumble-wasm      ← Platform impl for browser: Web Audio, WebTransport,
                   opus.wasm, wasm-bindgen-futures

egui-test        ← GUI (uses rumble-client + rumble-native on desktop,
                   rumble-client + rumble-wasm for web builds)
server           ← unchanged (server is always native)
mumble-bridge    ← uses rumble-client for connection (no more duplication)
```

---

## The Platform Trait

The central idea: `rumble-client` is generic over a `Platform` trait that bundles all platform-specific capabilities. Each platform crate provides one implementation.

```rust
// crates/rumble-client/src/platform.rs

/// Everything that differs between platforms lives behind this
/// single trait boundary.
///
/// Tokio is a concrete dependency (not abstracted) — WASM support
/// is deferred until WASM threading stabilizes.
pub trait Platform: Send + Sync + 'static {
    type Transport: Transport;
    type AudioBackend: AudioBackend;
    type Codec: VoiceCodec;
    type Storage: PersistentStorage;
    type KeyManager: KeySigning;
}
```

Each associated type is itself a trait. This keeps the `Platform` trait as a "bundle" while letting individual pieces be tested and swapped independently.

---

## Trait Definitions

### Transport

Abstracts QUIC (quinn) vs WebTransport vs WebSocket.

```rust
/// Reliable + unreliable transport for the Rumble protocol.
///
/// Implementations:
/// - NativeTransport: quinn (QUIC over UDP)
/// - WebTransport: browser WebTransport API
/// - (future) WebSocketTransport: fallback for environments without WT
pub trait Transport: Send + Sync + 'static {
    /// Connect to a server. Returns after the QUIC/WT handshake completes.
    /// TLS and ALPN are implementation details of the transport.
    async fn connect(addr: &str, tls_config: TlsConfig) -> Result<Self>
    where
        Self: Sized;

    /// Send a length-delimited protobuf frame on the reliable stream.
    async fn send(&mut self, data: &[u8]) -> Result<()>;

    /// Receive the next length-delimited frame from the reliable stream.
    /// Returns None on clean close.
    async fn recv(&mut self) -> Result<Option<Vec<u8>>>;

    /// Send an unreliable datagram (voice).
    /// Silently drops if the transport doesn't support datagrams or if
    /// the datagram is too large.
    fn send_datagram(&self, data: &[u8]) -> Result<()>;

    /// Receive the next datagram. Returns None on close.
    async fn recv_datagram(&self) -> Result<Option<Vec<u8>>>;

    /// Get the DER-encoded peer certificate (for auth payload signing).
    fn peer_certificate_der(&self) -> Option<Vec<u8>>;

    /// Close the connection.
    async fn close(&self);
}

/// TLS configuration that both native and web transports can use.
/// Native: maps to rustls config. Web: maps to WebTransport options.
pub struct TlsConfig {
    pub accept_invalid_certs: bool,
    pub additional_ca_certs: Vec<Vec<u8>>,  // DER-encoded
    pub accepted_fingerprints: Vec<[u8; 32]>,
}
```

**Why this shape:** The current backend already has a clean split between reliable (streams) and unreliable (datagrams) I/O. The quinn-specific `Connection`, `SendStream`, `RecvStream` types are only used in `handle.rs` and `audio_task.rs`. This trait captures exactly what those modules need.

**What changes:** `run_connection_task()` and `spawn_audio_task()` become generic over `T: Transport` instead of directly using `quinn::Connection`.

### AudioBackend

Abstracts cpal vs Web Audio API vs mobile audio.

```rust
/// Platform audio I/O.
///
/// The audio backend provides device enumeration and stream creation.
/// The actual encoding/decoding, jitter buffering, and pipeline processing
/// remain in rumble-client (they're pure Rust / platform-agnostic).
///
/// Implementations:
/// - CpalAudioBackend: cpal (Linux/macOS/Windows)
/// - WebAudioBackend: Web Audio API + AudioWorklet
/// - (future) OboeAudioBackend: Oboe (Android)
pub trait AudioBackend: Send + 'static {
    type CaptureStream: AudioCaptureStream;
    type PlaybackStream: AudioPlaybackStream;

    /// List available input (microphone) devices.
    fn list_input_devices(&self) -> Vec<AudioDeviceInfo>;

    /// List available output (speaker) devices.
    fn list_output_devices(&self) -> Vec<AudioDeviceInfo>;

    /// Open an input stream. Calls `on_frame` with FRAME_SIZE (960) f32
    /// samples at 48kHz mono whenever a frame is ready.
    fn open_input(
        &self,
        device_id: Option<&str>,
        on_frame: Box<dyn FnMut(&[f32]) + Send>,
    ) -> Result<Self::CaptureStream>;

    /// Open an output stream. The returned stream pulls samples via
    /// the provided callback.
    fn open_output(
        &self,
        device_id: Option<&str>,
        fill_buffer: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> Result<Self::PlaybackStream>;
}

/// A live audio capture stream. Dropping it stops capture.
pub trait AudioCaptureStream: Send {
    fn set_active(&self, active: bool);
}

/// A live audio playback stream. Dropping it stops playback.
pub trait AudioPlaybackStream: Send {}

/// Device info — this type is already defined in backend::audio,
/// just moves to api or rumble-client.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}
```

**Why this shape:** The current `AudioInput::new()` already takes a `FnMut(&[f32])` callback and the `AudioOutput` uses a pull-based model via a shared buffer. The trait captures exactly this pattern. The `InputProcessor` (sample accumulation into 960-sample frames) stays in shared code — it's pure math.

**What stays in shared code:** Jitter buffers, per-user decode state, pipeline processing, SFX mixing, DTX detection, VAD. All of these operate on `&[f32]` buffers and don't touch platform APIs.

### VoiceCodec

Abstracts opus-rs (C FFI) vs opus-wasm vs future codecs.

```rust
/// Opus encoder/decoder.
///
/// Implementations:
/// - NativeOpusCodec: opus-rs (C FFI via audiopus_sys)
/// - WasmOpusCodec: libopus compiled to WASM, or JS opus library
///
/// The codec always operates at 48kHz mono with 960-sample frames.
/// Settings (bitrate, complexity, FEC, DTX) are configured via
/// EncoderSettings which is a plain struct in shared code.
pub trait VoiceCodec: Send + 'static {
    type Encoder: VoiceEncoder;
    type Decoder: VoiceDecoder;

    fn create_encoder(settings: &EncoderSettings) -> Result<Self::Encoder>;
    fn create_decoder() -> Result<Self::Decoder>;
}

pub trait VoiceEncoder: Send {
    fn encode(&mut self, pcm: &[f32], output: &mut [u8]) -> Result<usize>;
    fn apply_settings(&mut self, settings: &EncoderSettings) -> Result<()>;
}

pub trait VoiceDecoder: Send {
    /// Decode an Opus packet to PCM.
    fn decode(&mut self, data: &[u8], output: &mut [f32]) -> Result<usize>;
    /// Packet loss concealment (decode a missing frame).
    fn decode_plc(&mut self, output: &mut [f32]) -> Result<usize>;
    /// Forward error correction (decode FEC data from a later packet).
    fn decode_fec(&mut self, data: &[u8], output: &mut [f32]) -> Result<usize>;
}
```

**Why this shape:** The existing `VoiceEncoder` and `VoiceDecoder` structs in `codec.rs` already have exactly this API. The trait is a near-1:1 extraction.

**What stays in shared code:** `EncoderSettings`, `DecoderStats`, `EncoderStats`, `is_dtx_frame()`, codec constants (`OPUS_FRAME_SIZE`, `OPUS_SAMPLE_RATE`, etc.).

### PersistentStorage

Abstracts filesystem vs localStorage/IndexedDB.

```rust
/// Key-value persistent storage for client settings.
///
/// Implementations:
/// - FileStorage: JSON files in XDG config dir (Linux/macOS/Windows)
/// - WebStorage: localStorage or IndexedDB
pub trait PersistentStorage: Send + Sync + 'static {
    /// Load a value by key. Returns None if not found.
    fn load(&self, key: &str) -> Result<Option<String>>;

    /// Save a value by key.
    fn save(&self, key: &str, value: &str) -> Result<()>;

    /// Delete a key.
    fn delete(&self, key: &str) -> Result<()>;

    /// List all keys with a given prefix.
    fn list_keys(&self, prefix: &str) -> Result<Vec<String>>;
}
```

**What this replaces:** The current `settings.rs` does `fs::read_to_string` / `fs::write` to `~/.config/rumble/`. On WASM this would use `localStorage` or `IndexedDB`. The settings types themselves (`PersistentSettings`, `KeyConfig`) remain as shared structs — only the load/save mechanism is abstracted.

### KeySigning

Abstracts ed25519 signing across platforms.

```rust
/// Ed25519 key management and signing.
///
/// Implementations:
/// - NativeKeySigning: local key files + SSH agent (Unix domain sockets)
/// - WebKeySigning: WebCrypto API + localStorage, or passkey/WebAuthn
pub trait KeySigning: Send + Sync + 'static {
    /// List available signing identities.
    async fn list_keys(&self) -> Result<Vec<KeyInfo>>;

    /// Get a signing callback for a specific key.
    /// The callback signs arbitrary payloads with the Ed25519 private key.
    async fn get_signer(&self, public_key: &[u8; 32]) -> Result<SigningCallback>;

    /// Generate a new Ed25519 keypair and store it.
    async fn generate_key(&self, label: &str) -> Result<KeyInfo>;

    /// Import a key from raw bytes.
    async fn import_key(&self, private_key: &[u8; 32], label: &str) -> Result<KeyInfo>;
}

/// A callback that signs a payload. Already exists in events.rs.
pub type SigningCallback = Arc<dyn Fn(&[u8]) -> Result<[u8; 64]> + Send + Sync>;
```

**What moves where:** `key_manager.rs` (835 lines, currently in egui-test) splits:
- `KeyInfo`, `KeyConfig`, `KeySource` types → shared (api or rumble-client)
- SSH agent implementation → `rumble-native`
- WebCrypto implementation → `rumble-wasm`

---

## What Moves Where

### Into `api` (protocol + shared types)

These are currently scattered and should be centralized:

| What | Currently in | Lines |
|------|-------------|-------|
| `State`, `ConnectionState`, `AudioState` | `backend/events.rs` | ~1300 |
| `Command` enum | `backend/events.rs` | ~250 |
| `RoomTree`, `RoomTreeNode` | `backend/events.rs` | ~100 |
| `VoiceMode`, `AudioSettings` | `backend/events.rs` | ~80 |
| `FileTransfer` types | `backend/events.rs` | ~100 |
| `AudioDeviceInfo` | `backend/audio.rs` | ~10 |
| `EncoderSettings`, codec constants | `backend/codec.rs` | ~50 |
| `KeyInfo`, `KeyConfig`, `KeySource` | `egui-test/key_manager.rs` | ~60 |
| `now_ms()` | duplicated | 5 |
| `SigningCallback` type | `backend/events.rs` | 1 |
| Permission display names | `egui-test/app.rs` | ~30 |
| Envelope builder helpers | (new) | ~50 |

**Note:** `api` remains dependency-light. These types use only `serde`, `uuid`, `prost` (for proto types), and `bitflags` — all of which `api` already depends on. No platform dependencies.

### Into `rumble-client` (the "brain")

The platform-agnostic client logic, generic over `P: Platform`:

| What | Currently in | Lines |
|------|-------------|-------|
| Connection task (`run_connection_task`) | `backend/handle.rs` | ~2500 |
| Audio task (jitter buffer, mixing, pipeline) | `backend/audio_task.rs` | ~2000 |
| `BackendHandle` (command routing, state management) | `backend/handle.rs` | ~500 |
| `InputProcessor` (frame accumulation) | `backend/audio.rs` | ~30 |
| Jitter buffer (`UserAudioState`) | `backend/audio_task.rs` | ~200 |
| Bounded voice channel | `backend/bounded_voice.rs` | ~200 |
| SFX library | `backend/sfx.rs` + `synth.rs` | ~200 |
| Audio dump (debugging) | `backend/audio_dump.rs` | ~100 |
| Cert verification logic | `backend/cert_verifier.rs` | ~200 |

`rumble-client` depends on: `api`, `pipeline`, and standard library. **No** cpal, quinn, opus, tokio, rqbit, libp2p.

### Into `rumble-native` (desktop platform impl)

| What | Currently in | Notes |
|------|-------------|-------|
| `CpalAudioBackend` | `backend/audio.rs` | Wraps cpal device enum + streams |
| `NativeOpusCodec` | `backend/codec.rs` | Wraps opus-rs |
| `QuinnTransport` | `backend/handle.rs` (embedded) | Extracted from connection setup |
| `TokioRuntime` | `backend/handle.rs` (embedded) | Thin wrapper |
| `FileStorage` | `egui-test/settings.rs` (partially) | XDG dirs + JSON files |
| `NativeKeySigning` | `egui-test/key_manager.rs` | SSH agent + local files |
| `TorrentManager` | `backend/torrent.rs` | rqbit wrapper |
| `P2PManager` | `backend/p2p.rs` | libp2p wrapper |

Dependencies: `cpal`, `opus`, `quinn`, `rustls`, `tokio`, `librqbit`, `libp2p`, `ssh-agent-lib`, etc.

### Into `rumble-wasm` (browser platform impl)

| What | Implementation |
|------|---------------|
| `WebAudioBackend` | Web Audio API + AudioWorklet |
| `WasmOpusCodec` | libopus compiled to WASM |
| `WebTransportImpl` | WebTransport API |
| `WasmRuntime` | `wasm_bindgen_futures::spawn_local` |
| `WebStorage` | localStorage / IndexedDB |
| `WebKeySigning` | WebCrypto API |

Dependencies: `wasm-bindgen`, `web-sys`, `js-sys`, `wasm-bindgen-futures`

---

## What Stays Where It Is

Not everything needs to move:

- **`egui-test/app.rs`** — Stays as the egui UI. Still depends on `rumble-client` + `rumble-native`. Gets smaller as key_manager, settings core, and permission formatting move out.
- **`server/`** — Unchanged. The server is always native. No trait abstraction needed.
- **`pipeline/`** — Unchanged. Already pure Rust, already portable.
- **`mumble-bridge/`** — Switches from its own auth handshake to using `rumble-client` for connection. Keeps Mumble-specific protocol code (`mumble_voice.rs`, `mumble_framing.rs`, etc.)

---

## File Transfer: Plugin Architecture

File transfer is **not part of the `Platform` trait**. It's an optional plugin that `BackendHandle` accepts separately. Different platforms and deployments can use different strategies — or none at all.

### Client-Side Plugin

```rust
/// Optional file transfer capability, injected into BackendHandle.
/// Not part of Platform — different deployments can use different
/// strategies, or disable file transfer entirely.
pub trait FileTransferPlugin: Send + Sync + 'static {
    /// Offer a file for transfer. Returns metadata to share in chat.
    fn share(&self, path: PathBuf) -> Result<FileOffer>;

    /// Accept a file offer and begin downloading.
    fn download(&self, offer: &FileOffer) -> Result<TransferHandle>;

    /// Query active transfer status.
    fn transfers(&self) -> Vec<TransferStatus>;

    /// Cancel an active transfer.
    fn cancel(&self, id: &TransferId) -> Result<()>;
}
```

`BackendHandle<P>` takes `Option<Box<dyn FileTransferPlugin>>`. If `None`, file commands return "not available" to the UI.

### Planned Implementations

1. **`rqbit-plugin`** (existing, to be refactored) — BitTorrent-based. Decentralized, good for large files to many recipients. Uses server tracker + relay for peer discovery and NAT traversal. Heavyweight dependency (rqbit + libp2p).

2. **`relay-plugin`** (new, simple) — Server-mediated relay over QUIC streams. Sender opens a stream to server, server opens a stream to recipient, pipes bytes through. Zero new dependencies on the client. Works for any platform. Good enough for screenshots, documents, short clips.

The relay plugin is the priority — it's simpler, lighter, and validates the plugin architecture. The rqbit plugin can be refactored into the same interface later.

---

## Server Plugin System

The server has the same problem as the client — tracker, relay service, and bridge protocol handlers are bolted directly into `handlers.rs` and `server.rs`. A plugin system lets us namespace experiments without touching the core message loop.

### Design

Plugins are **compile-time** — they're Rust crates compiled into the server binary, not dynamically loaded. Each plugin can bring its own `.proto` file that gets included in the build. Proto field numbers are reserved by range:

```proto
// api.proto — field number reservations
// 1-49:    Core protocol (auth, state, chat, rooms)
// 50-69:   ACL system
// 70-79:   Bridge protocol (virtual users — core, not a plugin)
// 80-89:   Tracker plugin (file-transfer-bittorrent)
// 90-99:   File relay plugin (file-transfer-relay)
// 100-109: (next plugin)
```

### Server Plugin Trait

```rust
pub trait ServerPlugin: Send + Sync + 'static {
    /// Plugin name for logging and config namespacing.
    fn name(&self) -> &str;

    /// Handle a proto envelope on the control stream.
    /// Return Ok(true) if handled, Ok(false) to pass to next handler.
    async fn on_message(
        &self,
        envelope: &Envelope,
        sender: &ClientRef,
        ctx: &ServerCtx,
    ) -> Result<bool>;

    /// A client opened a new QUIC stream addressed to this plugin.
    /// The plugin takes ownership of the stream pair.
    async fn on_stream(
        &self,
        header: StreamHeader,
        send: SendStream,
        recv: RecvStream,
        sender: &ClientRef,
        ctx: &ServerCtx,
    ) -> Result<()>;

    /// Client disconnected — clean up any plugin state.
    async fn on_disconnect(&self, client: &ClientRef, ctx: &ServerCtx);

    /// Spawn background tasks on startup (optional).
    async fn start(&self, ctx: &ServerCtx) -> Result<()> { Ok(()) }

    /// Clean shutdown (optional).
    async fn stop(&self) -> Result<()> { Ok(()) }
}
```

### Stream Routing

QUIC's multi-stream architecture provides natural namespacing. Beyond the initial control stream (stream 0), clients can open additional streams tagged for specific plugins:

```rust
/// First frame on a plugin-owned stream.
pub struct StreamHeader {
    /// Plugin name (e.g. "file-relay", "tracker")
    pub plugin: String,
    /// Plugin-specific metadata (e.g. transfer ID, target user)
    pub metadata: Vec<u8>,
}
```

The server reads the header, dispatches to the matching plugin, and the plugin owns that stream's lifetime. File transfer data flows on its own streams, never competing with control messages on stream 0.

### Server Context

Plugins get controlled but powerful access to server capabilities:

```rust
/// What plugins can do with the server.
pub struct ServerCtx {
    // --- Messaging ---
    /// Send a proto envelope to a specific client.
    pub async fn send_to(&self, user_id: u64, envelope: Envelope) -> Result<()>;
    /// Broadcast to all clients in a room.
    pub async fn broadcast_room(&self, room_id: Uuid, envelope: Envelope) -> Result<()>;

    // --- State queries ---
    /// Look up a connected client.
    pub fn get_client(&self, user_id: u64) -> Option<ClientRef>;
    /// Get all users in a room.
    pub fn get_room_members(&self, room_id: Uuid) -> Vec<u64>;
    /// Get a user's current room.
    pub fn get_user_room(&self, user_id: u64) -> Option<Uuid>;

    // --- Stream creation ---
    /// Open a server-initiated stream to a client, tagged for this plugin.
    pub async fn open_stream_to(&self, user_id: u64, header: StreamHeader)
        -> Result<(SendStream, RecvStream)>;

    // --- Persistence ---
    /// Access the persistence layer (if available).
    pub fn persistence(&self) -> Option<&Persistence>;
}
```

### What Becomes a Plugin

| Current code | Becomes | Notes |
|-------------|---------|-------|
| `server/src/tracker.rs` | `file-transfer-bittorrent` plugin | Bundles tracker + relay service |
| `server/src/relay.rs` | `file-transfer-bittorrent` plugin | Same plugin as tracker |
| (new) | `file-transfer-relay` plugin | Simple QUIC stream pipe between clients |
| Bridge protocol (fields 70-79) | **Stays built-in** | Core protocol for virtual users, not an experiment |
| ACL system | **Stays built-in** | Core permission model |
| Auth, chat, rooms, state sync | **Stays built-in** | Core protocol |

### WASM Considerations (Deferred)

WASM support is deferred until WASM threading stabilizes. When it arrives, the trait system is ready — a `rumble-wasm` crate implements `Platform` and slots in. The `Send` bound question, AudioWorklet threading, Opus WASM build strategy, and WebTransport server support will be revisited then.

---

## Migration Plan

The migration is incremental. Each phase produces a working build.

### Phase 1: Extract shared types into `api` *(no behavior change)*

1. Move `State`, `Command`, `RoomTree`, `VoiceMode`, `AudioState`, `AudioDeviceInfo`, `EncoderSettings`, `SigningCallback`, `KeyInfo`, `KeyConfig` from `backend/events.rs` and `egui-test/key_manager.rs` into `api/src/types.rs`
2. Add envelope builder helpers to `api`
3. Add `now_ms()`, permission display names to `api`
4. `backend` and `egui-test` re-export from `api` (no downstream breakage)
5. `mumble-bridge` switches to `api` types instead of ad-hoc construction

**Risk:** Low. Mechanical moves with re-exports for compatibility.

### Phase 2: Define traits in `rumble-client` *(new crate, not yet used)*

1. Create `crates/rumble-client/` with the `Platform` trait and associated traits (`Transport`, `AudioBackend`, `VoiceCodec`, `PersistentStorage`, `KeySigning`)
2. Create `FileTransferPlugin` trait (not part of `Platform`)
3. Move platform-agnostic client logic into generic functions over `P: Platform`
4. Test with a mock `Platform` implementation in unit tests
5. `rumble-client` depends on: `api`, `pipeline`, `tokio`

**Risk:** Medium. This is the core design work. Getting the trait boundaries right is critical.

### Phase 3: Create `rumble-native` *(wraps current code)*

1. Create `crates/rumble-native/` implementing all `Platform` traits with current deps
2. `CpalAudioBackend` wraps `backend/audio.rs`
3. `NativeOpusCodec` wraps `backend/codec.rs`
4. `QuinnTransport` extracts from `backend/handle.rs`
5. `FileStorage` extracts from `egui-test/settings.rs`
6. `NativeKeySigning` wraps `egui-test/key_manager.rs`
7. `NativePlatform` struct bundles all implementations

**Risk:** Medium. This is the largest phase but it's mostly extraction, not new logic.

### Phase 4: Server plugin system

1. Define `ServerPlugin` trait and `ServerCtx` in `crates/server/`
2. Implement plugin dispatch in the server's message loop and stream accept loop
3. Extract tracker + relay into `file-transfer-bittorrent` plugin
4. Implement `file-transfer-relay` plugin (simple QUIC stream pipe)
5. Refactor file transfer client-side code into `FileTransferPlugin` implementations

**Risk:** Medium. Server-side is mostly reshuffling existing code behind the trait. The new relay plugin is small.

### Phase 5: Switch `egui-test` and `mumble-bridge` to `rumble-client` *(the flip)*

1. `egui-test` depends on `rumble-client` + `rumble-native` instead of `backend`
2. `BackendHandle<NativePlatform>` replaces current `BackendHandle`
3. `mumble-bridge` uses `rumble-client` for connection (deletes `rumble_client.rs`)
4. Delete the old `backend` crate (or keep as a thin re-export shim temporarily)

**Risk:** Medium-high. This is the integration point. Expect to iterate on trait bounds.

### Phase 6: WASM *(deferred)*

Parked until WASM threading stabilizes. When ready:
1. Create `crates/rumble-wasm/` implementing `Platform` traits
2. Build egui client for `wasm32-unknown-unknown`
3. Add WebTransport support to server

---

## Impact on Existing Sprints

### What doesn't change
- Server core protocol — untouched (auth, chat, rooms, ACL, state sync, bridge)
- Proto definitions — untouched (plugins extend with reserved field ranges)
- Pipeline — untouched
- egui rendering code — untouched (it reads `State` and sends `Command`, both of which are now in `api`)
- Build command for native — `cargo run -p egui-test` still works

### What gets easier
- New clients — depend on `rumble-client` + your platform crate
- mumble-bridge — no more auth handshake duplication
- Testing — mock `Platform` implementation for unit tests without hardware
- Feature additions — state types and commands are in one place
- Server experiments — new server plugin, not surgery on handlers.rs
- File transfer experiments — swap plugin implementations without touching core

### What gets harder (temporarily)
- Adding new `Command` variants requires touching `api` (but that's already where proto lives)
- Trait bounds can be fiddly to get right during Phase 2-3
- Two crate boundaries to cross instead of one (api → rumble-client → rumble-native)
- Server plugins need `ServerCtx` to expose the right capabilities

---

## Resolved Design Decisions

### 1. Async runtime: concrete tokio, no trait

`rumble-client` depends on tokio directly. No `AsyncRuntime` trait. WASM is deferred — when WASM threading stabilizes, tokio will likely support it natively. If not, we add the trait then.

### 2. BackendHandle: generic type parameter

`BackendHandle<P: Platform>` — the handle is generic over the platform. The constructor takes a `P` instance. Command routing stays in shared code. UI code uses a type alias:
```rust
type Handle = BackendHandle<NativePlatform>;
```

### 3. Trait definitions live in `rumble-client`

The consumer defines the contract. `rumble-native` depends on `rumble-client` to see the traits it implements. The server and `api` crate don't need to know about client platform abstractions.

### 4. File transfer: optional plugin, not part of Platform

`BackendHandle<P>` accepts `Option<Box<dyn FileTransferPlugin>>`. Two planned implementations: rqbit (refactored) and simple QUIC relay (new). See "File Transfer: Plugin Architecture" section above.

### 5. Server plugins: compile-time, proto-extending

Server plugins are Rust crates compiled into the server binary. Each brings its own `.proto` with reserved field number ranges. Plugins get a `ServerCtx` with access to messaging, state queries, stream creation, and persistence. See "Server Plugin System" section above.

### 6. Bridge stays built-in

The mumble-bridge is a separate binary connecting as a client. The server-side bridge protocol (virtual users, voice dedup) is core protocol, not a plugin. Other bridge types (Discord, etc.) would be separate binaries using the same bridge protocol.

### 7. Tracker + relay become one server plugin

`file-transfer-bittorrent` plugin bundles the tracker and relay service. The new simple file relay is a separate `file-transfer-relay` plugin. Both are compiled into the server.

### Deferred (WASM)

These questions are parked until WASM threading stabilizes:
- Audio task threading model on WASM
- Opus WASM build strategy
- `Send` / `MaybeSend` bounds
- WebTransport server support
- nnnoiseless performance on WASM
- RPC on WASM (likely: not needed, browser is the UI)

---

## Dependency Graph (Proposed)

```
                    ┌──────────────┐
                    │     api      │  Proto, types, helpers
                    │  (no deps)   │
                    └──────┬───────┘
                           │
              ┌────────────┼─────────────────────┐
              │            │                     │
              ▼            ▼                     ▼
        ┌──────────┐ ┌──────────┐     ┌──────────────────┐
        │ pipeline │ │  rumble  │     │      server      │
        │(pure Rust│ │  client  │     │  ┌────────────┐  │
        │ portable)│ │  traits  │     │  │  core +    │  │
        └────┬─────┘ │  + logic │     │  │  plugins   │  │
             │       │  + tokio │     │  │  (tracker,  │  │
             │       └────┬─────┘     │  │   relay,   │  │
             │            │           │  │   file-xfr) │  │
             │     ┌──────┴─────┐     │  └────────────┘  │
             │     │            │     └──────────────────┘
             ▼     ▼            │
        ┌──────────────┐        │  (future)
        │ rumble-native│        │  ┌─────────────┐
        │ cpal, quinn, │        └──│ rumble-wasm  │
        │ opus-rs,     │           └─────────────┘
        │ rqbit, libp2p│
        └──────┬───────┘
               │
        ┌──────┴──────┐
        │  egui-test  │
        │  (desktop)  │
        └──────┬──────┘
               │
        ┌──────┴──────┐
        │mumble-bridge│  (uses rumble-client for connection)
        └─────────────┘
```

## Naming Note

`egui-test` is a development-era name. With this restructuring, consider:
- `rumble-desktop` — the native desktop client binary
- `rumble-web` — the WASM web client
- Both share the same egui UI code (eframe supports native + WASM)

---

## egui-test Platform Dependencies

The GUI crate has its own platform-specific deps beyond what's in backend:

| Dependency | Used for | WASM-compatible? | Strategy |
|-----------|---------|-----------------|----------|
| `global-hotkey` | PTT keybind outside window | No | Feature-gate; web uses HTML key events |
| `ashpd` (Linux) | Wayland portal shortcuts | No | Feature-gate; Linux-only already |
| `rfd` | Native file dialog (file sharing) | Partial (web has limited support) | Use `rfd`'s WASM feature or browser `<input type="file">` |
| `arboard` | Clipboard copy/paste | No | Use `web-sys` clipboard API |
| `directories` | XDG config/data dirs | No | Replaced by `PersistentStorage` trait |
| `ssh-agent-lib` | SSH agent signing | No | Part of `KeySigning` trait; web uses WebCrypto |
| `open` (via backend) | "Open file" with OS default app | No | Not applicable on web |
| `clap` | CLI argument parsing | N/A | Not needed on web |
| `tempfile` | Temp files for downloads | No | Use blob URLs on web |

Most of these are already isolated to specific code paths. The main work is:
1. Feature-gate `global-hotkey` and `ashpd` behind `native-hotkeys`
2. Abstract clipboard behind a small trait or `#[cfg]` (2 call sites)
3. Accept that `rfd` and `open` are native-only features

---

## Appendix A: What the Earlier wasm-support.md Got Wrong

The earlier doc proposed `#[cfg(target_arch = "wasm32")]` branches *inside* the backend crate. This has several problems:

1. **Conditional compilation infects everything** — Every file that uses audio, transport, or codec needs `#[cfg]` blocks. Changes to native code must be mirrored in WASM blocks.

2. **Testing is harder** — You can't run WASM code paths in native tests. With traits, you can mock everything.

3. **Compile times increase** — The backend crate compiles both native and WASM paths, only to discard half. With separate crates, each platform compiles only its own code.

4. **No third platform** — Adding Android (Oboe audio, native transport, different storage) would require adding a third `#[cfg]` branch to every conditional. With traits, it's a new crate implementing `Platform`.

The trait approach costs more upfront (Phase 2-3) but pays off immediately when the second platform is added, and on every platform thereafter.

---

## Appendix B: Complete Platform Dependency Inventory

Every non-WASM-compatible dependency and where it's used:

### backend crate (29 non-WASM-safe call sites)

| File | Dependency | What it does | Trait boundary |
|------|-----------|-------------|---------------|
| `audio.rs` (entire) | `cpal` | Device enum, stream create/destroy, sample format handling | `AudioBackend` |
| `audio_task.rs:49` | `quinn::Connection` | `ConnectionEstablished` command carries quinn handle | `Transport` |
| `audio_task.rs` | `tokio::sync::mpsc` | Audio command channel | `AsyncRuntime` |
| `codec.rs` (entire) | `opus` (C FFI) | Encode/decode, FEC, PLC, DTX, settings | `VoiceCodec` |
| `handle.rs:279` | `std::thread::spawn` | Dedicated OS thread for tokio runtime | `AsyncRuntime` |
| `handle.rs:280` | `tokio::runtime::Runtime` | Creates multi-threaded async runtime | `AsyncRuntime` |
| `handle.rs:2042` | `quinn::Endpoint::connect` | QUIC connection to server | `Transport` |
| `handle.rs:3071` | `quinn::Endpoint::client` | Bind local UDP socket | `Transport` |
| `handle.rs:3090` | `std::fs::read` | Load PEM/DER certificate files | `PersistentStorage` |
| `handle.rs:2036` | `std::net::ToSocketAddrs` | DNS resolution | `Transport` |
| `torrent.rs` (entire) | `librqbit` | BitTorrent download/upload/relay | Feature-gate |
| `torrent.rs:75` | `tokio::net::TcpListener` | TCP relay proxy | Feature-gate |
| `p2p.rs` (entire) | `libp2p` | NAT traversal, file exchange | Feature-gate |
| `rpc.rs` (entire) | `tokio::net::UnixListener` | Unix domain socket RPC | Native-only |
| `audio_dump.rs:87` | `std::fs::create_dir_all` | Debug audio dump files | Native-only (debug) |
| `cert_verifier.rs` | `rustls` | TLS cert verification | `Transport` (internal) |

### egui-test crate (11 non-WASM-safe call sites)

| File | Dependency | What it does | Trait boundary |
|------|-----------|-------------|---------------|
| `key_manager.rs` | `ssh-agent-lib` | SSH agent protocol | `KeySigning` |
| `key_manager.rs` | `std::fs` | Key config persistence | `PersistentStorage` |
| `settings.rs` | `directories` | XDG config paths | `PersistentStorage` |
| `settings.rs` | `std::fs` | Settings load/save | `PersistentStorage` |
| `hotkeys.rs` | `global-hotkey` | OS-level key capture | Feature-gate |
| `portal_hotkeys.rs` | `ashpd` | Wayland portal shortcuts | Feature-gate (Linux) |
| `app.rs` | `arboard` | Clipboard access | Small `#[cfg]` |
| `app.rs` | `rfd` | Native file picker | Feature-gate |
| `main.rs` | `clap` | CLI args | N/A (not on web) |
| `main.rs` | `tokio::runtime` | Async runtime for key ops | `AsyncRuntime` |
| `rpc_client.rs` | `tokio::net::UnixStream` | RPC client socket | Native-only |

### Pure Rust / WASM-safe (no changes needed)

| Crate/File | Why it's safe |
|-----------|--------------|
| `api/` (entire) | prost, blake3, uuid, serde, bitflags — all pure Rust |
| `pipeline/` (entire) | Pure Rust audio processing framework |
| `backend/bounded_voice.rs` | Channels + atomics only |
| `backend/sfx.rs` + `synth.rs` | Pure math waveform generation |
| `backend/events.rs` | State types + command enum (no I/O) |
| `nnnoiseless` | Pure Rust RNNoise port (CPU-intensive but compiles to WASM) |
| `ed25519-dalek` | Pure Rust Ed25519 (compiles to WASM) |
| `argon2` + `chacha20poly1305` | Pure Rust crypto (compiles to WASM) |
