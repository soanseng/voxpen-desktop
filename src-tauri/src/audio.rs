use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{SampleRate, Stream};

use voxink_core::audio::encoder;
use voxink_core::audio::recorder::AudioRecorder;
use voxink_core::error::AppError;

/// Target sample rate for Whisper API (16 kHz mono).
const TARGET_RATE: u32 = encoder::SAMPLE_RATE;

/// Concrete audio recorder using cpal for cross-platform PCM capture.
///
/// Records at the device's native sample rate / channel count, then resamples
/// and downmixes to 16 kHz mono in [`stop()`] so the rest of the pipeline is
/// unaffected.
pub struct CpalRecorder {
    buffer: Arc<Mutex<Vec<i16>>>,
    recording: Arc<AtomicBool>,
    stream: Mutex<Option<Stream>>,
    /// Native sample rate the device is actually recording at.
    native_rate: Mutex<u32>,
    /// Native channel count the device is actually recording at.
    native_channels: Mutex<u16>,
    /// Preferred input device name. None = system default.
    preferred_device: Mutex<Option<String>>,
}

// SAFETY: CpalRecorder wraps cpal::Stream (which contains raw pointers and is
// not Send) inside a std::sync::Mutex, ensuring all access is synchronized.
// The Stream itself is safe to use from any thread when access is serialized.
unsafe impl Send for CpalRecorder {}
unsafe impl Sync for CpalRecorder {}

impl CpalRecorder {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            recording: Arc::new(AtomicBool::new(false)),
            stream: Mutex::new(None),
            native_rate: Mutex::new(TARGET_RATE),
            native_channels: Mutex::new(1),
            preferred_device: Mutex::new(None),
        }
    }

    /// Set the preferred input device. None = use system default.
    pub fn set_preferred_device(&self, name: Option<String>) {
        *self.preferred_device.lock().unwrap_or_else(|e| e.into_inner()) = name;
    }
}

/// List available audio input devices.
pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

impl AudioRecorder for CpalRecorder {
    fn start(&self) -> Result<(), AppError> {
        let host = cpal::default_host();
        let preferred = self
            .preferred_device
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let device = if let Some(ref name) = preferred {
            host.input_devices()
                .ok()
                .and_then(|mut devs| devs.find(|d| d.name().ok().as_deref() == Some(name)))
                .or_else(|| host.default_input_device())
        } else {
            host.default_input_device()
        }
        .ok_or_else(|| AppError::Audio("no input device found".to_string()))?;

        // Use the device's preferred config — avoids "unsupported config" on
        // Windows devices that don't natively support 16 kHz mono.
        let default_cfg = device
            .default_input_config()
            .map_err(|e| AppError::Audio(format!("no supported input config: {e}")))?;

        let rate = default_cfg.sample_rate().0;
        let channels = default_cfg.channels();

        let config = cpal::StreamConfig {
            channels,
            sample_rate: SampleRate(rate),
            buffer_size: cpal::BufferSize::Default,
        };

        *self.native_rate.lock().unwrap_or_else(|e| e.into_inner()) = rate;
        *self.native_channels.lock().unwrap_or_else(|e| e.into_inner()) = channels;

        let buffer = Arc::clone(&self.buffer);
        buffer.lock().unwrap_or_else(|e| e.into_inner()).clear();

        let recording = Arc::clone(&self.recording);
        recording.store(true, Ordering::SeqCst);

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if recording.load(Ordering::SeqCst) {
                        buffer.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(data);
                    }
                },
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| AppError::Audio(format!("failed to build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| AppError::Audio(format!("failed to start stream: {e}")))?;

        *self.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some(stream);
        Ok(())
    }

    fn stop(&self) -> Result<Vec<i16>, AppError> {
        self.recording.store(false, Ordering::SeqCst);

        // Drop the stream to release the audio device
        *self.stream.lock().unwrap_or_else(|e| e.into_inner()) = None;

        let raw = std::mem::take(&mut *self.buffer.lock().unwrap_or_else(|e| e.into_inner()));
        let rate = *self.native_rate.lock().unwrap_or_else(|e| e.into_inner());
        let channels = *self.native_channels.lock().unwrap_or_else(|e| e.into_inner());

        let samples = to_16khz_mono(&raw, rate, channels);
        Ok(samples)
    }

    fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}

/// Downmix multi-channel audio to mono, then resample to [`TARGET_RATE`].
///
/// Uses linear interpolation which is good enough for speech audio destined
/// for the Whisper STT API.
fn to_16khz_mono(raw: &[i16], src_rate: u32, channels: u16) -> Vec<i16> {
    if raw.is_empty() {
        return Vec::new();
    }

    // Step 1: downmix to mono by averaging channels
    let ch = channels as usize;
    let mono: Vec<i16> = if ch <= 1 {
        raw.to_vec()
    } else {
        raw.chunks_exact(ch)
            .map(|frame| {
                let sum: i32 = frame.iter().map(|&s| s as i32).sum();
                (sum / ch as i32) as i16
            })
            .collect()
    };

    // Step 2: resample if needed
    if src_rate == TARGET_RATE {
        return mono;
    }

    let src_len = mono.len();
    let dst_len = ((src_len as u64) * TARGET_RATE as u64 / src_rate as u64) as usize;
    if dst_len == 0 {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(dst_len);
    let ratio = src_rate as f64 / TARGET_RATE as f64;

    for i in 0..dst_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;

        let s = if idx + 1 < src_len {
            // Linear interpolation between adjacent samples
            let a = mono[idx] as f64;
            let b = mono[idx + 1] as f64;
            a + frac * (b - a)
        } else {
            mono[idx.min(src_len - 1)] as f64
        };

        out.push(s.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_already_16khz_mono() {
        let input: Vec<i16> = vec![100, 200, 300];
        let result = to_16khz_mono(&input, 16_000, 1);
        assert_eq!(result, input);
    }

    #[test]
    fn downmix_stereo_to_mono() {
        // stereo: L=100 R=200, L=300 R=400
        let input: Vec<i16> = vec![100, 200, 300, 400];
        let result = to_16khz_mono(&input, 16_000, 2);
        assert_eq!(result, vec![150, 350]);
    }

    #[test]
    fn resample_48k_to_16k() {
        // 48kHz mono → 16kHz mono (3:1 ratio)
        // 6 samples at 48kHz = 2 samples at 16kHz
        let input: Vec<i16> = vec![0, 100, 200, 300, 400, 500];
        let result = to_16khz_mono(&input, 48_000, 1);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn resample_44100_to_16k() {
        // 441 samples at 44.1kHz ≈ 160 samples at 16kHz
        let input: Vec<i16> = (0..441).map(|i| (i * 10) as i16).collect();
        let result = to_16khz_mono(&input, 44_100, 1);
        let expected_len = (441u64 * 16_000 / 44_100) as usize;
        assert_eq!(result.len(), expected_len);
    }

    #[test]
    fn empty_input_returns_empty() {
        let result = to_16khz_mono(&[], 48_000, 2);
        assert!(result.is_empty());
    }

    #[test]
    fn resample_stereo_48k() {
        // 6 stereo frames at 48kHz → 2 mono samples at 16kHz
        // frames: (0,100), (200,300), (400,500), (600,700), (800,900), (1000,1100)
        let input: Vec<i16> = vec![0, 100, 200, 300, 400, 500, 600, 700, 800, 900, 1000, 1100];
        let result = to_16khz_mono(&input, 48_000, 2);
        assert_eq!(result.len(), 2);
    }
}
