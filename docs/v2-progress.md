# V2 Architecture Migration — Progress & Remaining Work

**Last updated:** 2026-03-17
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
| **5b** | Full Transport integration | Done | `7b5485d` |
| **5c** | `BackendHandle<P: Platform>` | Done | `7b5485d` |
| **5d** | Switch `egui-test` to rumble-client | Done (via alias) | `7b5485d` |
| **5e** | Switch `mumble-bridge` to rumble-client | Done | `7b5485d` |
| **5f** | Backend dead code removal | Done | `7b5485d` |
| **5g** | Auth deduplication | Done | `7a568be` |
| **5h–5i** | BitTorrent relay (superseded) | Deprecated | See file transfer rework |
| **6** | WASM platform | Deferred indefinitely | — |

### File Transfer Rework (2026-03-17)

| Phase | Description | Status | Commits |
|-------|-------------|--------|---------|
| Bug fixes | Path traversal, infohash dedup, proto enum | Done | `f30d7ad` |
| MIME consolidation | Replace manual match with `mime_guess` | Done | `98e0ee6` |
| Torrent/P2P deprecation | Remove ~9,800 lines of BitTorrent + libp2p | Done | `7309300` |
| Client-side stream dispatch | BiStreamHandle, StreamHeader, dispatch loop | Done | `9cd5dee` |
| Relay rework | Store-and-serve cache model (server + client) | Done | `8aeb269` |
| Review fixes | room_id wiring, truncation check, quota fix | Done | `d0b1602` |
| Plugin config system | PluginFactory trait, TOML config, humantime/bytesize | Done | `642dee0` |

---

## Current Architecture

### Platform Abstraction (Phases 1–5f)

All 7 trait families in `rumble-client` implemented in `rumble-native`:

| Trait | Impl | File |
|-------|------|------|
| `Platform` | `NativePlatform` | `platform.rs` |
| `Transport` + `DatagramTransport` + `BiStreamHandle` | `QuinnTransport` | `transport.rs` |
| `AudioBackend` | `CpalAudioBackend` | `audio.rs` |
| `VoiceCodec` | `NativeOpusCodec` | `codec.rs` |
| `PersistentStorage` | `FileStorage` | `storage.rs` |
| `KeySigning` | `NativeKeySigning` | `keys.rs` |
| `FileTransferPlugin` | `FileTransferRelayPlugin` | `file_transfer_relay.rs` |

`BackendHandle<P: Platform>` is fully generic. Backend contains only platform-agnostic client logic — no cpal, quinn, opus, librqbit, or libp2p dependencies.

### File Transfer: Server-Cached Relay

Replaced BitTorrent/P2P file transfer with a store-and-serve relay:

**Server-side** (`relay_plugin.rs`):
- `FileTransferRelayPlugin` with `DashMap` cache, room-scoped entries
- Upload: client streams file → server caches by transfer_id
- Fetch: client requests by transfer_id → server streams from cache
- Configurable via TOML: TTL, max file size, max total cache, room eviction
- TTL sweep task, proper shutdown via CancellationToken
- `PluginFactory` trait for config-driven construction

**Client-side** (`file_transfer_relay.rs`):
- `FileTransferRelayPlugin` using `StreamOpener` (transport-agnostic)
- `share()`: upload to server, return FileOffer with share_data JSON
- `download()`: fetch from server by transfer_id
- Real cancellation via per-transfer `CancellationToken`
- `parking_lot::Mutex` for transfer state
- Room tracking via `set_room_id()` on join/move

**Wire protocol** on `"file-relay"` streams:
- StreamHeader → type byte (0x01=upload, 0x02=fetch) → length-prefixed proto → raw file bytes
- Proto: `RelayUpload`, `RelayUploadResponse`, `RelayFetch`, `RelayFetchResponse`, `RelayResult` enum

### Client-Side Stream Dispatch

Mirrors server's stream routing pattern:
- `BiSendStream` / `BiRecvStream` traits for stream halves
- `BiStreamHandle` trait (cloneable, like `DatagramTransport`)
- `StreamHeader` shared between client and server
- `run_stream_dispatch()` in backend: accepts bi-streams, reads header, dispatches to plugins
- `StreamOpener` trait for plugins to open outgoing streams

### Server Plugin Config System

- `PluginFactory` trait: plugins define config structs, deserialize from `[plugins.<name>]` TOML
- `RelayCacheConfig` uses `bytesize` ("50 MB") and `humantime_serde` ("30m")
- Factory-based construction in server startup
- Unknown config sections logged as warnings

Example config:
```toml
[plugins.file-relay]
ttl = "30m"
max_file_size = "100 MB"
max_total_size = "500 MB"
evict_on_room_clear = true
```

---

## What Was Removed

### BitTorrent/P2P System (~9,800 lines)

| Component | Location | Status |
|-----------|----------|--------|
| `BitTorrentFileTransfer` | `rumble-native/src/file_transfer_bittorrent.rs` | Deleted |
| `TorrentManager` | `rumble-native/src/torrent.rs` | Deleted |
| `FileTransferBittorrentPlugin` | `server/src/tracker_plugin.rs` | Deleted |
| `Tracker` | `server/src/tracker.rs` | Deleted |
| TCP relay service | `server/src/relay.rs` | Deleted |
| P2P manager | `backend/src/p2p.rs` | Deleted |
| `librqbit` dependency | `rumble-native/Cargo.toml` | Removed |
| `libp2p` dependency | `backend/Cargo.toml` | Removed |
| Proto fields 50–53 (tracker) | `api.proto` | Removed |
| Proto fields 60–63 (P2P voice) | `api.proto` | Removed |
| `dyn Any` downcast hack | `backend/src/handle.rs` | Removed |
| File transfer UI (magnet links, download modal) | `egui-test/src/app.rs` | Removed |
| `FileMessage`, `P2pFileMessage`, `FileTransferState` | `api/src/types.rs` | Removed |
| File transfer Command variants | `api/src/types.rs` | Removed |

---

## Bugs Fixed (2026-03-17)

| Bug | Fix |
|-----|-----|
| Path traversal in relay download | Sanitize network filename with `Path::file_name()` |
| Zero infohash dedup collision | Skip dedup for all-zero infohashes |
| Proto enum `ACCEPTED = 0` | Add `UNSPECIFIED = 0`, shift `ACCEPTED` to 1 |
| Relay cancel was a no-op | `CancellationToken` per transfer task |
| Tasks outlive plugin on shutdown | Parent `CancellationToken` cancelled in `stop()` |
| `std::sync::Mutex` poison ignored | Switched to `parking_lot::Mutex` |
| Incoming stream listener steals bi-streams | Centralized client-side stream dispatch |
| `dyn Any` downcast hack | Removed with torrent deprecation; relay uses `StreamOpener` |
| Fragile magnet link parsing | Removed with torrent deprecation |
| Duplicate MIME guessing | Consolidated to `mime_guess` crate |
| `room_id` never set on relay plugin | Wired `set_room_id()` on connect + room change |
| Truncated downloads accepted silently | Validate received bytes vs file_size |
| Cache quota drift on overwrite | Subtract old entry size before inserting |

---

## Remaining Work

| Item | Priority | Notes |
|------|----------|-------|
| File transfer UI rework | Medium | egui-test needs new UI for relay-based sharing (old torrent UI was removed) |
| Wire `P::Storage` and `P::KeyManager` into BackendHandle | Low | Currently deferred nice-to-haves |
| WASM platform (Phase 6) | Deferred | Trait infrastructure ready when WASM threading stabilizes |

---

## Test Coverage

| Module | Tests | Notes |
|--------|-------|-------|
| `rumble-native/transport.rs` | 0 | Needs integration test with framing roundtrip |
| `rumble-native/audio.rs` | 0 | Hard to test without audio hardware |
| `rumble-native/cert_verifier.rs` | 0 | Should test fingerprint matching logic |
| `rumble-native/keys.rs` | 4 | Good coverage of local keys; SSH agent untestable in CI |
| `rumble-native/codec.rs` | 5 | Good coverage |
| `rumble-native/storage.rs` | 7 | Good coverage |
| `rumble-native/file_transfer_relay.rs` | 0 | Needs integration test with mock server |
| `server/relay_plugin.rs` | 0 | Needs integration test for upload/fetch/eviction |
| `rumble-client` (traits) | 0 | Could add mock Platform impl for trait boundary testing |
