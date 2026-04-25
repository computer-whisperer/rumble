//! UI state for the first-run identity wizard.
//!
//! Lives here (not in `rumble-desktop-shell`) because the wizard's
//! render code is paradigm-specific — egui draws the SelectMethod /
//! GenerateLocal / SelectAgentKey screens itself. rumble-next will get
//! its own wizard once we design the shell-paradigm flow.

use rumble_desktop_shell::KeyInfo;

/// State of the first-run setup flow.
#[derive(Debug, Clone, Default)]
pub enum FirstRunState {
    /// Not in first-run mode (key is configured).
    #[default]
    NotNeeded,
    /// Showing the main selection screen.
    SelectMethod,
    /// Generating a new local key (with optional password).
    GenerateLocal {
        password: String,
        password_confirm: String,
        error: Option<String>,
    },
    /// Connecting to SSH agent.
    ConnectingAgent,
    /// Selecting a key from SSH agent.
    SelectAgentKey {
        keys: Vec<KeyInfo>,
        selected: Option<usize>,
        error: Option<String>,
    },
    /// Generating a new key to add to agent.
    GenerateAgentKey { comment: String },
    /// Error state.
    Error { message: String },
    /// Setup complete.
    Complete,
}
