use std::thread;
use std::time::Duration;

use crate::error::AppError;
use crate::input::clipboard::ClipboardManager;

/// Trait abstracting keyboard simulation.
///
/// Implemented by `enigo`-based simulator in the Tauri crate.
#[cfg_attr(test, mockall::automock)]
pub trait KeySimulator: Send + Sync {
    /// Simulate Cmd+V (macOS) or Ctrl+V (Windows/Linux).
    fn paste(&self) -> Result<(), AppError>;
}

/// Small delay after writing to the clipboard to let the OS finish processing
/// the clipboard-change notification chain before we simulate Ctrl+V.
#[cfg(target_os = "windows")]
const PRE_PASTE_DELAY: Duration = Duration::from_millis(60);

#[cfg(not(target_os = "windows"))]
const PRE_PASTE_DELAY: Duration = Duration::from_millis(10);

/// Delay after the paste keystroke before restoring the original clipboard.
/// The target app must have time to read the clipboard contents.
/// Windows needs more time due to clipboard chain processing and
/// potential antivirus scanning. macOS/Linux are faster.
#[cfg(target_os = "windows")]
const POST_PASTE_DELAY: Duration = Duration::from_millis(350);

#[cfg(not(target_os = "windows"))]
const POST_PASTE_DELAY: Duration = Duration::from_millis(100);

/// Paste text at cursor position using clipboard + key simulation.
///
/// Strategy (same as Typeless/Wispr Flow/1Password):
/// 1. Save current clipboard content
/// 2. Write transcription to clipboard
/// 3. Wait briefly for clipboard to settle
/// 4. Simulate paste keystroke
/// 5. Wait for target app to process the paste (100ms on macOS/Linux, 350ms on Windows)
/// 6. Restore original clipboard
///
/// If paste simulation fails, text remains on clipboard (fallback).
///
/// **Threading note**: This function uses `thread::sleep` because the underlying
/// `enigo` and `arboard` APIs are synchronous and require OS event loop access.
/// When called from async context, wrap in `tokio::task::spawn_blocking`.
pub fn paste_text(
    clipboard: &dyn ClipboardManager,
    keys: &dyn KeySimulator,
    text: &str,
) -> Result<(), AppError> {
    // 1. Save original clipboard
    let original = clipboard.get_text().unwrap_or(None);

    // 2. Write transcription to clipboard
    clipboard.set_text(text)?;

    // 3. Let the OS finish clipboard-change notifications before simulating paste
    thread::sleep(PRE_PASTE_DELAY);

    // 4. Simulate paste
    let paste_result = keys.paste();

    // 5. Wait for the target app to read the clipboard before restoring
    thread::sleep(POST_PASTE_DELAY);

    // 6. Restore original clipboard (best-effort, don't fail if this fails)
    if let Some(ref orig) = original {
        let _ = clipboard.set_text(orig);
    }

    paste_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::clipboard::MockClipboardManager;
    use mockall::predicate::eq;

    #[test]
    fn should_save_and_restore_clipboard() {
        let mut clipboard = MockClipboardManager::new();
        let mut keys = MockKeySimulator::new();

        // Expect: read original → "old text"
        clipboard
            .expect_get_text()
            .times(1)
            .returning(|| Ok(Some("old text".to_string())));

        // Expect: write transcription
        clipboard
            .expect_set_text()
            .with(eq("new transcription"))
            .times(1)
            .returning(|_| Ok(()));

        // Expect: simulate paste
        keys.expect_paste().times(1).returning(|| Ok(()));

        // Expect: restore original
        clipboard
            .expect_set_text()
            .with(eq("old text"))
            .times(1)
            .returning(|_| Ok(()));

        let result = paste_text(&clipboard, &keys, "new transcription");
        assert!(result.is_ok());
    }

    #[test]
    fn should_write_text_and_simulate_paste() {
        let mut clipboard = MockClipboardManager::new();
        let mut keys = MockKeySimulator::new();

        // No original clipboard content
        clipboard
            .expect_get_text()
            .times(1)
            .returning(|| Ok(None));

        // Write transcription
        clipboard
            .expect_set_text()
            .with(eq("hello"))
            .times(1)
            .returning(|_| Ok(()));

        // Simulate paste
        keys.expect_paste().times(1).returning(|| Ok(()));

        // No restore call (original was None)

        let result = paste_text(&clipboard, &keys, "hello");
        assert!(result.is_ok());
    }

    #[test]
    fn should_handle_clipboard_restore_failure_gracefully() {
        let mut clipboard = MockClipboardManager::new();
        let mut keys = MockKeySimulator::new();

        // Read original
        clipboard
            .expect_get_text()
            .times(1)
            .returning(|| Ok(Some("original".to_string())));

        // Write transcription
        clipboard
            .expect_set_text()
            .with(eq("transcribed"))
            .times(1)
            .returning(|_| Ok(()));

        // Paste succeeds
        keys.expect_paste().times(1).returning(|| Ok(()));

        // Restore fails — should not propagate error
        clipboard
            .expect_set_text()
            .with(eq("original"))
            .times(1)
            .returning(|_| Err(AppError::Paste("restore failed".to_string())));

        // Overall result should still be Ok (paste itself succeeded)
        let result = paste_text(&clipboard, &keys, "transcribed");
        assert!(result.is_ok());
    }

    #[test]
    fn should_propagate_paste_failure() {
        let mut clipboard = MockClipboardManager::new();
        let mut keys = MockKeySimulator::new();

        clipboard
            .expect_get_text()
            .times(1)
            .returning(|| Ok(None));

        clipboard
            .expect_set_text()
            .times(1)
            .returning(|_| Ok(()));

        // Paste fails
        keys.expect_paste()
            .times(1)
            .returning(|| Err(AppError::Paste("simulation failed".to_string())));

        let result = paste_text(&clipboard, &keys, "text");
        assert!(matches!(result, Err(AppError::Paste(_))));
    }
}
