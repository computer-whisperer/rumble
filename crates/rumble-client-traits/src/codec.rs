//! Voice codec abstraction for encoding and decoding audio.

pub use rumble_protocol::EncoderSettings;

/// Encodes PCM audio into compressed frames.
pub trait VoiceEncoder: Send {
    /// Encode a frame of PCM samples into `output`. Returns the number of
    /// bytes written.
    fn encode(&mut self, pcm: &[f32], output: &mut [u8]) -> anyhow::Result<usize>;

    /// Apply new encoder settings (bitrate, complexity, etc.).
    fn apply_settings(&mut self, settings: &EncoderSettings) -> anyhow::Result<()>;
}

/// Decodes compressed frames back to PCM audio.
pub trait VoiceDecoder: Send {
    /// Decode a compressed frame into PCM samples. Returns the number of
    /// samples written.
    fn decode(&mut self, data: &[u8], output: &mut [f32]) -> anyhow::Result<usize>;

    /// Generate a packet-loss concealment frame (no data received).
    fn decode_plc(&mut self, output: &mut [f32]) -> anyhow::Result<usize>;

    /// Decode using Forward Error Correction data from a later packet.
    fn decode_fec(&mut self, data: &[u8], output: &mut [f32]) -> anyhow::Result<usize>;
}

/// Factory for creating voice encoder and decoder instances.
///
/// Implementations wrap a specific codec (e.g. Opus).
pub trait VoiceCodec: Send + 'static {
    type Encoder: VoiceEncoder;
    type Decoder: VoiceDecoder;

    /// Create a new encoder with the given settings.
    fn create_encoder(settings: &EncoderSettings) -> anyhow::Result<Self::Encoder>;

    /// Create a new decoder with default settings.
    fn create_decoder() -> anyhow::Result<Self::Decoder>;
}
