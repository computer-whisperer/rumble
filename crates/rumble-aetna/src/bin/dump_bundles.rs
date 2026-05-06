//! Dump aetna bundle artifacts (svg + tree + draw_ops + lint + manifest)
//! for every canonical UI scene of `rumble-aetna`.
//!
//! Run:
//!   cargo run -p rumble-aetna --bin dump_bundles
//!   cargo run -p rumble-aetna --bin dump_bundles -- connected cert_pending
//!
//! Output: `crates/rumble-aetna/out/rumble_<scene>.{svg,tree.txt,draw_ops.txt,lint.txt,shader_manifest.txt}`.
//!
//! Mirrors the `aetna-volume::render_artifacts` shape: a small
//! `MockBackend` that returns a canned `State`, a `Scene` enum that
//! enumerates the views worth snapshotting, and the same four-line
//! `render_bundle` + `write_bundle` core that aetna-core ships in the
//! prelude. No GPU is involved — the SVG fallback renders the same
//! draw-op stream the wgpu Runner would, so layout regressions show
//! up faithfully without spinning up a window or device.

use std::{path::PathBuf, sync::Arc, time::SystemTime};

use aetna_core::prelude::*;

use rumble_aetna::{
    Identity, RumbleApp, SettingsOpenSelect, SettingsTab, UnlockState, WizardState, backend::UiBackend,
};
use rumble_desktop_shell::{KeyInfo, SettingsStore};
use rumble_protocol::{
    AudioDeviceInfo, AudioState, AudioStats, ChatMessage, ChatMessageKind, Command, ConnectionState,
    PendingCertificate, State, Uuid, VoiceMode,
    proto::{RoomInfo, User, UserId},
    room_id_from_uuid,
};

// ---------------------------------------------------------------------
// Mock backend
// ---------------------------------------------------------------------

/// Returns a canned `State` to the renderer and discards every command.
/// The fixture only exercises the read half of [`UiBackend`]; commands
/// would be no-ops here even if we did record them, since there's no
/// backend to apply them.
struct MockBackend {
    state: State,
}

impl UiBackend for MockBackend {
    fn state(&self) -> State {
        self.state.clone()
    }
    fn send(&self, _command: Command) {}
}

// ---------------------------------------------------------------------
// Scene catalog
// ---------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Scene {
    /// Idle disconnected state — toolbar shows the muted "Disconnected"
    /// pill and the welcome panel.
    Disconnected,
    /// Connect form open over the disconnected backdrop.
    ConnectModalOpen,
    /// Connection in progress.
    Connecting,
    /// Live session: rooms, users, chat — exercises the full shell.
    Connected,
    /// Connection lost with an error message in the toolbar.
    ConnectionLost,
    /// Cert acceptance modal up over the disconnected backdrop.
    CertPending,
    /// First-run wizard: choose between local key and ssh-agent.
    WizardSelectMethod,
    /// First-run wizard: local-key password entry.
    WizardGenerateLocal,
    /// First-run wizard: ssh-agent key picker.
    WizardSelectAgentKey,
    /// First-run wizard: terminal error screen.
    WizardError,
    /// Encrypted-key unlock prompt at startup.
    UnlockPrompt,
    /// Toolbar "Identity" modal showing the configured key + regenerate.
    IdentityModal,
    /// Settings dialog — Connection tab (default).
    SettingsConnection,
    /// Settings dialog — Devices tab with the input dropdown open.
    SettingsDevices,
    /// Settings dialog — Voice tab (encoder/jitter/PTT toggles).
    SettingsVoice,
    /// Settings dialog — Sounds tab (sfx toggles + per-event preview).
    SettingsSounds,
    /// Settings dialog — Chat tab with the timestamp-format dropdown open.
    SettingsChat,
    /// Settings dialog — Files tab (auto-download + bandwidth).
    SettingsFiles,
    /// Settings dialog — Stats tab (read-only audio metrics).
    SettingsStats,
}

impl Scene {
    const ALL: &'static [Scene] = &[
        Scene::Disconnected,
        Scene::ConnectModalOpen,
        Scene::Connecting,
        Scene::Connected,
        Scene::ConnectionLost,
        Scene::CertPending,
        Scene::WizardSelectMethod,
        Scene::WizardGenerateLocal,
        Scene::WizardSelectAgentKey,
        Scene::WizardError,
        Scene::UnlockPrompt,
        Scene::IdentityModal,
        Scene::SettingsConnection,
        Scene::SettingsDevices,
        Scene::SettingsVoice,
        Scene::SettingsSounds,
        Scene::SettingsChat,
        Scene::SettingsFiles,
        Scene::SettingsStats,
    ];

    fn slug(self) -> &'static str {
        match self {
            Scene::Disconnected => "disconnected",
            Scene::ConnectModalOpen => "connect_modal_open",
            Scene::Connecting => "connecting",
            Scene::Connected => "connected",
            Scene::ConnectionLost => "connection_lost",
            Scene::CertPending => "cert_pending",
            Scene::WizardSelectMethod => "wizard_select_method",
            Scene::WizardGenerateLocal => "wizard_generate_local",
            Scene::WizardSelectAgentKey => "wizard_select_agent_key",
            Scene::WizardError => "wizard_error",
            Scene::UnlockPrompt => "unlock_prompt",
            Scene::IdentityModal => "identity_modal",
            Scene::SettingsConnection => "settings_connection",
            Scene::SettingsDevices => "settings_devices",
            Scene::SettingsVoice => "settings_voice",
            Scene::SettingsSounds => "settings_sounds",
            Scene::SettingsChat => "settings_chat",
            Scene::SettingsFiles => "settings_files",
            Scene::SettingsStats => "settings_stats",
        }
    }

    fn build_state(self) -> State {
        match self {
            Scene::Disconnected => State::default(),
            Scene::ConnectModalOpen => State::default(),
            Scene::Connecting => State {
                connection: ConnectionState::Connecting {
                    server_addr: "rumble.example:5000".into(),
                },
                ..State::default()
            },
            Scene::Connected => connected_state(),
            Scene::ConnectionLost => State {
                connection: ConnectionState::ConnectionLost {
                    error: "stream closed by peer".into(),
                },
                ..State::default()
            },
            Scene::CertPending => State {
                connection: ConnectionState::CertificatePending {
                    cert_info: demo_pending_cert(),
                },
                ..State::default()
            },
            Scene::WizardSelectMethod
            | Scene::WizardGenerateLocal
            | Scene::WizardSelectAgentKey
            | Scene::WizardError
            | Scene::UnlockPrompt
            | Scene::IdentityModal
            | Scene::SettingsConnection
            | Scene::SettingsChat
            | Scene::SettingsVoice
            | Scene::SettingsSounds
            | Scene::SettingsFiles => State::default(),
            // The Devices scene needs realistic input/output device
            // lists so the dropdown menu is non-trivial; the Stats
            // scene needs non-zero counters so the read-only grid
            // shows real numbers.
            Scene::SettingsDevices => State {
                audio: device_state(),
                ..State::default()
            },
            Scene::SettingsStats => State {
                audio: stats_state(),
                ..State::default()
            },
        }
    }

    /// Drive any local UI state the scene needs by injecting synthetic
    /// events through the real `App::on_event` path. This means the
    /// rendered scene is exactly what the user would see after
    /// performing the same interaction — there's no "fixture-only"
    /// shortcut that the production code can drift away from.
    fn drive_setup(self, app: &mut RumbleApp<MockBackend>) {
        match self {
            Scene::ConnectModalOpen => {
                app.on_event(UiEvent::synthetic_click("connect:open"));
            }
            Scene::WizardSelectMethod => {
                app.set_wizard_state_for_test(WizardState::SelectMethod);
            }
            Scene::WizardGenerateLocal => {
                app.set_wizard_state_for_test(WizardState::GenerateLocal {
                    password: "hunter2".to_string(),
                    password_sel: aetna_core::TextSelection::default(),
                    confirm: "hunter".to_string(),
                    confirm_sel: aetna_core::TextSelection::default(),
                    error: None,
                });
            }
            Scene::WizardSelectAgentKey => {
                app.set_wizard_state_for_test(WizardState::SelectAgentKey {
                    keys: demo_agent_keys(),
                    selected: Some(1),
                    error: None,
                });
            }
            Scene::WizardError => {
                app.set_wizard_state_for_test(WizardState::Error {
                    message: "Failed to connect to SSH agent: SSH_AUTH_SOCK is not set".to_string(),
                });
            }
            Scene::UnlockPrompt => {
                app.set_unlock_state_for_test(UnlockState {
                    password: "••••".to_string(),
                    password_sel: aetna_core::TextSelection::default(),
                    error: Some("Wrong password — try again.".to_string()),
                });
            }
            Scene::IdentityModal => {
                app.set_identity_modal_open_for_test(true);
            }
            Scene::SettingsConnection => app.open_settings_for_test(SettingsTab::Connection),
            Scene::SettingsDevices => {
                app.open_settings_for_test(SettingsTab::Devices);
                app.open_settings_dropdown_for_test(SettingsOpenSelect::InputDevice);
            }
            Scene::SettingsVoice => app.open_settings_for_test(SettingsTab::Voice),
            Scene::SettingsSounds => app.open_settings_for_test(SettingsTab::Sounds),
            Scene::SettingsChat => {
                app.open_settings_for_test(SettingsTab::Chat);
                app.open_settings_dropdown_for_test(SettingsOpenSelect::TimestampFormat);
            }
            Scene::SettingsFiles => app.open_settings_for_test(SettingsTab::Files),
            Scene::SettingsStats => app.open_settings_for_test(SettingsTab::Stats),
            _ => {}
        }
    }

    /// True for scenes that purposefully render the first-run / unlock
    /// modal — those need the suppression hook left alone.
    fn keeps_first_run(self) -> bool {
        matches!(
            self,
            Scene::WizardSelectMethod
                | Scene::WizardGenerateLocal
                | Scene::WizardSelectAgentKey
                | Scene::WizardError
                | Scene::UnlockPrompt
        )
    }
}

// ---------------------------------------------------------------------
// Canned data for the "Connected" scene
// ---------------------------------------------------------------------

const ROOM_LOBBY: u128 = 0x1111_1111_1111_1111_1111_1111_1111_1111;
const ROOM_WORK: u128 = 0x2222_2222_2222_2222_2222_2222_2222_2222;

fn make_room(uuid: u128, name: &str) -> RoomInfo {
    RoomInfo {
        id: Some(room_id_from_uuid(Uuid::from_u128(uuid))),
        name: name.into(),
        parent_id: None,
        description: None,
        inherit_acl: false,
        acls: Vec::new(),
        effective_permissions: 0,
    }
}

fn make_user(id: u64, name: &str, room: u128, mut tweak: impl FnMut(&mut User)) -> User {
    let mut u = User {
        user_id: Some(UserId { value: id }),
        username: name.into(),
        current_room: Some(room_id_from_uuid(Uuid::from_u128(room))),
        is_muted: false,
        is_deafened: false,
        server_muted: false,
        is_elevated: false,
        groups: Vec::new(),
    };
    tweak(&mut u);
    u
}

fn make_chat(id: u8, sender: &str, text: &str, kind: ChatMessageKind) -> ChatMessage {
    let mut bytes = [0u8; 16];
    bytes[15] = id;
    ChatMessage {
        id: bytes,
        sender: sender.into(),
        text: text.into(),
        timestamp: SystemTime::UNIX_EPOCH,
        is_local: false,
        kind,
        attachment: None,
    }
}

fn connected_state() -> State {
    let mut audio = AudioState::default();
    audio.voice_mode = VoiceMode::Continuous;
    // Bob is talking; Charlie is self-muted.
    audio.talking_users.insert(2);

    let mut state = State {
        connection: ConnectionState::Connected {
            server_name: "rumble.example".into(),
            user_id: 1,
        },
        rooms: vec![make_room(ROOM_LOBBY, "Lobby"), make_room(ROOM_WORK, "Work")],
        users: vec![
            make_user(1, "alice", ROOM_LOBBY, |_| {}),
            make_user(2, "bob", ROOM_WORK, |_| {}),
            make_user(3, "charlie", ROOM_LOBBY, |u| u.is_muted = true),
            make_user(4, "diana", ROOM_WORK, |u| u.server_muted = true),
        ],
        my_user_id: Some(1),
        my_room_id: Some(Uuid::from_u128(ROOM_LOBBY)),
        audio,
        chat_messages: vec![
            make_chat(1, "alice", "morning everyone", ChatMessageKind::Room),
            make_chat(2, "bob", "did the deploy go through?", ChatMessageKind::Room),
            make_chat(
                3,
                "charlie",
                "(announcement) maintenance window 14:00 UTC",
                ChatMessageKind::Tree,
            ),
        ],
        ..State::default()
    };
    state.rebuild_room_tree();
    state
}

fn device_state() -> AudioState {
    AudioState {
        input_devices: vec![
            AudioDeviceInfo {
                id: "alsa:default".to_string(),
                name: "Default (PulseAudio)".to_string(),
                pipeline: None,
                is_default: true,
            },
            AudioDeviceInfo {
                id: "alsa:usb-mic".to_string(),
                name: "Blue Yeti USB Microphone".to_string(),
                pipeline: None,
                is_default: false,
            },
            AudioDeviceInfo {
                id: "alsa:webcam".to_string(),
                name: "Logitech HD Pro Webcam C920".to_string(),
                pipeline: None,
                is_default: false,
            },
        ],
        output_devices: vec![
            AudioDeviceInfo {
                id: "alsa:hdmi".to_string(),
                name: "HDMI 1 (Built-in Audio)".to_string(),
                pipeline: None,
                is_default: false,
            },
            AudioDeviceInfo {
                id: "alsa:headphones".to_string(),
                name: "Sennheiser HD 600 (USB DAC)".to_string(),
                pipeline: None,
                is_default: true,
            },
        ],
        selected_input: Some("alsa:usb-mic".to_string()),
        selected_output: Some("alsa:headphones".to_string()),
        ..AudioState::default()
    }
}

fn stats_state() -> AudioState {
    let mut stats = AudioStats::default();
    stats.actual_bitrate_bps = 64_000.0;
    stats.avg_frame_size_bytes = 159.4;
    stats.packets_sent = 12_804;
    stats.packets_received = 12_731;
    stats.packets_lost = 73;
    stats.packets_recovered_fec = 41;
    stats.frames_concealed = 14;
    stats.playback_buffer_packets = 3;
    AudioState {
        stats,
        ..AudioState::default()
    }
}

fn demo_agent_keys() -> Vec<KeyInfo> {
    vec![
        KeyInfo {
            fingerprint: "SHA256:7gK3qPL5dEvF8sN1xR9wT2yJ4mB6cZ0aV/X+kH=".into(),
            comment: "alice@workstation".into(),
            public_key: [0u8; 32],
        },
        KeyInfo {
            fingerprint: "SHA256:Q9mNxV3pA2dLcK7yE5sT1jR4hF6oZ8bU/W+iH==".into(),
            comment: "rumble-identity".into(),
            public_key: [1u8; 32],
        },
        KeyInfo {
            fingerprint: "SHA256:M2tXrY5pL7vC1qA8nB3kE9oH4jD6sZ0wU/V+iK==".into(),
            comment: "yubikey".into(),
            public_key: [2u8; 32],
        },
    ]
}

fn demo_pending_cert() -> PendingCertificate {
    let no_op_signer: rumble_client::SigningCallback =
        Arc::new(|_payload: &[u8]| Err("fixture identity is not signing".to_string()));
    PendingCertificate {
        certificate_der: vec![0u8; 32],
        fingerprint: [
            0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x12, 0x34,
            0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0, 0x12, 0x34, 0x56, 0x78, 0x9A, 0xBC, 0xDE, 0xF0,
        ],
        server_name: "rumble.example".into(),
        server_addr: "rumble.example:5000".into(),
        username: "alice".into(),
        password: None,
        public_key: [0u8; 32],
        signer: no_op_signer,
    }
}

// ---------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Match the real app's window viewport so layout matches what users see.
    let viewport = Rect::new(0.0, 0.0, 1280.0, 800.0);
    let out_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("out");

    // Identity / SettingsStore both want a config dir to write to.
    // Fixtures don't actually mutate either, but the constructors
    // create files, so use a process-local scratch dir.
    let scratch = std::env::temp_dir().join("rumble_aetna_dump_bundles");
    std::fs::create_dir_all(&scratch)?;

    let requested: Vec<Scene> = std::env::args()
        .skip(1)
        .map(|raw| parse_scene(&raw).unwrap_or_else(|| panic!("unknown scene `{raw}`")))
        .collect();
    let scenes: Vec<Scene> = if requested.is_empty() {
        Scene::ALL.to_vec()
    } else {
        requested
    };

    for scene in scenes {
        let backend = MockBackend {
            state: scene.build_state(),
        };
        let identity = Identity::load(scratch.clone())?;
        let settings = SettingsStore::load_from_path(Some(scratch.join("settings.json")));
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build()?;
        let mut app = RumbleApp::new(backend, identity, settings, runtime);
        // Bypass the first-run wizard for connection-state scenes — they
        // illustrate the main shell, not the wizard. Wizard / unlock
        // scenes drive the wizard explicitly and need the suppression
        // *not* applied.
        if !scene.keeps_first_run() {
            app.suppress_first_run_for_test();
        }
        scene.drive_setup(&mut app);

        let mut tree = app.build();
        let bundle = render_bundle(&mut tree, viewport, Some("crates/rumble-aetna/src"));

        let basename = format!("rumble_{}", scene.slug());
        let written = write_bundle(&bundle, &out_dir, &basename)?;
        for path in &written {
            println!("wrote {}", path.display());
        }

        if !bundle.lint.findings.is_empty() {
            eprintln!("\n{basename} lint findings ({}):", bundle.lint.findings.len());
            eprint!("{}", bundle.lint.text());
        }
    }

    Ok(())
}

fn parse_scene(raw: &str) -> Option<Scene> {
    Scene::ALL.iter().copied().find(|s| s.slug().eq_ignore_ascii_case(raw))
}
