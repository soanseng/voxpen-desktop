use std::sync::Mutex;

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};

use voxink_core::error::AppError;
use voxink_core::input::paste::KeySimulator;

/// Concrete keyboard simulator using enigo for paste keystroke simulation.
pub struct EnigoKeyboard {
    enigo: Mutex<Enigo>,
}

// SAFETY: EnigoKeyboard wraps Enigo (which may contain non-Send platform handles)
// inside a std::sync::Mutex, ensuring all access is synchronized.
unsafe impl Send for EnigoKeyboard {}
unsafe impl Sync for EnigoKeyboard {}

impl EnigoKeyboard {
    pub fn new() -> Result<Self, AppError> {
        let enigo = Enigo::new(&EnigoSettings::default())
            .map_err(|e| AppError::Paste(format!("failed to init keyboard simulator: {e}")))?;
        Ok(Self {
            enigo: Mutex::new(enigo),
        })
    }
}

impl KeySimulator for EnigoKeyboard {
    fn paste(&self) -> Result<(), AppError> {
        let mut enigo = self.enigo.lock().unwrap_or_else(|e| e.into_inner());

        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;

        enigo
            .key(modifier, Direction::Press)
            .map_err(|e| AppError::Paste(format!("key press failed: {e}")))?;
        enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|e| AppError::Paste(format!("key click failed: {e}")))?;
        enigo
            .key(modifier, Direction::Release)
            .map_err(|e| AppError::Paste(format!("key release failed: {e}")))?;

        Ok(())
    }
}
