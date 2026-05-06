//! Ed25519 identity wrapper, backed by `rumble_desktop_shell::KeyManager`.
//!
//! Mirrors `rumble-next::identity::Identity` so the on-disk
//! `identity.json` written by either client loads identically here.

use std::path::PathBuf;

use ed25519_dalek::SigningKey;
use rumble_client::SigningCallback;
use rumble_desktop_shell::{KeyInfo, KeyManager, compute_fingerprint};

pub struct Identity {
    manager: KeyManager,
    public_key: Option<[u8; 32]>,
}

impl Identity {
    pub fn load(config_dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(&config_dir)?;
        let manager = KeyManager::new(config_dir);
        let public_key = manager.public_key_bytes();
        Ok(Self { manager, public_key })
    }

    pub fn public_key(&self) -> Option<[u8; 32]> {
        self.public_key
    }

    pub fn signer(&self) -> SigningCallback {
        match self.manager.create_signer() {
            Some(s) => s,
            None => {
                tracing::error!("identity: no usable signer (locked encrypted key or unsupported source)");
                std::sync::Arc::new(|_payload: &[u8]| Err("identity locked or unsupported".to_string()))
            }
        }
    }

    pub fn needs_setup(&self) -> bool {
        self.manager.needs_setup()
    }

    pub fn needs_unlock(&self) -> bool {
        self.manager.needs_unlock()
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

    pub fn fingerprint(&self) -> String {
        self.public_key
            .map(|key| compute_fingerprint(&key))
            .unwrap_or_else(|| "(not set up)".to_string())
    }

    pub fn signing_key(&self) -> Option<&SigningKey> {
        self.manager.signing_key()
    }

    pub fn manager(&self) -> &KeyManager {
        &self.manager
    }

    pub fn manager_mut(&mut self) -> &mut KeyManager {
        &mut self.manager
    }
}
