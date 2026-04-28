//! Ed25519 identity wrapper. Backed by `rumble_desktop_shell::KeyManager`
//! so plaintext / encrypted / SSH-agent identities written by either
//! client load identically.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use rumble_client::SigningCallback;
use rumble_desktop_shell::{KeyInfo, KeyManager};

pub struct Identity {
    manager: KeyManager,
    public_key: Option<[u8; 32]>,
}

impl Identity {
    /// Load the identity manager from `<config_dir>/identity.json`
    /// without creating a replacement key. Missing config is a valid
    /// first-run state and should be resolved by the UI wizard.
    pub fn load(config_dir: &PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(config_dir)?;
        let manager = KeyManager::new(config_dir.clone());
        let public_key = manager.public_key_bytes();

        Ok(Self { manager, public_key })
    }

    pub fn public_key(&self) -> Option<[u8; 32]> {
        self.public_key
    }

    /// Build a signing callback for the backend. Returns a no-op
    /// callback that always errors when the underlying key is locked
    /// or its source-type's feature is disabled — `connect_to_server`
    /// will surface the resulting handshake failure as a connection
    /// error.
    pub fn signer(&self) -> SigningCallback {
        match self.manager.create_signer() {
            Some(signer) => signer,
            None => {
                tracing::error!("identity: no usable signer (locked encrypted key or unsupported source)");
                std::sync::Arc::new(|_payload: &[u8]| Err("identity locked or unsupported".to_string()))
            }
        }
    }

    pub fn needs_setup(&self) -> bool {
        self.manager.needs_setup()
    }

    pub fn generate_local_key(&mut self, password: Option<&str>) -> anyhow::Result<KeyInfo> {
        let info = self.manager.generate_local_key(password)?;
        self.public_key = Some(info.public_key);
        Ok(info)
    }

    pub fn select_agent_key(&mut self, key_info: &KeyInfo) -> anyhow::Result<()> {
        self.manager.select_agent_key(key_info)?;
        self.public_key = Some(key_info.public_key);
        Ok(())
    }

    pub fn unlock(&mut self, password: &str) -> anyhow::Result<()> {
        self.manager.unlock_local_key(password)?;
        self.public_key = self.manager.public_key_bytes();
        Ok(())
    }

    /// Underlying manager — exposed so a future first-run wizard can
    /// drive `generate_local_key`, `select_agent_key`, etc.
    pub fn manager(&self) -> &KeyManager {
        &self.manager
    }

    pub fn manager_mut(&mut self) -> &mut KeyManager {
        &mut self.manager
    }

    /// True if the on-disk identity is encrypted and we haven't been
    /// given a password yet. UI can use this to gate the connect form.
    pub fn needs_unlock(&self) -> bool {
        self.manager.needs_unlock()
    }

    /// Hex of the SHA256 fingerprint, formatted for display
    /// (e.g. in the connect view's public-key footer).
    pub fn fingerprint(&self) -> String {
        self.public_key
            .map(|key| rumble_desktop_shell::compute_fingerprint(&key))
            .unwrap_or_else(|| "(not set up)".to_string())
    }

    /// Cached `SigningKey` for plaintext / unlocked-encrypted sources.
    /// Used by tests; production paths go through `signer()`.
    pub fn signing_key(&self) -> Option<&SigningKey> {
        self.manager.signing_key()
    }
}

/// Default config directory — matches rumble-egui's
/// `ProjectDirs::from("com", "rumble", "Rumble")`, so the identity file
/// is shared between the two clients. Honors `RUMBLE_NEXT_CONFIG_DIR`
/// for headless tests / screenshots that need a clean sandbox.
pub fn default_config_dir() -> PathBuf {
    if let Ok(override_dir) = std::env::var("RUMBLE_NEXT_CONFIG_DIR") {
        return PathBuf::from(override_dir);
    }
    if let Some(dirs) = directories::ProjectDirs::from("com", "rumble", "Rumble") {
        dirs.config_dir().to_path_buf()
    } else {
        PathBuf::from("./config")
    }
}
