use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::Stream;

use voxink_core::audio::recorder::AudioRecorder;
use voxink_core::error::AppError;

/// Concrete audio recorder using cpal for cross-platform 16kHz mono PCM capture.
pub struct CpalRecorder {
    buffer: Arc<Mutex<Vec<i16>>>,
    recording: Arc<AtomicBool>,
    stream: Mutex<Option<Stream>>,
}

impl CpalRecorder {
    pub fn new() -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            recording: Arc::new(AtomicBool::new(false)),
            stream: Mutex::new(None),
        }
    }
}

impl AudioRecorder for CpalRecorder {
    fn start(&self) -> Result<(), AppError> {
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| AppError::Audio("no input device found".to_string()))?;

        let config = cpal::StreamConfig {
            channels: 1,
            sample_rate: cpal::SampleRate(16000),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer = Arc::clone(&self.buffer);
        buffer.lock().unwrap().clear();

        let recording = Arc::clone(&self.recording);
        recording.store(true, Ordering::SeqCst);

        let stream = device
            .build_input_stream(
                &config,
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if recording.load(Ordering::SeqCst) {
                        buffer.lock().unwrap().extend_from_slice(data);
                    }
                },
                |err| eprintln!("audio stream error: {err}"),
                None,
            )
            .map_err(|e| AppError::Audio(format!("failed to build input stream: {e}")))?;

        stream
            .play()
            .map_err(|e| AppError::Audio(format!("failed to start stream: {e}")))?;

        *self.stream.lock().unwrap() = Some(stream);
        Ok(())
    }

    fn stop(&self) -> Result<Vec<i16>, AppError> {
        self.recording.store(false, Ordering::SeqCst);

        // Drop the stream to release the audio device
        *self.stream.lock().unwrap() = None;

        let samples = std::mem::take(&mut *self.buffer.lock().unwrap());
        Ok(samples)
    }

    fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }
}
