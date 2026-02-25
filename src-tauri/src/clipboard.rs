use std::sync::Mutex;

use arboard::Clipboard;

use voxink_core::error::AppError;
use voxink_core::input::clipboard::ClipboardManager;

/// Concrete clipboard manager using arboard for cross-platform clipboard access.
pub struct ArboardClipboard {
    clipboard: Mutex<Clipboard>,
}

// SAFETY: ArboardClipboard wraps arboard::Clipboard (which may contain non-Send
// platform handles) inside a std::sync::Mutex, ensuring all access is synchronized.
unsafe impl Send for ArboardClipboard {}
unsafe impl Sync for ArboardClipboard {}

impl ArboardClipboard {
    pub fn new() -> Result<Self, AppError> {
        let clipboard = Clipboard::new()
            .map_err(|e| AppError::Paste(format!("failed to init clipboard: {e}")))?;
        Ok(Self {
            clipboard: Mutex::new(clipboard),
        })
    }
}

impl ClipboardManager for ArboardClipboard {
    fn get_text(&self) -> Result<Option<String>, AppError> {
        let mut cb = self.clipboard.lock().unwrap_or_else(|e| e.into_inner());
        match cb.get_text() {
            Ok(text) => Ok(Some(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(None),
            Err(e) => Err(AppError::Paste(format!("clipboard read failed: {e}"))),
        }
    }

    fn set_text(&self, text: &str) -> Result<(), AppError> {
        let mut cb = self.clipboard.lock().unwrap_or_else(|e| e.into_inner());
        cb.set_text(text)
            .map_err(|e| AppError::Paste(format!("clipboard write failed: {e}")))
    }
}
