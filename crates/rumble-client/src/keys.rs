//! Key management and signing abstraction.

use async_trait::async_trait;

pub use api::SigningCallback;

/// Information about a signing key.
#[derive(Debug, Clone)]
pub struct KeyInfo {
    /// The 32-byte Ed25519 public key.
    pub public_key: [u8; 32],
    /// Human-readable label for the key.
    pub label: String,
    /// Where the key is stored/managed.
    pub source: KeySource,
}

/// Where a key is stored/managed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeySource {
    /// Local key file on disk.
    Local,
    /// SSH agent.
    SshAgent,
    /// Generated in-memory (ephemeral, not persisted).
    Ephemeral,
}

/// Platform key management: listing, signing, generating, and importing keys.
#[async_trait]
pub trait KeySigning: Send + Sync + 'static {
    /// List all available signing keys.
    async fn list_keys(&self) -> anyhow::Result<Vec<KeyInfo>>;

    /// Get a synchronous signing callback for the given public key.
    async fn get_signer(&self, public_key: &[u8; 32]) -> anyhow::Result<SigningCallback>;

    /// Generate a new Ed25519 key pair and persist it with the given label.
    async fn generate_key(&self, label: &str) -> anyhow::Result<KeyInfo>;

    /// Import an existing private key and persist it with the given label.
    async fn import_key(&self, private_key: &[u8; 32], label: &str) -> anyhow::Result<KeyInfo>;
}
