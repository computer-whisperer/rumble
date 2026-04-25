//! Ed25519 identity wrapper. Backed by `rumble_desktop_shell::KeyManager`
//! so plaintext / encrypted / SSH-agent identities written by either
//! rumble-egui or a future rumble-next first-run wizard load
//! identically.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use rumble_client::SigningCallback;
use rumble_desktop_shell::KeyManager;

pub struct Identity {
    manager: KeyManager,
    public_key: [u8; 32],
}

impl Identity {
    /// Load the identity from `<config_dir>/identity.json`. If none
    /// exists, generate a fresh plaintext key and persist it.
    ///
    /// Encrypted-at-rest keys load with `cached_signing_key = None`
    /// until the user provides a password — for now rumble-next has
    /// no unlock UI, so an encrypted file effectively prevents login.
    /// Same goes for SSH-agent identities when the agent isn't
    /// reachable. Both paths log clearly so the failure mode is
    /// obvious in the terminal.
    pub fn load_or_create(config_dir: &PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(config_dir)?;
        let mut manager = KeyManager::new(config_dir.clone());

        if manager.needs_setup() {
            let info = manager.generate_local_key(None).map_err(std::io::Error::other)?;
            tracing::info!("Generated fresh identity ({})", info.fingerprint);
        }

        let public_key = manager
            .public_key_bytes()
            .ok_or_else(|| std::io::Error::other("identity loaded but public key is invalid"))?;

        Ok(Self { manager, public_key })
    }

    pub fn public_key(&self) -> [u8; 32] {
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
        rumble_desktop_shell::compute_fingerprint(&self.public_key)
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
