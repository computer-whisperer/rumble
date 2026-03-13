//! Audio backend abstraction for capture and playback.

pub use api::AudioDeviceInfo;

/// A live audio capture stream that can be paused/resumed.
pub trait AudioCaptureStream: Send {
    /// Enable or disable capture. When inactive, the stream should produce
    /// silence (or simply not invoke the callback).
    fn set_active(&self, active: bool);
}

/// A live audio playback stream.
pub trait AudioPlaybackStream: Send {}

/// Platform audio I/O: device enumeration, capture, and playback.
///
/// Implementations use the platform's native audio API (e.g. cpal on
/// desktop, Web Audio on browser).
pub trait AudioBackend: Send + Default + 'static {
    type CaptureStream: AudioCaptureStream;
    type PlaybackStream: AudioPlaybackStream;

    /// List available audio input (microphone) devices.
    fn list_input_devices(&self) -> Vec<AudioDeviceInfo>;

    /// List available audio output (speaker) devices.
    fn list_output_devices(&self) -> Vec<AudioDeviceInfo>;

    /// Open an input device for capture.
    ///
    /// `device_id` selects a specific device; `None` uses the default.
    /// `on_frame` is called with 960-sample f32 PCM frames at 48 kHz mono.
    fn open_input(
        &self,
        device_id: Option<&str>,
        on_frame: Box<dyn FnMut(&[f32]) + Send>,
    ) -> anyhow::Result<Self::CaptureStream>;

    /// Open an output device for playback.
    ///
    /// `device_id` selects a specific device; `None` uses the default.
    /// `fill_buffer` is called to fill the output buffer with f32 PCM samples.
    fn open_output(
        &self,
        device_id: Option<&str>,
        fill_buffer: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> anyhow::Result<Self::PlaybackStream>;
}
