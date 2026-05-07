use std::sync::Mutex;

use arboard::Clipboard;

use voxpen_core::error::AppError;
use voxpen_core::input::clipboard::ClipboardManager;

/// Create the appropriate clipboard manager for the current platform.
///
/// On Linux Wayland, uses `wl-copy`/`wl-paste` because `arboard` (smithay-clipboard)
/// requires a focused surface to serve clipboard data. Since VoxPen is a tray app
/// without a focused window, `arboard`-set clipboard content is invisible to other apps.
/// `wl-copy` forks a background daemon that serves clipboard data independently of focus.
///
/// On all other platforms, uses `arboard`.
pub fn create_clipboard() -> Result<Box<dyn ClipboardManager>, AppError> {
    #[cfg(target_os = "linux")]
    if crate::keyboard::is_wayland() {
        if let Ok(cb) = WlClipboard::new() {
            eprintln!("clipboard: using wl-copy/wl-paste (Wayland)");
            return Ok(Box::new(cb));
        }
        eprintln!("clipboard: wl-clipboard not available, falling back to arboard");
    }
    ArboardClipboard::new().map(|cb| Box::new(cb) as Box<dyn ClipboardManager>)
}

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

/// Check if `create_clipboard` selects the correct implementation for the platform.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_create_clipboard_without_panicking() {
        let result = create_clipboard();
        if has_desktop_clipboard_session() {
            if let Err(e) = result {
                panic!("clipboard init failed: {e}");
            }
        } else if let Err(e) = result {
            eprintln!("clipboard init unavailable in headless test environment: {e}");
        }
    }

    fn has_desktop_clipboard_session() -> bool {
        #[cfg(target_os = "linux")]
        {
            std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
        }

        #[cfg(not(target_os = "linux"))]
        {
            true
        }
    }
}

/// Wayland clipboard using `wl-copy` and `wl-paste` commands.
///
/// Unlike `arboard` (which uses smithay-clipboard and requires a focused surface),
/// `wl-copy` forks a background process that serves clipboard data to any app,
/// regardless of which window has focus. This is critical for tray apps like VoxPen.
#[cfg(target_os = "linux")]
pub struct WlClipboard;

#[cfg(target_os = "linux")]
impl WlClipboard {
    pub fn new() -> Result<Self, AppError> {
        // Verify both wl-copy and wl-paste are available.
        let copy_ok = std::process::Command::new("wl-copy")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok();
        let paste_ok = std::process::Command::new("wl-paste")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok();
        if copy_ok && paste_ok {
            Ok(Self)
        } else {
            Err(AppError::Paste(
                "wl-clipboard not found — install with: pacman -S wl-clipboard".to_string(),
            ))
        }
    }
}

#[cfg(target_os = "linux")]
impl ClipboardManager for WlClipboard {
    fn get_text(&self) -> Result<Option<String>, AppError> {
        let output = std::process::Command::new("wl-paste")
            .args(["--no-newline", "--type", "text/plain"])
            .output()
            .map_err(|e| AppError::Paste(format!("wl-paste failed: {e}")))?;
        if output.status.success() {
            let text = String::from_utf8_lossy(&output.stdout).to_string();
            if text.is_empty() {
                Ok(None)
            } else {
                Ok(Some(text))
            }
        } else {
            // wl-paste exits non-zero when clipboard is empty or has non-text content.
            Ok(None)
        }
    }

    fn set_text(&self, text: &str) -> Result<(), AppError> {
        use std::io::Write;
        let mut child = std::process::Command::new("wl-copy")
            .args(["--type", "text/plain"])
            .stdin(std::process::Stdio::piped())
            // Use null for stdout/stderr — wl-copy forks a background daemon
            // that inherits pipes, so wait_with_output() would hang forever.
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| AppError::Paste(format!("wl-copy spawn failed: {e}")))?;
        let write_result = if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| AppError::Paste(format!("wl-copy write failed: {e}")))
        } else {
            Ok(())
        };
        // stdin is dropped here regardless of write success — wl-copy gets EOF.
        write_result?;
        let status = child
            .wait()
            .map_err(|e| AppError::Paste(format!("wl-copy wait failed: {e}")))?;
        if !status.success() {
            return Err(AppError::Paste(format!("wl-copy exited with {status}")));
        }
        Ok(())
    }
}
