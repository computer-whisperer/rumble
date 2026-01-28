# Hybrid P2P Architecture Requirements Specification

## 1. Overview

This document specifies requirements for a hybrid networking architecture in Rumble that combines:
- **Existing QUIC transport** for control messages and server-relayed voice
- **libp2p** for peer connectivity, NAT traversal, and file transfer
- **Optional P2P voice** using hole-punched connections

**Current status (2026-01-04)**
- Session certificates are exchanged in the client/server handshake and session ids are tracked in client state and server-side mappings.
- libp2p scaffold (identify, relay client, dcutr, request-response file-transfer behaviour) is implemented behind the `p2p` feature flag; not yet replacing TorrentManager in production.
- Hole-punch flow and server-hosted relay wiring remain TODO; P2P voice is still planned.

The server remains authoritative for room state, user presence, and access control. P2P capabilities augment rather than replace the server-centric model.

---

## 2. Goals

1. **File transfer works across NATs** - Peers behind NAT can share files without manual port forwarding
2. **Optional P2P voice** - Direct voice between peers when possible, reducing server bandwidth
3. **Attestable peer identity** - Clients can cryptographically verify each other's identity
4. **Bounded peer set** - Leverage server knowledge to limit peer discovery scope
5. **Client control** - Users can opt out of P2P features for privacy or performance
6. **Multi-session support** - Same user on multiple devices with distinct session identities
7. **Backward compatibility** - Existing server-relay mode remains fully functional

## 3. Non-Goals

1. Fully decentralized operation (server remains required for rooms/auth)
2. Anonymous communication (users are authenticated)
3. Global DHT or open peer discovery
4. WebRTC browser support (native clients only, initially)

---

## 4. Identity Model

### 4.1. Current State

- `user_id: u64` assigned by server, not cryptographically bound to client
- Ed25519 keypairs used for authentication but not for peer-to-peer attestation
- No mechanism for clients to verify each other's identity directly

### 4.2. Required Changes

#### 4.2.1. User Identity (Long-Term)

| Property    | Requirement                               |
| ----------- | ----------------------------------------- |
| Format      | Ed25519 public key (32 bytes)             |
| Display     | Base32 or similar human-readable encoding |
| Persistence | Stored in user's key file, never changes  |

#### 4.2.2. Session Identity (Per-Connection)

| Property       | Requirement                                |
| -------------- | ------------------------------------------ |
| Format         | Ephemeral Ed25519 keypair + session token  |
| Lifetime       | Single connection to server                |
| Binding        | Session key signed by user's long-term key |
| Purpose        | Distinguish multiple clients of same user  |
| P2P Identifier | `SessionId = Hash(SessionPublicKey)`       |
| Status         | Implemented: session certs exchanged in handshake and stored in client/server state |

#### 4.2.3. Identity Hierarchy

```
User Identity (long-term Ed25519 keypair)
 └─► signs ──► Session Certificate
                 ├── Session public key (ephemeral)
                 ├── Timestamp / expiry
                 └── Optional: device name
```

When connecting P2P, peers exchange session certificates. Each peer can verify:
1. The session cert is signed by a valid user key
2. The user key is someone the server says is in the room

---

## 5. Trust Model

### 5.1. Current State

- Server presents TLS certificate (self-signed or CA-issued)
- Client uses TOFU (Trust On First Use) or pinned fingerprint
- client uses weppki to verify server cert

### 5.2. Proposed: DNS-Based Server Trust

#### 5.2.1. DNS TXT Record Format

```
_rumble-key.example.com TXT "v=1 k=ed25519 p=<base64-pubkey> fp=<cert-fingerprint>"
```

| Field | Description                                                      |
| ----- | ---------------------------------------------------------------- |
| `v`   | Version (1)                                                      |
| `k`   | Key type (ed25519)                                               |
| `p`   | Server's Ed25519 public key (base64)                             |
| `fp`  | SHA-256 fingerprint of server's TLS cert (optional, for pinning) |

#### 5.2.2. Trust Verification Flow

1. Client resolves `_rumble-key.<server-domain>` TXT record
2. Client connects to server via QUIC
3. Server proves possession of the Ed25519 key during handshake
4. Client verifies server's signature matches DNS-published key
5. Trust established without CA dependency

#### 5.2.3. Fallback Modes

- **TOFU**: If DNS record absent, fall back to current fingerprint pinning
- **Manual pin**: User can explicitly trust a server fingerprint
- **DNSSEC**: When available, provides stronger guarantee

---

## 6. P2P Connectivity Layer (libp2p)

### 6.1. Purpose

libp2p provides:
- NAT traversal (DCUtR hole punching)
- Relay fallback (Circuit Relay v2)
- Peer address discovery
- Stream multiplexing for file transfer

### 6.2. Integration Points

| Component               | Transport                            |
| ----------------------- | ------------------------------------ |
| Control messages        | Existing QUIC (server only)          |
| Voice (server relay)    | Existing QUIC datagrams              |
| Voice (P2P)             | Direct QUIC datagrams (hole-punched) |
| File transfer           | libp2p streams                       |
| Hole punch coordination | libp2p DCUtR                         |

### 6.3. Swarm Scope

The libp2p swarm is **not** global. It operates within bounds set by the server:

- **Peer list**: Only peers in the same room(s)
- **Relay**: Server acts as relay node
- **Discovery**: No DHT; peers announced via control channel

### 6.4. libp2p Behaviours Required

| Behaviour               | Purpose                                   |
| ----------------------- | ----------------------------------------- |
| `identify`              | Exchange observed addresses               |
| `relay::client`         | Reserve relay slot on server              |
| `dcutr`                 | Direct Connection Upgrade through Relay   |
| Custom: `file-transfer` | BitTorrent-like protocol for file sharing |

**Implementation status:** Behaviours above are scaffolded in the backend under the `p2p` feature; integration with production file transfer is still pending.

### 6.5. Identity Mapping

- libp2p `PeerId` derived from **session key** (not user key)
- Server maintains mapping: `PeerId ↔ (UserId, SessionId)`
- Clients receive this mapping via control channel

---

## 7. P2P Voice Requirements

### 7.1. Mode Selection

P2P voice is **all-or-nothing** per client. A client either:
- **P2P Mode**: Sends voice directly to all peers in the room
- **Server Relay Mode**: Sends voice only to server for distribution

There is no partial mesh or selective routing. This simplifies implementation and reasoning about network load.

### 7.2. Activation Criteria

A client uses P2P voice mode when ALL of the following are true:

1. User has enabled P2P voice in settings (opt-in)
2. Client has sufficient upstream bandwidth for full mesh
3. NAT traversal succeeded to at least one peer
4. Server indicates P2P is allowed for this room

If any condition fails, the client uses server relay mode.

### 7.3. Bandwidth Requirement

For P2P mode, client must have upstream capacity for:

```
required_bandwidth = (num_peers_in_room - 1) × voice_bitrate
```

Example: 5 users in room, 64 kbps Opus = 256 kbps upstream required

If bandwidth is insufficient, client uses server relay (sends one stream to server, server distributes).

### 7.4. Server's Role

- Authoritative room membership
- Broadcasts which peers are P2P-capable and their addresses
- Receives voice from relay-mode clients and distributes to room
- Does NOT receive voice from P2P-mode clients (saves bandwidth) except when there is a relay mode user in the room
- Tracks which mode each client is using

### 7.5. Mixed Mode Rooms

When some clients use P2P and others use relay:

- P2P clients send directly to other P2P clients
- P2P clients also send to server for relay clients
- Relay clients only send to server
- Server distributes relay client voice to everyone

### 7.6. Fallback to Server Relay

A client switches from P2P to relay mode when:
- Multiple P2P connections fail
- User manually disables P2P
- Bandwidth becomes insufficient

### 7.7. Multi-Session Handling

When same user has multiple sessions (e.g., desktop + mobile):

- Each session has distinct `SessionId` / `PeerId`
- Server tracks "active session" for voice (user chooses or most recent)
- File transfers may target all sessions of a user

---

## 8. File Transfer Requirements

### 8.1. Protocol

File transfer uses a BitTorrent-like protocol over libp2p streams:

- Piece-based transfer with integrity verification
- Multi-source downloading
- Resume support

### 8.2. Peer Discovery

- Sender announces file to server (infohash, metadata)
- Server broadcasts to room
- Interested peers connect via libp2p
- No external trackers or DHT

### 8.3. Relay Support

- If direct connection fails, use server's circuit relay
- Bandwidth-limited relay (prevent abuse)

### 8.4. Privacy Considerations

- Clients can disable file transfer reception
- IP addresses visible to peers during direct transfer
- Relay mode available for privacy (server sees content but peers don't see IPs)

---

## 9. Client Privacy & Control

### 9.1. User-Configurable Options

| Option               | Default | Description                           |
| -------------------- | ------- | ------------------------------------- |
| `enable_p2p_voice`   | `true`  | Allow direct voice connections        |
| `enable_p2p_files`   | `true`  | Allow direct file transfer            |
| `prefer_relay`       | `false` | Always use server relay (hides IP)    |
| `bandwidth_tier`     | `auto`  | Self-reported capacity                |
| `share_presence_p2p` | `true`  | Allow peers to see when you're online |

### 9.2. IP Address Exposure

| Mode              | Server Sees IP | Peers See IP |
| ----------------- | -------------- | ------------ |
| Server relay only | ✓              | ✗            |
| P2P direct        | ✓              | ✓            |
| P2P via relay     | ✓              | ✗            |

### 9.3. Opt-Out Guarantees

- Disabling P2P must not degrade core functionality
- Server relay always available as fallback

---

## 10. Server Requirements

### 10.1. New Responsibilities

| Responsibility       | Description                                      |
| -------------------- | ------------------------------------------------ |
| Peer registry        | Map `SessionId` ↔ libp2p `PeerId` ↔ `UserId`     |
| Relay node           | Run libp2p relay service for NAT'd peers         |
| Topology hints       | Suggest P2P mesh structure based on capabilities |
| Bandwidth tracking   | Monitor relay usage, enforce limits              |
| Session certificates | countersign session certs for extra trust        |

### 10.2. Protocol Extensions

New control message types:

| Message            | Direction | Purpose                             |
| ------------------ | --------- | ----------------------------------- |
| `PeerAnnounce`     | S→C       | Announce peer's libp2p addresses    |
| `PeerCapabilities` | C→S       | Report P2P capabilities             |
| `P2PVoiceStatus`   | C→S       | Report active P2P voice connections |
| `TopologyHint`     | S→C       | Suggest P2P mesh structure          |
| `RelayAllocation`  | S→C       | Provide relay reservation details   |

---

## 11. Migration Strategy

### 11.1. Phase 1: Identity Foundation

Status: **Done**

1. Add session certificates to handshake ✅
2. Clients generate ephemeral session keypairs ✅
3. Server tracks session keys alongside user_id ✅
4. No P2P yet; just establish identity model ✅

### 11.2. Phase 2: libp2p File Transfer

Status: **In progress**

1. Integrate libp2p swarm in backend ✅ (behind `p2p` feature)
2. Implement file transfer behaviour ✅ (request-response codec scaffold)
3. Server runs relay service ⏳ (not yet wired)
4. Replace current torrent transport with libp2p-based ⏳ (TorrentManager still in use)

### 11.3. Phase 3: Hole Punching

Status: **Planned** (dcutr behaviour scaffolded; end-to-end flow not wired)

1. Enable DCUtR for direct peer connections
2. File transfers use direct connection when available
3. Collect metrics on NAT traversal success rates

### 11.4. Phase 4: P2P Voice

Status: **Planned**

1. Add voice transport over hole-punched connection
2. Implement bandwidth-aware topology
3. Fallback to server relay when needed
4. Careful rollout with opt-in

### 11.5. Compatibility

- Clients supporting only Phase 1 can still use server relay
- P2P features gracefully degrade when peers don't support them
- Server can disable P2P features globally if needed

---

## 12. Security Considerations

### 12.1. Threats

| Threat                | Mitigation                                                                   |
| --------------------- | ---------------------------------------------------------------------------- |
| Impersonation         | Session certs signed by user key, verified by peers, countersigned by server |
| Man-in-the-middle     | TLS on all connections; peer identity in cert                                |
| Relay abuse           | Rate limits, user authentication required                                    |
| IP harvesting         | Relay mode option hides peer IPs                                             |
| Amplification attacks | QUIC has built-in anti-amplification                                         |

### 12.2. Trust Boundaries

- **Fully trusted**: Own client, own keys
- **Authenticated**: Server (via DNS/TLS), room members (via session certs)
- **Untrusted**: Network path, relay nodes (encryption protects content)

---

## 16. Current Progress (2026-01-04)

- Identity foundation ✅ session certs exchanged and stored; peer mapping by session id exists.
- libp2p scaffold ✅ identify + relay client + dcutr behaviours and request-response file-transfer codec are implemented behind the `p2p` feature.
- File transfer path 🚧 wired for direct P2P in backend; torrent transport still primary; relay path flaky/disabled in tests (circuit negotiation pending).
- Relay service 🚧 local test relay helper exists; server-hosted relay integration still to be wired and tuned (reservations/rate limits not enforced yet).
- Hole punching 🚧 dcutr behaviour present but no end-to-end punch orchestration yet.
- P2P voice ⏳ not implemented; requires hole punch plus bandwidth-aware topology.
- Control-plane extensions ✅ PeerAnnounce, PeerCapabilities, P2PVoiceStatus, RelayAllocation messages defined in protocol; server broadcasts PeerAnnounce on peer join/leave; clients send PeerCapabilities after P2P manager init.

### 12.3. Key Management

- User key: Long-term, backed up, never leaves device
- Session key: Ephemeral, per-connection, may be in memory only
- Server key: Published in DNS, used for signing not encryption

---

## 13. Multi-Room Behavior

### 13.1. File Sharing

File sharing is **not** strictly segregated by room:

- A file shared in Room A can be downloaded by a peer you only share Room B with
- libp2p connections are per-session, not per-room
- Access control for files is separate from room membership (future consideration)

### 13.2. P2P Voice

P2P voice connections are established with **all peers you share any room with**:

- If you're in Room A (with users X, Y) and Room B (with users Y, Z), you establish P2P links with X, Y, and Z
- Voice is only *sent* to peers in your *current* room
- The P2P connection remains available for quick room switches
- Reduces latency when moving between rooms

---

## 14. Open Questions

1. **Key rotation**: How/when do session keys rotate during long sessions?

---

## 15. Appendix: Terminology

| Term        | Definition                                              |
| ----------- | ------------------------------------------------------- |
| **User**    | A registered identity with long-term keypair            |
| **Session** | A single client connection, with ephemeral keys         |
| **PeerId**  | libp2p identifier, derived from session key             |
| **UserId**  | Stable identifier derived from user's public key        |
| **Room**    | Server-managed group where users can communicate        |
| **Relay**   | Server-mediated connection for NAT'd peers              |
| **DCUtR**   | Direct Connection Upgrade through Relay (hole punching) |
