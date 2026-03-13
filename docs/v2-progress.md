# V2 Architecture Migration — Progress & Remaining Work

**Last updated:** 2026-03-12
**Design doc:** `docs/v2-architecture.md`

---

## Phase Summary

| Phase | Description | Status | Commits |
|-------|-------------|--------|---------|
| **1** | Move shared types to `api` | Done | `7468040` |
| **2a** | Define traits in `rumble-client` | Done | `2318033` |
| **3** | Implement `rumble-native` | Done | `e930c71`, `0158da6` |
| **4a** | Server plugin infrastructure | Done | `90c84df` |
| **4b** | Extract tracker into plugin | Done | `0a321ed` |
| **4c** | Plugin stream routing | Done | `a833c10` |
| **5a** | Datagram transport abstraction | Done | `e6effb8`, `b82419d` |
| **5b** | Full Transport integration | Done | — |
| **5c** | `BackendHandle<P: Platform>` | Done | — |
| **5d** | Switch `egui-test` to rumble-client | Done (via alias) | — |
| **5e** | Switch `mumble-bridge` to rumble-client | Done | — |
| **5f** | Deprecate `backend` crate | Done (dead code removed) | — |
| **6** | WASM platform | Deferred indefinitely | — |

---

## What's Done

### Phase 1 — Shared types in `api`

State, Command, AudioDeviceInfo, EncoderSettings, SigningCallback, and other shared types live in `api/src/types.rs`. Both `backend` and `egui-test` import from `api`.

### Phase 2a — Trait definitions in `rumble-client`

All six trait families are defined and exported:

| Trait | File | Methods |
|-------|------|---------|
| `Platform` | `platform.rs` | Bundle of associated types |
| `Transport` + `DatagramTransport` | `transport.rs` | connect, send/recv, datagram, close |
| `AudioBackend` + streams | `audio.rs` | list devices, open input/output |
| `VoiceCodec` + encoder/decoder | `codec.rs` | encode, decode, PLC, FEC, settings |
| `PersistentStorage` | `storage.rs` | load, save, delete, list_keys |
| `KeySigning` | `keys.rs` | list_keys, get_signer, generate, import |
| `FileTransferPlugin` | `file_transfer.rs` | share, download, transfers, cancel |

The `Transport` trait was extended in Phase 5a with `type Datagram: DatagramTransport` and `datagram_handle()` to split reliable vs unreliable I/O for the two-task architecture.

### Phase 3 — `rumble-native` implementations

All Platform trait impls exist with `NativePlatform` as the bundle:

| Impl | File | Lines | Tests |
|------|------|-------|-------|
| `QuinnTransport` + `QuinnDatagramHandle` | `transport.rs` | ~230 | 0 |
| `CpalAudioBackend` + streams | `audio.rs` | ~640 | 0 |
| `NativeOpusCodec` + encoder/decoder | `codec.rs` | ~210 | 5 |
| `FileStorage` | `storage.rs` | ~200 | 7 |
| `NativeKeySigning` (local + SSH agent) | `keys.rs` | ~450 | 4 |
| `FingerprintVerifier` + `AcceptAllVerifier` | `cert_verifier.rs` | ~180 | 0 |

### Phase 4 — Server plugin system

- **ServerPlugin trait** (`plugin.rs`): `on_message`, `on_stream`, `on_disconnect`, `start`, `stop`
- **ServerCtx**: messaging (send_to, broadcast_room), state queries, `open_stream_to`, persistence
- **StreamHeader**: first frame on plugin-owned QUIC streams (u16 name_len + name + metadata)
- **Stream dispatch** (`server.rs`): secondary streams probed for StreamHeader, dispatched to matching plugin
- **FileTransferBittorrentPlugin** (`tracker_plugin.rs`): handles TrackerAnnounce + TrackerScrape, owns its own Tracker instance

### Phase 5a — Datagram transport abstraction

- `DatagramTransport` trait added to `rumble-client` (send_datagram + recv_datagram)
- `audio_task.rs` uses `Arc<dyn DatagramTransport>` instead of `quinn::Connection`
- `handle.rs` wraps `quinn::Connection` in `QuinnDatagramHandle` before passing to audio task
- Fixed framing bug: `QuinnTransport::send()` now uses varint prefix (matching `api::try_decode_frame`)

### Phase 5b — Full Transport integration in handle.rs

- `Transport` trait extended with `TransportRecvStream` and `take_recv()` for two-task architecture
- `TlsConfig` extended with `captured_cert: Option<CapturedCert>` for interactive cert verification
- `ServerCertInfo`, `CapturedCert`, helper functions moved to `rumble-client/cert.rs`
- `InteractiveCertVerifier` moved to `rumble-native/cert_verifier.rs` (alongside existing `FingerprintVerifier`)
- `QuinnTransport::connect()` now supports all three verifier modes (accept-all, interactive, fingerprint-pinned)
- `connect_to_server()` uses `Transport::connect()` for QUIC handshake, `Transport::send()/recv()` for auth
- `run_connection_task()` stores `Option<QuinnTransport>` — zero `quinn::` references in handle.rs
- `run_receiver_task()` uses `TransportRecvStream` for framed message reception
- `make_client_endpoint()` and `compute_server_cert_hash()` removed (functionality in Transport)
- `backend/cert_verifier.rs` reduced to re-exports from `rumble-client` and `rumble-native`

---

## Remaining Work

### Phase 5b — Integrate Transport trait into connection task — DONE

Completed: `handle.rs` now uses `Transport::send()/recv()` exclusively. Zero `quinn::` references remain in handle.rs.

**What was done:**
- Added `TransportRecvStream` trait and `take_recv()` to `Transport` for two-task send/recv split
- Added `CapturedCert` to `TlsConfig` for interactive cert verification (hybrid of options A+B)
- Moved `ServerCertInfo` and `CapturedCert` types to `rumble-client/cert.rs` (platform-agnostic)
- Moved `InteractiveCertVerifier` to `rumble-native/cert_verifier.rs`
- `connect_to_server()` now uses `QuinnTransport::connect()` + `transport.send()/recv()` for auth handshake
- `run_connection_task()` stores `Option<QuinnTransport>` instead of `quinn::Connection` + `quinn::SendStream`
- All ~25 command handlers use `send_envelope(t, &env)` helper instead of `send.write_all(&encode_frame(...))`
- `run_receiver_task()` takes `QuinnRecvStream` (via `TransportRecvStream` trait)
- `make_client_endpoint()` deleted — connection setup handled by `QuinnTransport::connect()`
- `compute_server_cert_hash()` replaced by `transport.peer_certificate_der()`
- TorrentManager still uses `transport.connection()` accessor for raw quinn connection (Phase 5f)
- `backend/cert_verifier.rs` now re-exports from `rumble-client` and `rumble-native`

### Phase 5c — Make `BackendHandle` generic over `Platform` — DONE

Completed: `BackendHandle<P: Platform>` is fully generic. `pub type BackendHandle = handle::BackendHandle<NativePlatform>` alias in lib.rs preserves backward compatibility.

**What was done:**
- `audio_task.rs`: Made generic over `P: Platform` — `spawn_audio_task<P>`, `run_audio_task<P>`, all helper functions parameterized
- `audio_task.rs`: Replaced `AudioSystem`/`AudioInput`/`AudioOutput` with `P::AudioBackend` + shared `Arc<Mutex<VecDeque<f32>>>` playback buffer (push→pull bridge)
- `audio_task.rs`: Replaced `VoiceEncoder`/`VoiceDecoder` with `P::Codec` trait usage, zero-copy encode/decode API
- `audio_task.rs`: `UserAudioState<D: VoiceDecoderTrait>` generic, `capture_active` Arc<AtomicBool> replaced with local bool + `AudioCaptureStream::set_active()`
- `audio_task.rs`: Type aliases `Enc<P>`, `Dec<P>`, `CapStream<P>`, `PlayStream<P>` for ergonomics
- `handle.rs`: `BackendHandle<P: Platform>` with PhantomData, all functions generic over `Transport`/`Platform`
- `handle.rs`: `send_envelope<T>`, `wait_for_server_hello<T>`, `wait_for_auth_result<T>`, `connect_to_server<T>`, `run_connection_task<P>`
- `handle.rs`: TorrentManager uses `&dyn Any` downcast to `QuinnTransport` (Phase 5f will extract properly)
- `handle.rs`: `is_cert_verification_error` replaced with `is_cert_error_message` (platform-agnostic)
- `handle.rs`: `run_receiver_task` takes `impl TransportRecvStream` instead of concrete `QuinnRecvStream`
- `lib.rs`: `pub type BackendHandle = handle::BackendHandle<NativePlatform>` — zero downstream breakage
- `rumble-client/audio.rs`: Added `Default` supertrait to `AudioBackend`
- Tests: `UserAudioState` tests use `MockDecoder` instead of concrete `VoiceDecoder`
- Storage (`P::Storage`) and KeyManager (`P::KeyManager`) not yet wired — deferred to Phase 5d

### Phase 5d — Switch `egui-test` to `rumble-client` + `rumble-native` — DONE (via alias)

The `pub type BackendHandle = handle::BackendHandle<NativePlatform>` alias in backend/lib.rs makes this a no-op. egui-test's imports all work unchanged:
- `BackendHandle` → type alias to `BackendHandle<NativePlatform>` ✓
- `State`, `Command`, `ConnectionState`, etc. → re-exported from api::types via backend::events ✓
- `SigningCallback` → already in api::types ✓
- `ConnectConfig`, `SfxKind`, RPC types → backend-native, no change needed ✓

**Deferred nice-to-haves** (not required for v2 migration):
- Refactor `key_manager.rs` to delegate to `NativeKeySigning` (currently works fine with custom key management)
- Migrate settings persistence to use `FileStorage` (currently works fine with hand-rolled JSON)

### Phase 5e — Switch `mumble-bridge` to `rumble-client` — DONE

Completed: mumble-bridge now uses `QuinnTransport` from rumble-native instead of raw quinn, and aws-lc-rs instead of ring.

**What was done:**
- Switched crypto provider from `ring` to `aws-lc-rs` (quinn, rustls, tokio-rustls features in Cargo.toml)
- Removed direct `quinn` dependency — bridge uses `rumble-native::QuinnTransport` and `rumble-native::QuinnConnection`
- Rewrote `rumble_client.rs`: `connect()` uses `QuinnTransport::connect()` with `TlsConfig { accept_invalid_certs: true }` instead of manual endpoint setup
- `RumbleConnection` holds `QuinnTransport` instead of raw `quinn::Connection` + `SendStream` + `RecvStream` + `BytesMut`
- All `send_*` functions take `&mut QuinnTransport`, use `transport.send()` via shared `send_envelope()` helper
- `read_envelope()` uses `transport.recv()` instead of manual buffer management
- Deleted `make_bridge_endpoint()`, `AcceptAnyCert` struct, `compute_server_cert_hash()` — replaced by Transport trait
- `main.rs`: connection decomposition uses `transport.take_recv()` + `transport.connection()` for recv task + datagram reading
- `bridge.rs`: `run_bridge` takes `QuinnConnection` + `&mut QuinnTransport` instead of raw quinn types
- `mumble_tls.rs`: switched from `ring::default_provider()` to `aws_lc_rs::default_provider()`
- Added `pub use quinn::Connection as QuinnConnection` to `rumble-native/lib.rs` for bridge access

### Phase 5f — Deprecate `backend` crate — DONE (dead code removed)

Completed: Dead concrete implementations removed from backend. The crate now contains only platform-agnostic logic and thin re-exports.

**What was done:**
- `audio.rs`: Removed `AudioSystem`, `AudioInput`, `AudioOutput`, `AudioConfig`, `InputProcessor`, helpers (~690 lines deleted). Kept `SAMPLE_RATE`, `CHANNELS` constants and `AudioDeviceInfo` re-export.
- `codec.rs`: Removed `VoiceEncoder`, `VoiceDecoder`, `CodecError`, `EncoderStats`, `DecoderStats`, `opus_version()` and most tests (~660 lines deleted). Kept `is_dtx_frame()`, constant re-exports, and DTX test.
- `cert_verifier.rs`: Already just re-exports from rumble-client + rumble-native (no changes needed).
- `Cargo.toml`: Removed `cpal` and `opus` direct dependencies (now only needed via rumble-native).
- `lib.rs`: Removed dead type re-exports (`AudioSystem`, `AudioInput`, `AudioOutput`, `VoiceEncoder`, `VoiceDecoder`, etc.)
- `audio_task.rs`: Replaced `FRAME_SIZE` import with `OPUS_FRAME_SIZE` from codec module.

**What remains in backend** (platform-agnostic client logic):
- `handle.rs`: Generic `BackendHandle<P: Platform>` — connection task, command handling
- `audio_task.rs`: Generic audio task — jitter buffers, mixing, voice I/O
- `bounded_voice.rs`, `sfx.rs`, `synth.rs`: Pure Rust utilities
- `events.rs`, `processors.rs`, `rpc.rs`: State types, pipeline wrappers, RPC
- `audio_dump.rs`: Debug utility
- `torrent.rs`, `p2p.rs`: Still use raw quinn (future: extract to rumble-native)
- `cert_verifier.rs`: Thin re-export shim

**Future work** (not blocking v2):
- Move `torrent.rs` to rumble-native as `FileTransferPlugin` impl
- Move `p2p.rs` to rumble-native (feature-gated)
- Remove remaining `quinn`/`rustls-pemfile`/`webpki-roots` deps once torrent + cert loading migrated

### Phase 6 — WASM (deferred indefinitely)

No work planned. The trait infrastructure is ready for when WASM threading stabilizes.

---

## Test Coverage Gaps

| Module | Tests | Notes |
|--------|-------|-------|
| `rumble-native/transport.rs` | 0 | Needs integration test with actual framing |
| `rumble-native/audio.rs` | 0 | Hard to test without audio hardware |
| `rumble-native/cert_verifier.rs` | 0 | Should test fingerprint matching logic |
| `rumble-native/keys.rs` | 4 | Good coverage of local keys; SSH agent untestable in CI |
| `rumble-native/codec.rs` | 5 | Good coverage |
| `rumble-native/storage.rs` | 7 | Good coverage |
| `rumble-client` (traits) | 0 | Could add mock Platform impl for trait boundary testing |

**Recommendation:** Add at least a framing roundtrip test for `encode_frame_raw` / `try_decode_frame` to verify the wire format fix.

---

## Suggested Sprint Order

The remaining work has clear dependencies:

```
5b (Transport in handle.rs)
  └→ 5c (BackendHandle<P>)
       ├→ 5d (egui-test switch)
       └→ 5f (deprecate backend)

5e (mumble-bridge) — independent, blocked by crypto provider swap
```

**Recommended next sprint:** Phase 5b — integrate Transport trait into handle.rs. This is the hardest remaining piece and unblocks everything else. The interactive cert verification redesign (option B above) is the key design decision.
