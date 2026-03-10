//! Platform bundle trait that ties together all platform-specific abstractions.

use crate::{
    audio::AudioBackend, codec::VoiceCodec, keys::KeySigning, storage::PersistentStorage, transport::Transport,
};

/// Bundle trait grouping all platform-specific associated types.
///
/// A concrete `Platform` implementation selects the transport, audio backend,
/// codec, persistent storage, and key-signing strategy for a given target
/// (e.g. native desktop vs. web).
pub trait Platform: Send + Sync + 'static {
    type Transport: Transport;
    type AudioBackend: AudioBackend;
    type Codec: VoiceCodec;
    type Storage: PersistentStorage;
    type KeyManager: KeySigning;
}
