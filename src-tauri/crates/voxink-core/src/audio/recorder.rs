use crate::error::AppError;

/// Trait abstracting audio recording hardware.
///
/// Implemented by `cpal`-based recorder in the Tauri crate.
/// Mockable in voxink-core tests via `mockall`.
#[cfg_attr(test, mockall::automock)]
pub trait AudioRecorder: Send + Sync {
    /// Begin capturing PCM audio from the default input device.
    fn start(&self) -> Result<(), AppError>;

    /// Stop capturing and return accumulated PCM samples.
    fn stop(&self) -> Result<Vec<i16>, AppError>;

    /// Whether recording is currently in progress.
    fn is_recording(&self) -> bool;
}
