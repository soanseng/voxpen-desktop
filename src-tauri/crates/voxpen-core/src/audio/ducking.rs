use crate::error::AppError;

/// Trait for lowering other applications' audio volume during recording.
///
/// Platform implementations:
/// - **Windows**: WASAPI session enumeration + volume control
/// - **Linux**: PulseAudio/PipeWire via `pactl` commands
/// - **macOS / fallback**: No-op
pub trait AudioDucker: Send + Sync {
    /// Lower other applications' audio to the configured volume level.
    /// Called when recording starts. Errors are non-fatal.
    fn duck(&self, volume_percent: u8) -> Result<(), AppError>;

    /// Restore other applications' audio to their original levels.
    /// Called when recording stops. Errors are non-fatal.
    fn restore(&self) -> Result<(), AppError>;
}

/// No-op implementation for platforms without ducking support.
pub struct NoOpDucker;

impl AudioDucker for NoOpDucker {
    fn duck(&self, _volume_percent: u8) -> Result<(), AppError> {
        Ok(())
    }

    fn restore(&self) -> Result<(), AppError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_ducker_should_succeed() {
        let ducker = NoOpDucker;
        assert!(ducker.duck(20).is_ok());
        assert!(ducker.restore().is_ok());
    }
}
