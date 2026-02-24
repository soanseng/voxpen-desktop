use crate::error::AppError;

/// Trait abstracting clipboard access.
///
/// Implemented by `arboard`-based clipboard in the Tauri crate.
/// Mockable in voxink-core tests via `mockall`.
#[cfg_attr(test, mockall::automock)]
pub trait ClipboardManager: Send + Sync {
    /// Read current clipboard text. Returns `None` if clipboard is empty or non-text.
    fn get_text(&self) -> Result<Option<String>, AppError>;

    /// Write text to clipboard.
    fn set_text(&self, text: &str) -> Result<(), AppError>;
}
