//! Key management using ed25519-dalek and SSH agent.
//!
//! Implements `KeySigning` for native desktop via:
//! - Local key files stored as JSON in the config directory
//! - SSH agent integration for hardware-backed or agent-managed keys

use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use ed25519_dalek::SigningKey;
use rumble_client::keys::{KeyInfo, KeySigning, KeySource};
use serde::{Deserialize, Serialize};

/// On-disk format for a single stored key.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredKey {
    /// Human-readable label.
    label: String,
    /// Hex-encoded 32-byte Ed25519 public key.
    public_key_hex: String,
    /// Hex-encoded 32-byte Ed25519 private key (plaintext for now).
    private_key_hex: String,
}

/// Native key signing using local key files and SSH agent.
pub struct NativeKeySigning {
    /// Config directory where `keys.json` is stored.
    config_dir: PathBuf,
}

impl NativeKeySigning {
    /// Create a new `NativeKeySigning` with the given config directory.
    pub fn new(config_dir: PathBuf) -> Self {
        Self { config_dir }
    }

    /// Path to the keys JSON file.
    fn keys_path(&self) -> PathBuf {
        self.config_dir.join("keys.json")
    }

    /// Load all stored keys from disk.
    fn load_stored_keys(&self) -> Vec<StoredKey> {
        let path = self.keys_path();
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        };
        match serde_json::from_str(&data) {
            Ok(keys) => keys,
            Err(e) => {
                tracing::warn!("Failed to parse keys.json: {}", e);
                Vec::new()
            }
        }
    }

    /// Save stored keys to disk.
    fn save_stored_keys(&self, keys: &[StoredKey]) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.config_dir)?;
        let contents = serde_json::to_string_pretty(keys)?;
        std::fs::write(self.keys_path(), contents)?;
        Ok(())
    }

    /// Find a stored key by public key bytes.
    fn find_stored_key(&self, public_key: &[u8; 32]) -> Option<StoredKey> {
        let target_hex = hex::encode(public_key);
        self.load_stored_keys()
            .into_iter()
            .find(|k| k.public_key_hex == target_hex)
    }

    /// List Ed25519 keys from the SSH agent, returning empty vec on failure.
    async fn list_agent_keys(&self) -> Vec<KeyInfo> {
        match agent_list_ed25519_keys().await {
            Ok(keys) => keys,
            Err(e) => {
                tracing::debug!("SSH agent not available: {}", e);
                Vec::new()
            }
        }
    }
}

impl Default for NativeKeySigning {
    fn default() -> Self {
        let config_dir = directories::ProjectDirs::from("", "", "rumble")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        Self::new(config_dir)
    }
}

#[async_trait]
impl KeySigning for NativeKeySigning {
    async fn list_keys(&self) -> anyhow::Result<Vec<KeyInfo>> {
        let mut result = Vec::new();

        // Local keys
        for stored in self.load_stored_keys() {
            if let Some(pk) = parse_public_key_hex(&stored.public_key_hex) {
                result.push(KeyInfo {
                    public_key: pk,
                    label: stored.label,
                    source: KeySource::Local,
                });
            }
        }

        // SSH agent keys
        for agent_key in self.list_agent_keys().await {
            result.push(agent_key);
        }

        Ok(result)
    }

    async fn get_signer(&self, public_key: &[u8; 32]) -> anyhow::Result<api::SigningCallback> {
        // Try local key first
        if let Some(stored) = self.find_stored_key(public_key) {
            let key_bytes = hex::decode(&stored.private_key_hex)?;
            if key_bytes.len() != 32 {
                return Err(anyhow::anyhow!("Invalid private key length"));
            }
            let mut arr = [0u8; 32];
            arr.copy_from_slice(&key_bytes);

            return Ok(Arc::new(move |payload: &[u8]| {
                use ed25519_dalek::Signer;
                let sk = SigningKey::from_bytes(&arr);
                let sig = sk.sign(payload);
                Ok(sig.to_bytes())
            }));
        }

        // Try SSH agent
        let target = *public_key;
        Ok(create_agent_signer_by_pubkey(target))
    }

    async fn generate_key(&self, label: &str) -> anyhow::Result<KeyInfo> {
        let signing_key = SigningKey::from_bytes(&rand::random());
        let public_key = signing_key.verifying_key().to_bytes();

        let stored = StoredKey {
            label: label.to_string(),
            public_key_hex: hex::encode(public_key),
            private_key_hex: hex::encode(signing_key.to_bytes()),
        };

        let mut keys = self.load_stored_keys();
        keys.push(stored);
        self.save_stored_keys(&keys)?;

        Ok(KeyInfo {
            public_key,
            label: label.to_string(),
            source: KeySource::Local,
        })
    }

    async fn import_key(&self, private_key: &[u8; 32], label: &str) -> anyhow::Result<KeyInfo> {
        let signing_key = SigningKey::from_bytes(private_key);
        let public_key = signing_key.verifying_key().to_bytes();

        let stored = StoredKey {
            label: label.to_string(),
            public_key_hex: hex::encode(public_key),
            private_key_hex: hex::encode(private_key),
        };

        let mut keys = self.load_stored_keys();
        keys.push(stored);
        self.save_stored_keys(&keys)?;

        Ok(KeyInfo {
            public_key,
            label: label.to_string(),
            source: KeySource::Local,
        })
    }
}

// =============================================================================
// Helpers
// =============================================================================

/// Parse a hex-encoded public key into a 32-byte array.
fn parse_public_key_hex(hex_str: &str) -> Option<[u8; 32]> {
    let bytes = hex::decode(hex_str).ok()?;
    if bytes.len() != 32 {
        return None;
    }
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    Some(arr)
}

// =============================================================================
// SSH Agent
// =============================================================================

/// Get the SSH agent socket/pipe path for the current platform.
fn get_agent_path() -> anyhow::Result<String> {
    if let Ok(path) = std::env::var("SSH_AUTH_SOCK") {
        return Ok(path);
    }

    #[cfg(windows)]
    {
        let default_pipe = r"\\.\pipe\openssh-ssh-agent";
        return Ok(default_pipe.to_string());
    }

    #[cfg(not(windows))]
    {
        Err(anyhow::anyhow!("SSH_AUTH_SOCK not set - is ssh-agent running?"))
    }
}

/// Parse an agent path into a service-binding Binding.
fn parse_agent_binding(path: &str) -> anyhow::Result<service_binding::Binding> {
    if path.starts_with(r"\\") {
        return Ok(service_binding::Binding::NamedPipe(path.into()));
    }

    if path.starts_with("npipe://") {
        return path
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse named pipe path: {:?}", e));
    }

    #[cfg(unix)]
    {
        Ok(service_binding::Binding::FilePath(path.into()))
    }

    #[cfg(not(unix))]
    {
        Err(anyhow::anyhow!("Unix socket paths not supported on Windows: {}", path))
    }
}

/// Connect to the SSH agent, returning a client session.
async fn connect_agent() -> anyhow::Result<Box<dyn ssh_agent_lib::agent::Session + Send + Sync>> {
    let agent_path = get_agent_path()?;
    tracing::debug!("Connecting to SSH agent at: {}", agent_path);

    let binding = parse_agent_binding(&agent_path)?;
    let stream: service_binding::Stream = binding
        .try_into()
        .map_err(|e: std::io::Error| anyhow::anyhow!("Failed to connect to SSH agent: {}", e))?;

    let client = ssh_agent_lib::client::connect(stream)
        .map_err(|e| anyhow::anyhow!("Failed to create SSH agent client: {}", e))?;

    Ok(client)
}

/// Compute SHA256 fingerprint of a public key (matching egui-test format).
fn compute_fingerprint(public_key: &[u8; 32]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(public_key);
    let hash = hasher.finalize();
    let hex_parts: Vec<String> = hash.iter().take(16).map(|b| format!("{:02x}", b)).collect();
    format!("SHA256:{}", hex_parts.join(":"))
}

/// List Ed25519 keys from the SSH agent.
async fn agent_list_ed25519_keys() -> anyhow::Result<Vec<KeyInfo>> {
    use ssh_key::public::KeyData;

    let mut client = connect_agent().await?;
    let identities = client
        .request_identities()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to request identities: {}", e))?;

    let mut result = Vec::new();
    for id in identities {
        if let KeyData::Ed25519(ed_key) = &id.pubkey {
            let public_key: [u8; 32] = ed_key.0;
            let label = if id.comment.is_empty() {
                compute_fingerprint(&public_key)
            } else {
                id.comment.clone()
            };
            result.push(KeyInfo {
                public_key,
                label,
                source: KeySource::SshAgent,
            });
        }
    }

    tracing::info!("Found {} Ed25519 keys in SSH agent", result.len());
    Ok(result)
}

/// Sign data via the SSH agent using the given public key.
async fn agent_sign(public_key: &[u8; 32], data: &[u8]) -> anyhow::Result<[u8; 64]> {
    use ssh_agent_lib::proto::SignRequest;
    use ssh_key::public::KeyData;

    let mut client = connect_agent().await?;
    let identities = client
        .request_identities()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to request identities: {}", e))?;

    let target_fp = compute_fingerprint(public_key);

    let identity = identities
        .into_iter()
        .find(|id| {
            if let KeyData::Ed25519(ed_key) = &id.pubkey {
                let pk: [u8; 32] = ed_key.0;
                compute_fingerprint(&pk) == target_fp
            } else {
                false
            }
        })
        .ok_or_else(|| anyhow::anyhow!("Key not found in SSH agent"))?;

    let request = SignRequest {
        pubkey: identity.pubkey.clone(),
        data: data.to_vec(),
        flags: 0,
    };

    let signature = client
        .sign(request)
        .await
        .map_err(|e| anyhow::anyhow!("SSH agent signing failed: {}", e))?;

    let sig_bytes = signature.as_bytes();
    if sig_bytes.len() != 64 {
        return Err(anyhow::anyhow!(
            "Invalid Ed25519 signature length: expected 64, got {}",
            sig_bytes.len()
        ));
    }

    let mut raw_sig = [0u8; 64];
    raw_sig.copy_from_slice(sig_bytes);
    Ok(raw_sig)
}

/// Create a synchronous signing callback that uses the SSH agent,
/// identified by the 32-byte public key.
///
/// Spawns a dedicated thread with its own tokio runtime for each sign
/// operation, avoiding nested-runtime issues.
fn create_agent_signer_by_pubkey(public_key: [u8; 32]) -> api::SigningCallback {
    Arc::new(move |payload: &[u8]| {
        let pk = public_key;
        let payload = payload.to_vec();

        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            let result = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .map_err(|e| format!("Failed to create runtime: {}", e))
                .and_then(|rt| {
                    rt.block_on(async {
                        agent_sign(&pk, &payload)
                            .await
                            .map_err(|e| format!("Agent signing failed: {}", e))
                    })
                });
            let _ = tx.send(result);
        });

        rx.recv().map_err(|e| format!("Channel receive error: {}", e))?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_generate_and_list_keys() {
        let dir = tempfile::tempdir().unwrap();
        let ks = NativeKeySigning::new(dir.path().to_path_buf());

        // Initially empty (ignoring agent keys)
        let keys = ks.load_stored_keys();
        assert!(keys.is_empty());

        // Generate a key
        let info = ks.generate_key("test-key").await.unwrap();
        assert_eq!(info.label, "test-key");
        assert_eq!(info.source, KeySource::Local);

        // Should appear in stored keys
        let keys = ks.load_stored_keys();
        assert_eq!(keys.len(), 1);
        assert_eq!(keys[0].label, "test-key");
    }

    #[tokio::test]
    async fn test_import_key() {
        let dir = tempfile::tempdir().unwrap();
        let ks = NativeKeySigning::new(dir.path().to_path_buf());

        let sk = SigningKey::from_bytes(&rand::random());
        let pk = sk.verifying_key().to_bytes();

        let info = ks.import_key(&sk.to_bytes(), "imported").await.unwrap();
        assert_eq!(info.public_key, pk);
        assert_eq!(info.label, "imported");
        assert_eq!(info.source, KeySource::Local);
    }

    #[tokio::test]
    async fn test_get_signer_local() {
        let dir = tempfile::tempdir().unwrap();
        let ks = NativeKeySigning::new(dir.path().to_path_buf());

        let info = ks.generate_key("sign-test").await.unwrap();
        let signer = ks.get_signer(&info.public_key).await.unwrap();

        let message = b"hello world";
        let sig = signer(message).expect("signing should succeed");

        // Verify the signature
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};
        let vk = VerifyingKey::from_bytes(&info.public_key).unwrap();
        let signature = Signature::from_bytes(&sig);
        vk.verify(message, &signature).unwrap();
    }

    #[tokio::test]
    async fn test_list_keys_includes_local() {
        let dir = tempfile::tempdir().unwrap();
        let ks = NativeKeySigning::new(dir.path().to_path_buf());

        ks.generate_key("key-a").await.unwrap();
        ks.generate_key("key-b").await.unwrap();

        let all = ks.list_keys().await.unwrap();
        let local: Vec<_> = all.iter().filter(|k| k.source == KeySource::Local).collect();
        assert_eq!(local.len(), 2);
    }
}
