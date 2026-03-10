//! Optional file transfer capability.

use std::path::PathBuf;

/// Unique identifier for a file transfer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TransferId(pub String);

/// Metadata about a file being offered for transfer.
#[derive(Debug, Clone)]
pub struct FileOffer {
    pub id: TransferId,
    pub name: String,
    pub size: u64,
    pub mime: String,
    /// Opaque data to share with recipients (e.g., magnet link, peer info).
    pub share_data: String,
}

/// Status of a file transfer in progress.
#[derive(Debug, Clone)]
pub struct TransferStatus {
    pub id: TransferId,
    pub name: String,
    pub size: u64,
    /// Progress as a fraction in [0.0, 1.0].
    pub progress: f32,
    /// Download speed in bytes per second.
    pub download_speed: u64,
    /// Upload speed in bytes per second.
    pub upload_speed: u64,
    pub is_complete: bool,
    pub error: Option<String>,
}

/// Optional file transfer capability, injected into BackendHandle.
///
/// Not part of `Platform` — different deployments can use different
/// strategies (BitTorrent, direct transfer, etc.) or disable file
/// transfer entirely.
pub trait FileTransferPlugin: Send + Sync + 'static {
    /// Share a local file and return metadata for recipients.
    fn share(&self, path: PathBuf) -> anyhow::Result<FileOffer>;

    /// Begin downloading a file from an offer.
    fn download(&self, offer: &FileOffer) -> anyhow::Result<TransferId>;

    /// List all active transfers and their status.
    fn transfers(&self) -> Vec<TransferStatus>;

    /// Cancel an active transfer.
    fn cancel(&self, id: &TransferId) -> anyhow::Result<()>;
}
