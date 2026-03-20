//! Audio I/O implementation using cpal.
//!
//! Implements the [`AudioBackend`] trait from `rumble-client-traits` using cpal for
//! native desktop audio capture and playback.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use cpal::{
    Device, Host, SampleFormat, StreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use tracing::{debug, error, info};

use rumble_client_traits::audio::{AudioBackend, AudioCaptureStream, AudioPlaybackStream};
use rumble_protocol::AudioDeviceInfo;

/// Audio sample rate used for voice communication (48 kHz, native for Opus).
const SAMPLE_RATE: u32 = 48000;

/// Number of channels (mono).
const CHANNELS: u16 = 1;

/// Opus frame size in samples: 20 ms at 48 kHz.
const FRAME_SIZE: usize = 960;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Get a device's display name via the cpal 0.17 `description()` API.
fn get_device_name(device: &Device) -> Option<String> {
    device.description().map(|desc| desc.name().to_string()).ok()
}

/// Returns a preference score for a sample format (higher is better).
fn sample_format_preference(format: SampleFormat) -> u8 {
    match format {
        SampleFormat::F32 => 5,
        SampleFormat::I16 => 4,
        SampleFormat::I32 => 3,
        SampleFormat::U16 => 2,
        SampleFormat::U8 => 1,
        _ => 0,
    }
}

/// Find a device by name from an iterator, or return the provided default.
fn find_device_or_default(
    devices: impl Iterator<Item = Device>,
    device_id: Option<&str>,
    default: Option<Device>,
) -> anyhow::Result<Device> {
    match device_id {
        Some(id) => devices
            .into_iter()
            .find(|d| get_device_name(d).as_deref() == Some(id))
            .ok_or_else(|| anyhow::anyhow!("audio device not found: {}", id)),
        None => default.ok_or_else(|| anyhow::anyhow!("no default audio device available")),
    }
}

/// Pick the best supported config for a device (input or output) that matches
/// our target sample rate / channel count.  Returns `(StreamConfig, SampleFormat,
/// actual_sample_rate, actual_channels)`.
///
/// If no config matches our exact rate/channels we fall back to the best
/// available format and handle conversion in the callback.
fn pick_config(
    configs: impl Iterator<Item = cpal::SupportedStreamConfigRange>,
) -> Option<(StreamConfig, SampleFormat, u32, u16)> {
    // First pass: find configs that match our exact requirements.
    let mut all: Vec<cpal::SupportedStreamConfigRange> = configs.collect();

    // Sort by preference (best format first).
    all.sort_by(|a, b| sample_format_preference(b.sample_format()).cmp(&sample_format_preference(a.sample_format())));

    // Ideal: exact channel + sample rate match.
    for cfg in &all {
        if cfg.channels() == CHANNELS && cfg.min_sample_rate() <= SAMPLE_RATE && cfg.max_sample_rate() >= SAMPLE_RATE {
            let sc = StreamConfig {
                channels: CHANNELS,
                sample_rate: SAMPLE_RATE,
                buffer_size: cpal::BufferSize::Default,
            };
            return Some((sc, cfg.sample_format(), SAMPLE_RATE, CHANNELS));
        }
    }

    // Fallback: accept any channel count at our sample rate.
    for cfg in &all {
        if cfg.min_sample_rate() <= SAMPLE_RATE && cfg.max_sample_rate() >= SAMPLE_RATE {
            let ch = cfg.channels();
            let sc = StreamConfig {
                channels: ch,
                sample_rate: SAMPLE_RATE,
                buffer_size: cpal::BufferSize::Default,
            };
            return Some((sc, cfg.sample_format(), SAMPLE_RATE, ch));
        }
    }

    // Fallback: different sample rate, prefer mono.
    for cfg in &all {
        let rate = cfg.max_sample_rate(); // pick highest supported rate
        let ch = cfg.channels();
        let sc = StreamConfig {
            channels: ch,
            sample_rate: rate,
            buffer_size: cpal::BufferSize::Default,
        };
        return Some((sc, cfg.sample_format(), rate, ch));
    }

    None
}

/// Basic linear interpolation resampler.
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if input.is_empty() || from_rate == to_rate {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        output.push(a + (b - a) * frac as f32);
    }
    output
}

// ---------------------------------------------------------------------------
// InputProcessor
// ---------------------------------------------------------------------------

/// Accumulates incoming samples into fixed-size frames.
struct InputProcessor {
    buffer: Vec<f32>,
    frame_size: usize,
    on_frame: Box<dyn FnMut(&[f32]) + Send>,
}

impl InputProcessor {
    fn new(frame_size: usize, on_frame: Box<dyn FnMut(&[f32]) + Send>) -> Self {
        Self {
            buffer: Vec::with_capacity(frame_size * 2),
            frame_size,
            on_frame,
        }
    }

    fn process(&mut self, samples: &[f32]) {
        self.buffer.extend_from_slice(samples);
        while self.buffer.len() >= self.frame_size {
            let frame: Vec<f32> = self.buffer.drain(..self.frame_size).collect();
            (self.on_frame)(&frame);
        }
    }
}

// ---------------------------------------------------------------------------
// CpalAudioBackend
// ---------------------------------------------------------------------------

/// Audio backend using the cpal library for native desktop audio.
pub struct CpalAudioBackend {
    host: Host,
}

impl CpalAudioBackend {
    pub fn new() -> Self {
        let host = cpal::default_host();
        info!(host_id = ?host.id(), "audio: initialized cpal host");
        Self { host }
    }
}

impl Default for CpalAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioBackend for CpalAudioBackend {
    type CaptureStream = CpalCaptureStream;
    type PlaybackStream = CpalPlaybackStream;

    fn list_input_devices(&self) -> Vec<AudioDeviceInfo> {
        let default_name = self.host.default_input_device().and_then(|d| get_device_name(&d));

        self.host
            .input_devices()
            .map(|devices| {
                devices
                    .filter_map(|device| {
                        let name = get_device_name(&device)?;
                        let is_default = default_name.as_ref() == Some(&name);
                        Some(AudioDeviceInfo {
                            id: name.clone(),
                            name,
                            is_default,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn list_output_devices(&self) -> Vec<AudioDeviceInfo> {
        let default_name = self.host.default_output_device().and_then(|d| get_device_name(&d));

        self.host
            .output_devices()
            .map(|devices| {
                devices
                    .filter_map(|device| {
                        let name = get_device_name(&device)?;
                        let is_default = default_name.as_ref() == Some(&name);
                        Some(AudioDeviceInfo {
                            id: name.clone(),
                            name,
                            is_default,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn open_input(
        &self,
        device_id: Option<&str>,
        on_frame: Box<dyn FnMut(&[f32]) + Send>,
    ) -> anyhow::Result<CpalCaptureStream> {
        let device = find_device_or_default(self.host.input_devices()?, device_id, self.host.default_input_device())?;

        let dev_name = get_device_name(&device).unwrap_or_else(|| "<unknown>".into());
        info!(device = %dev_name, "audio: opening input device");

        let supported = device.supported_input_configs()?;
        let (stream_config, sample_format, actual_rate, actual_channels) = pick_config(supported)
            .ok_or_else(|| anyhow::anyhow!("no suitable input config for device {}", dev_name))?;

        debug!(
            ?sample_format,
            rate = actual_rate,
            channels = actual_channels,
            "audio: input config selected"
        );

        let is_active = Arc::new(AtomicBool::new(true));
        let is_active_clone = is_active.clone();
        let processor = Arc::new(Mutex::new(InputProcessor::new(FRAME_SIZE, on_frame)));

        let err_fn = move |err| error!("audio: input stream error: {}", err);

        // We need different type parameters for build_input_stream based on
        // the sample format, so we match and build accordingly.
        let stream = match sample_format {
            SampleFormat::F32 => {
                let ch = actual_channels;
                let rate = actual_rate;
                device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !is_active_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        let samples = convert_input_f32(data, ch, rate);
                        if let Ok(mut proc) = processor.lock() {
                            proc.process(&samples);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let ch = actual_channels;
                let rate = actual_rate;
                let is_active_clone = is_active.clone();
                let processor = processor.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !is_active_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        let float_data: Vec<f32> = data.iter().map(|&s| s as f32 / i16::MAX as f32).collect();
                        let samples = convert_input_f32(&float_data, ch, rate);
                        if let Ok(mut proc) = processor.lock() {
                            proc.process(&samples);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let ch = actual_channels;
                let rate = actual_rate;
                let is_active_clone = is_active.clone();
                let processor = processor.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u16], _: &cpal::InputCallbackInfo| {
                        if !is_active_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        let float_data: Vec<f32> =
                            data.iter().map(|&s| (s as f32 / u16::MAX as f32) * 2.0 - 1.0).collect();
                        let samples = convert_input_f32(&float_data, ch, rate);
                        if let Ok(mut proc) = processor.lock() {
                            proc.process(&samples);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I32 => {
                let ch = actual_channels;
                let rate = actual_rate;
                let is_active_clone = is_active.clone();
                let processor = processor.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i32], _: &cpal::InputCallbackInfo| {
                        if !is_active_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        let float_data: Vec<f32> = data.iter().map(|&s| s as f32 / i32::MAX as f32).collect();
                        let samples = convert_input_f32(&float_data, ch, rate);
                        if let Ok(mut proc) = processor.lock() {
                            proc.process(&samples);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U8 => {
                let ch = actual_channels;
                let rate = actual_rate;
                let is_active_clone = is_active.clone();
                let processor = processor.clone();
                device.build_input_stream(
                    &stream_config,
                    move |data: &[u8], _: &cpal::InputCallbackInfo| {
                        if !is_active_clone.load(Ordering::Relaxed) {
                            return;
                        }
                        let float_data: Vec<f32> = data.iter().map(|&s| (s as f32 - 128.0) / 128.0).collect();
                        let samples = convert_input_f32(&float_data, ch, rate);
                        if let Ok(mut proc) = processor.lock() {
                            proc.process(&samples);
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            other => anyhow::bail!("unsupported input sample format: {:?}", other),
        };

        stream.play()?;
        info!("audio: input stream started");

        Ok(CpalCaptureStream {
            _stream: stream,
            is_active,
        })
    }

    fn open_output(
        &self,
        device_id: Option<&str>,
        fill_buffer: Box<dyn FnMut(&mut [f32]) + Send>,
    ) -> anyhow::Result<CpalPlaybackStream> {
        let device = find_device_or_default(
            self.host.output_devices()?,
            device_id,
            self.host.default_output_device(),
        )?;

        let dev_name = get_device_name(&device).unwrap_or_else(|| "<unknown>".into());
        info!(device = %dev_name, "audio: opening output device");

        let supported = device.supported_output_configs()?;
        let (stream_config, sample_format, actual_rate, actual_channels) = pick_config(supported)
            .ok_or_else(|| anyhow::anyhow!("no suitable output config for device {}", dev_name))?;

        debug!(
            ?sample_format,
            rate = actual_rate,
            channels = actual_channels,
            "audio: output config selected"
        );

        let fill = Arc::new(Mutex::new(fill_buffer));
        let err_fn = move |err| error!("audio: output stream error: {}", err);

        let stream = match sample_format {
            SampleFormat::F32 => {
                let fill = fill.clone();
                let ch = actual_channels;
                let rate = actual_rate;
                device.build_output_stream(
                    &stream_config,
                    move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                        write_output_f32(data, &fill, ch, rate);
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I16 => {
                let fill = fill.clone();
                let ch = actual_channels;
                let rate = actual_rate;
                device.build_output_stream(
                    &stream_config,
                    move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                        let mono_len = output_mono_len(data.len(), ch, rate);
                        let mut mono = vec![0.0f32; mono_len];
                        if let Ok(mut f) = fill.lock() {
                            f(&mut mono);
                        }
                        let expanded = expand_output(&mono, ch, rate);
                        for (out, &val) in data.iter_mut().zip(expanded.iter()) {
                            *out = (val * i16::MAX as f32) as i16;
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U16 => {
                let fill = fill.clone();
                let ch = actual_channels;
                let rate = actual_rate;
                device.build_output_stream(
                    &stream_config,
                    move |data: &mut [u16], _: &cpal::OutputCallbackInfo| {
                        let mono_len = output_mono_len(data.len(), ch, rate);
                        let mut mono = vec![0.0f32; mono_len];
                        if let Ok(mut f) = fill.lock() {
                            f(&mut mono);
                        }
                        let expanded = expand_output(&mono, ch, rate);
                        for (out, &val) in data.iter_mut().zip(expanded.iter()) {
                            *out = ((val + 1.0) / 2.0 * u16::MAX as f32) as u16;
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::I32 => {
                let fill = fill.clone();
                let ch = actual_channels;
                let rate = actual_rate;
                device.build_output_stream(
                    &stream_config,
                    move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                        let mono_len = output_mono_len(data.len(), ch, rate);
                        let mut mono = vec![0.0f32; mono_len];
                        if let Ok(mut f) = fill.lock() {
                            f(&mut mono);
                        }
                        let expanded = expand_output(&mono, ch, rate);
                        for (out, &val) in data.iter_mut().zip(expanded.iter()) {
                            *out = (val * i32::MAX as f32) as i32;
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            SampleFormat::U8 => {
                let fill = fill.clone();
                let ch = actual_channels;
                let rate = actual_rate;
                device.build_output_stream(
                    &stream_config,
                    move |data: &mut [u8], _: &cpal::OutputCallbackInfo| {
                        let mono_len = output_mono_len(data.len(), ch, rate);
                        let mut mono = vec![0.0f32; mono_len];
                        if let Ok(mut f) = fill.lock() {
                            f(&mut mono);
                        }
                        let expanded = expand_output(&mono, ch, rate);
                        for (out, &val) in data.iter_mut().zip(expanded.iter()) {
                            *out = ((val * 128.0) + 128.0).clamp(0.0, 255.0) as u8;
                        }
                    },
                    err_fn,
                    None,
                )?
            }
            other => anyhow::bail!("unsupported output sample format: {:?}", other),
        };

        stream.play()?;
        info!("audio: output stream started");

        Ok(CpalPlaybackStream { _stream: stream })
    }
}

// ---------------------------------------------------------------------------
// Input conversion helper (already-f32 path with channel/rate conversion)
// ---------------------------------------------------------------------------

/// Down-mix multi-channel f32 data to mono and resample if needed.
fn convert_input_f32(data: &[f32], channels: u16, device_rate: u32) -> Vec<f32> {
    let mono = if channels == 1 {
        data.to_vec()
    } else {
        let ch = channels as usize;
        data.chunks_exact(ch)
            .map(|frame| frame.iter().sum::<f32>() / ch as f32)
            .collect()
    };

    if device_rate == SAMPLE_RATE {
        mono
    } else {
        resample(&mono, device_rate, SAMPLE_RATE)
    }
}

// ---------------------------------------------------------------------------
// Output conversion helpers
// ---------------------------------------------------------------------------

/// Calculate how many mono 48 kHz samples we need from the fill callback
/// to produce `total_device_samples` at the device's rate and channel count.
fn output_mono_len(total_device_samples: usize, channels: u16, device_rate: u32) -> usize {
    let device_mono = total_device_samples / channels as usize;
    if device_rate == SAMPLE_RATE {
        device_mono
    } else {
        // We need enough source samples so that after resampling we get device_mono.
        ((device_mono as f64) * (SAMPLE_RATE as f64 / device_rate as f64)).ceil() as usize
    }
}

/// Expand mono 48 kHz f32 data to the device's channel count and sample rate.
fn expand_output(mono: &[f32], channels: u16, device_rate: u32) -> Vec<f32> {
    // Step 1: resample from 48 kHz to device rate
    let resampled = if device_rate == SAMPLE_RATE {
        mono.to_vec()
    } else {
        resample(mono, SAMPLE_RATE, device_rate)
    };

    // Step 2: duplicate mono to all channels
    if channels == 1 {
        resampled
    } else {
        let ch = channels as usize;
        let mut out = Vec::with_capacity(resampled.len() * ch);
        for &s in &resampled {
            for _ in 0..ch {
                out.push(s);
            }
        }
        out
    }
}

/// Fill an f32 output buffer, handling channel/rate conversion.
fn write_output_f32(
    data: &mut [f32],
    fill: &Arc<Mutex<Box<dyn FnMut(&mut [f32]) + Send>>>,
    channels: u16,
    device_rate: u32,
) {
    if channels == 1 && device_rate == SAMPLE_RATE {
        // Fast path: no conversion needed.
        if let Ok(mut f) = fill.lock() {
            f(data);
        } else {
            data.fill(0.0);
        }
    } else {
        let mono_len = output_mono_len(data.len(), channels, device_rate);
        let mut mono = vec![0.0f32; mono_len];
        if let Ok(mut f) = fill.lock() {
            f(&mut mono);
        }
        let expanded = expand_output(&mono, channels, device_rate);
        let copy_len = data.len().min(expanded.len());
        data[..copy_len].copy_from_slice(&expanded[..copy_len]);
        // Zero any remaining samples if expanded was shorter.
        for s in &mut data[copy_len..] {
            *s = 0.0;
        }
    }
}

// ---------------------------------------------------------------------------
// CpalCaptureStream
// ---------------------------------------------------------------------------

/// Live audio capture stream wrapping a cpal input stream.
pub struct CpalCaptureStream {
    _stream: cpal::Stream,
    is_active: Arc<AtomicBool>,
}

impl AudioCaptureStream for CpalCaptureStream {
    fn set_active(&self, active: bool) {
        self.is_active.store(active, Ordering::Relaxed);
    }
}

// SAFETY: cpal::Stream is Send on all desktop platforms we target.
// The Arc<AtomicBool> is inherently Send + Sync.
unsafe impl Send for CpalCaptureStream {}

// ---------------------------------------------------------------------------
// CpalPlaybackStream
// ---------------------------------------------------------------------------

/// Live audio playback stream wrapping a cpal output stream.
pub struct CpalPlaybackStream {
    _stream: cpal::Stream,
}

impl AudioPlaybackStream for CpalPlaybackStream {}

// SAFETY: cpal::Stream is Send on all desktop platforms we target.
unsafe impl Send for CpalPlaybackStream {}
