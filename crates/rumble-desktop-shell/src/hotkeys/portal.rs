//! XDG Desktop Portal GlobalShortcuts backend for Wayland.
//!
//! Provides global hotkey support on Wayland via the
//! `org.freedesktop.portal.GlobalShortcuts` D-Bus interface.
//!
//! Supported environments: KDE Plasma 5.27+, GNOME 47+, Hyprland.
//! Older Wayland compositors fail gracefully via `PortalHotkeyBackend::new
//! → None` and the manager falls back to window-focused shortcuts.

use std::{collections::HashMap, sync::Arc};

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use futures_util::StreamExt;
use tokio::sync::{RwLock, mpsc};

use super::HotkeyEvent;

/// Stable shortcut IDs the portal session knows us by. Don't rename
/// without a migration — these are the keys the user's compositor
/// stores their custom bindings under.
pub const SHORTCUT_PTT: &str = "push-to-talk";
pub const SHORTCUT_MUTE: &str = "toggle-mute";
pub const SHORTCUT_DEAFEN: &str = "toggle-deafen";

#[derive(Debug, Clone)]
pub struct ShortcutInfo {
    pub id: String,
    pub description: String,
    /// Empty until the user binds something in the system settings.
    pub trigger_description: String,
}

#[derive(Default)]
pub struct PortalShortcutState {
    pub shortcuts: Vec<ShortcutInfo>,
}

pub struct PortalHotkeyBackend {
    event_rx: mpsc::UnboundedReceiver<HotkeyEvent>,
    /// Kept around so the listener task's sender stays alive even if
    /// callers later add a shutdown signal.
    _event_tx: mpsc::UnboundedSender<HotkeyEvent>,
    shortcuts_bound: bool,
    state: Arc<RwLock<PortalShortcutState>>,
    runtime_handle: tokio::runtime::Handle,
}

impl PortalHotkeyBackend {
    /// Connect to the portal, create a session, bind our shortcuts, and
    /// spawn the signal listener. Returns `None` if any step fails.
    pub async fn new(runtime_handle: tokio::runtime::Handle) -> Option<Self> {
        tracing::info!("Attempting to connect to XDG GlobalShortcuts portal");

        let shortcuts = match GlobalShortcuts::new().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("GlobalShortcuts portal not available: {e}");
                return None;
            }
        };

        let session = match shortcuts.create_session().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to create GlobalShortcuts session: {e}");
                return None;
            }
        };

        let shortcut_definitions = shortcut_definitions();
        let state = Arc::new(RwLock::new(PortalShortcutState::default()));

        let shortcuts_bound = match shortcuts.bind_shortcuts(&session, &shortcut_definitions, None).await {
            Ok(request) => match request.response() {
                Ok(response) => {
                    let bound = response.shortcuts();
                    tracing::info!("Shortcuts bound: {} entries", bound.len());

                    let mut state_guard = state.write().await;
                    state_guard.shortcuts = bound
                        .iter()
                        .map(|s| ShortcutInfo {
                            id: s.id().to_string(),
                            description: shortcut_description(s.id()),
                            trigger_description: s.trigger_description().to_string(),
                        })
                        .collect();
                    drop(state_guard);

                    !bound.is_empty()
                }
                Err(e) => {
                    tracing::warn!("Failed to get bind_shortcuts response: {e}");
                    false
                }
            },
            Err(e) => {
                tracing::warn!("Failed to bind shortcuts: {e}");
                false
            }
        };

        let (event_tx, event_rx) = mpsc::unbounded_channel();

        let event_tx_clone = event_tx.clone();
        runtime_handle.spawn(async move {
            Self::listen_for_signals(shortcuts, event_tx_clone).await;
        });

        Some(Self {
            event_rx,
            _event_tx: event_tx,
            shortcuts_bound,
            state,
            runtime_handle,
        })
    }

    async fn listen_for_signals(shortcuts: GlobalShortcuts<'static>, event_tx: mpsc::UnboundedSender<HotkeyEvent>) {
        tracing::debug!("Starting GlobalShortcuts signal listener");

        // Keys are static so the map can be initialised once. Values
        // map activation/deactivation onto the corresponding event.
        let shortcut_map: HashMap<&str, fn(bool) -> Option<HotkeyEvent>> = [
            (
                SHORTCUT_PTT,
                (|pressed| {
                    Some(if pressed {
                        HotkeyEvent::PttPressed
                    } else {
                        HotkeyEvent::PttReleased
                    })
                }) as fn(bool) -> Option<HotkeyEvent>,
            ),
            (
                SHORTCUT_MUTE,
                (|pressed| if pressed { Some(HotkeyEvent::ToggleMute) } else { None })
                    as fn(bool) -> Option<HotkeyEvent>,
            ),
            (
                SHORTCUT_DEAFEN,
                (|pressed| {
                    if pressed { Some(HotkeyEvent::ToggleDeafen) } else { None }
                }) as fn(bool) -> Option<HotkeyEvent>,
            ),
        ]
        .into_iter()
        .collect();

        let mut activated_stream = match shortcuts.receive_activated().await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!("Failed to subscribe to Activated signals: {e}");
                return;
            }
        };

        let mut deactivated_stream = match shortcuts.receive_deactivated().await {
            Ok(stream) => stream,
            Err(e) => {
                tracing::error!("Failed to subscribe to Deactivated signals: {e}");
                return;
            }
        };

        loop {
            tokio::select! {
                Some(activated) = activated_stream.next() => {
                    let id = activated.shortcut_id();
                    if let Some(handler) = shortcut_map.get(id)
                        && let Some(event) = handler(true)
                        && event_tx.send(event).is_err() {
                            tracing::debug!("Event channel closed, stopping listener");
                            break;
                        }
                }
                Some(deactivated) = deactivated_stream.next() => {
                    let id = deactivated.shortcut_id();
                    if let Some(handler) = shortcut_map.get(id)
                        && let Some(event) = handler(false)
                        && event_tx.send(event).is_err() {
                            tracing::debug!("Event channel closed, stopping listener");
                            break;
                        }
                }
                else => {
                    tracing::debug!("Signal streams ended");
                    break;
                }
            }
        }
    }

    pub fn is_available(&self) -> bool {
        self.shortcuts_bound
    }

    pub fn poll_events(&mut self) -> Vec<HotkeyEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Snapshot of currently bound shortcuts. `try_read` so the UI
    /// thread never blocks on the listener task.
    pub fn get_shortcuts(&self) -> Vec<ShortcutInfo> {
        self.state
            .try_read()
            .map(|guard| guard.shortcuts.clone())
            .unwrap_or_default()
    }

    /// Open the system shortcut configuration dialog. On most desktops
    /// this is the only way for the user to assign keys, since the
    /// portal does not expose the bindings to us at registration time.
    pub fn open_settings(&self) {
        let state = self.state.clone();
        self.runtime_handle.spawn(async move {
            if let Err(e) = open_shortcut_settings_and_update(state).await {
                tracing::error!("Failed to open shortcut settings: {e}");
            }
        });
    }
}

fn shortcut_definitions() -> Vec<NewShortcut> {
    vec![
        NewShortcut::new(SHORTCUT_PTT, "Hold to transmit voice (Push-to-Talk)"),
        NewShortcut::new(SHORTCUT_MUTE, "Toggle microphone mute"),
        NewShortcut::new(SHORTCUT_DEAFEN, "Toggle speaker mute (deafen)"),
    ]
}

fn shortcut_description(id: &str) -> String {
    match id {
        SHORTCUT_PTT => "Push-to-Talk".to_string(),
        SHORTCUT_MUTE => "Toggle Mute".to_string(),
        SHORTCUT_DEAFEN => "Toggle Deafen".to_string(),
        _ => id.to_string(),
    }
}

async fn open_shortcut_settings_and_update(state: Arc<RwLock<PortalShortcutState>>) -> Result<(), ashpd::Error> {
    let shortcuts = GlobalShortcuts::new().await?;
    let session = shortcuts.create_session().await?;
    let definitions = shortcut_definitions();

    let request = shortcuts.bind_shortcuts(&session, &definitions, None).await?;
    match request.response() {
        Ok(response) => {
            let bound = response.shortcuts();
            tracing::info!("Shortcuts reconfigured: {} entries", bound.len());

            let mut state_guard = state.write().await;
            state_guard.shortcuts = bound
                .iter()
                .map(|s| ShortcutInfo {
                    id: s.id().to_string(),
                    description: shortcut_description(s.id()),
                    trigger_description: s.trigger_description().to_string(),
                })
                .collect();
        }
        Err(e) => {
            tracing::warn!("Failed to get bind_shortcuts response: {e}");
        }
    }

    Ok(())
}

/// Stand-alone helper, kept for callers that don't have a backend
/// instance handy. Prefer `PortalHotkeyBackend::open_settings`.
pub async fn open_shortcut_settings() -> Result<(), ashpd::Error> {
    let shortcuts = GlobalShortcuts::new().await?;
    let session = shortcuts.create_session().await?;
    let definitions = shortcut_definitions();
    shortcuts.bind_shortcuts(&session, &definitions, None).await?;
    Ok(())
}
