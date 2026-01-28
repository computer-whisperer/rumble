```
┌─────────────────────────────────────────────────────────────────────────────────┐
│                                UI THREAD (Main)                                  │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ BackendHandle                                                              │  │
│  │  • state: Arc<RwLock<State>>  ◄──────────── Shared State (read by UI)     │  │
│  │  • command_tx: mpsc::UnboundedSender<Command> ──────────────┐             │  │
│  │  • audio_task: AudioTaskHandle ─────────────────────────────┼──┐          │  │
│  │  • repaint_callback: Arc<dyn Fn()> ◄────────────────────────┼──┼──────┐   │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────┬─────────────────────┬─────────┬──────┘
                                           │                     │         │
                    Commands (non-audio)   │    AudioCommands    │ Repaint │
                                           ▼                     ▼         │
┌─────────────────────────────────────────────────────────────────────────────────┐
│                         CONNECTION THREAD (std::thread)                          │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ Tokio Runtime (multi-threaded)                                             │  │
│  │  ┌─────────────────────────────────────────────────────────────────────┐  │  │
│  │  │ run_connection_task                                                  │  │  │
│  │  │  • command_rx: mpsc::UnboundedReceiver<Command>                     │  │  │
│  │  │  • state: Arc<RwLock<State>> (write access)                         │  │  │
│  │  │  • connection: Option<quinn::Connection>                            │  │  │
│  │  │  • send_stream: Option<quinn::SendStream>                           │  │  │
│  │  │                                                                      │  │  │
│  │  │  Handles:                                                            │  │  │
│  │  │   • Connect/Disconnect commands                                      │  │  │
│  │  │   • Room operations (Join/Create/Delete/Rename)                      │  │  │
│  │  │   • Chat messages                                                    │  │  │
│  │  │   • Mute/Deafen status → server                                      │  │  │
│  │  └──────────────────────────────────┬──────────────────────────────────┘  │  │
│  │                                     │                                      │  │
│  │                         On connect: spawns                                 │  │
│  │                                     ▼                                      │  │
│  │  ┌─────────────────────────────────────────────────────────────────────┐  │  │
│  │  │ run_receiver_task (tokio::spawn)                                     │  │  │
│  │  │  • recv: quinn::RecvStream (reliable messages only)                  │  │  │
│  │  │  • Handles: ServerEvent, ChatBroadcast, StateUpdate                  │  │  │
│  │  │  • Updates state on server messages                                  │  │  │
│  │  │  • Notifies audio_task of room changes (user joined/left/moved)      │  │  │
│  │  └─────────────────────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                         │                                                        │
│    AudioCommand::ConnectionEstablished(conn, user_id) ──────────┐               │
│    AudioCommand::ConnectionClosed ──────────────────────────────┤               │
│    AudioCommand::UserJoinedRoom/LeftRoom/RoomChanged ───────────┤               │
└─────────────────────────────────────────────────────────────────┼───────────────┘
                                                                  │
                                                                  ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│                           AUDIO THREAD (std::thread)                             │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ Tokio Runtime (single-threaded: current_thread)                           │  │
│  │  ┌─────────────────────────────────────────────────────────────────────┐  │  │
│  │  │ run_audio_task (main event loop with tokio::select!)                 │  │  │
│  │  │                                                                      │  │  │
│  │  │  Owns:                                                               │  │  │
│  │  │   • AudioSystem (cpal host - not Send)                               │  │  │
│  │  │   • connection: Option<quinn::Connection> (for datagrams)            │  │  │
│  │  │   • encoder: Arc<Mutex<Option<VoiceEncoder>>> (connection-scoped)    │  │  │
│  │  │   • user_audio: HashMap<u64, UserAudioState> (per-user decoders)     │  │  │
│  │  │   • audio_input: Option<AudioInput>                                  │  │  │
│  │  │   • audio_output: Option<AudioOutput>                                │  │  │
│  │  │   • processor_registry: ProcessorRegistry                            │  │  │
│  │  │   • tx/rx pipeline configs                                           │  │  │
│  │  │                                                                      │  │  │
│  │  │  Event Loop (tokio::select!):                                        │  │  │
│  │  │   1. command_rx.recv() → Process AudioCommands                       │  │  │
│  │  │   2. encoded_rx.recv() → Send voice datagrams (from capture)         │  │  │
│  │  │   3. conn.read_datagram() → Receive voice datagrams → jitter buffer  │  │  │
│  │  │   4. mix_interval (20ms) → Mix & play audio from all jitter buffers  │  │  │
│  │  │   5. cleanup_interval (500ms) → Remove stale talking users           │  │  │
│  │  │   6. stats_interval (500ms) → Update audio statistics in state       │  │  │
│  │  └─────────────────────────────────────────────────────────────────────┘  │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                  │
│                    Creates/manages (via cpal)                                    │
│                              │                                                   │
│              ┌───────────────┴───────────────┐                                  │
│              ▼                               ▼                                  │
│  ┌───────────────────────┐     ┌───────────────────────┐                       │
│  │ CPAL INPUT THREAD     │     │ CPAL OUTPUT THREAD    │                       │
│  │ (audio driver thread) │     │ (audio driver thread) │                       │
│  │                       │     │                       │                       │
│  │ AudioInput callback:  │     │ AudioOutput callback: │                       │
│  │  • Receives PCM from  │     │  • Reads from         │                       │
│  │    microphone         │     │    playback_buffer    │                       │
│  │  • Runs TX pipeline   │     │    (Arc<Mutex<VecDeque>>)                     │
│  │    (denoise, VAD)     │     │  • Fills output buffer│                       │
│  │  • Encodes with Opus  │     │    with mixed audio   │                       │
│  │  • Sends via          │     │                       │                       │
│  │    encoded_tx channel │     │                       │                       │
│  └───────────┬───────────┘     └───────────────────────┘                       │
│              │                               ▲                                  │
│              │ CaptureMessage                │ queue_samples()                  │
│              │ (EncodedFrame / EndOfStream)  │ (called from mix_and_play_audio) │
│              ▼                               │                                  │
│  ┌──────────────────────────────────────────────────────────────────────────┐  │
│  │ Audio Task Event Loop (continues above)                                   │  │
│  │  • encoded_rx receives frames → sends as QUIC datagrams                   │  │
│  │  • mix_interval ticks → decodes jitter buffers → mixes → queue_samples() │  │
│  └──────────────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────────────┘
```

## Thread Summary

| Thread | Runtime | Purpose | Key Responsibilities |
|--------|---------|---------|---------------------|
| **UI Thread** | Main | User interface | Renders state, sends commands |
| **Connection Thread** | std::thread + Tokio (multi) | Network I/O (reliable) | QUIC streams, protocol messages, state sync |
| **Receiver Task** | tokio::spawn (on Connection Thread) | Server events | Processes incoming reliable messages |
| **Audio Thread** | std::thread + Tokio (single) | Voice processing | Datagrams, encoding/decoding, mixing |
| **cpal Input Thread** | OS audio driver | Microphone capture | Runs TX pipeline, encodes Opus |
| **cpal Output Thread** | OS audio driver | Speaker playback | Pulls mixed samples from buffer |

## Key Synchronization Mechanisms

1. **Shared State** (`Arc<RwLock<State>>`):
   - Written by: Connection Thread, Audio Thread
   - Read by: UI Thread
   - Purpose: Single source of truth for UI rendering

2. **Command Channel** (`mpsc::UnboundedSender<Command>`):
   - From: UI Thread → Connection Thread
   - Purpose: Non-audio commands (connect, join room, chat, etc.)

3. **Audio Command Channel** (`mpsc::UnboundedSender<AudioCommand>`):
   - From: UI Thread, Connection Thread → Audio Thread
   - Purpose: Audio device control, connection lifecycle, room changes

4. **Encoded Frames Channel** (`mpsc::UnboundedSender<CaptureMessage>`):
   - From: cpal Input Thread → Audio Thread
   - Purpose: Encoded Opus frames ready to send as datagrams

5. **Playback Buffer** (`Arc<Mutex<VecDeque<f32>>>`):
   - Written by: Audio Thread (mix_and_play_audio)
   - Read by: cpal Output Thread
   - Purpose: Mixed audio samples for playback

6. **Repaint Callback** (`Arc<dyn Fn() + Send + Sync>`):
   - Called by: Connection Thread, Audio Thread
   - Triggers: UI Thread repaint
   - Purpose: Notify UI of state changes

## Design Rationale

1. **Separation of Connection and Audio**: Audio never blocks on reliable message I/O, ensuring minimal voice latency.

2. **Single-threaded Audio Runtime**: Uses `current_thread` Tokio runtime to avoid cross-thread sync overhead in the hot audio path.

3. **Connection-scoped Encoder**: Opus encoder persists for the entire connection lifetime, maintaining DTX state across PTT presses.

4. **Server-driven Decoder Lifecycle**: Decoders are created proactively when users join the room (via `UserJoinedRoom` command), not lazily on first packet.

5. **Jitter Buffer per User**: Each user has their own jitter buffer and decoder, allowing independent timing and FEC recovery.