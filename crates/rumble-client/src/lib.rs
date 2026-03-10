//! Platform-agnostic Rumble client library.
//!
//! This crate contains the client logic that works across all platforms.
//! Platform-specific implementations (audio, transport, codec, etc.) are
//! injected via the `Platform` trait.

pub mod audio;
pub mod codec;
pub mod file_transfer;
pub mod keys;
pub mod platform;
pub mod storage;
pub mod transport;

// Re-export key types
pub use audio::{AudioBackend, AudioCaptureStream, AudioPlaybackStream};
pub use codec::{VoiceCodec, VoiceDecoder, VoiceEncoder};
pub use file_transfer::{FileOffer, FileTransferPlugin, TransferId, TransferStatus};
pub use keys::{KeyInfo, KeySigning, KeySource};
pub use platform::Platform;
pub use storage::PersistentStorage;
pub use transport::{TlsConfig, Transport};
