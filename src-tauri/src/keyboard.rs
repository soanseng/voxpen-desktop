use std::sync::Mutex;

use enigo::{Direction, Enigo, Key, Keyboard, Settings as EnigoSettings};

use voxpen_core::error::AppError;
use voxpen_core::input::paste::KeySimulator;

/// Detect whether the current session is running under Wayland.
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
}

/// Create the appropriate keyboard simulator for the current platform.
///
/// On Linux Wayland sessions, uses `wtype` for reliable key simulation.
/// On all other platforms (macOS, Windows, Linux X11), uses `enigo`.
pub fn create_keyboard() -> Result<Box<dyn KeySimulator>, AppError> {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        return WtypeKeyboard::new().map(|kb| Box::new(kb) as Box<dyn KeySimulator>);
    }
    EnigoKeyboard::new().map(|kb| Box::new(kb) as Box<dyn KeySimulator>)
}

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

        let click_result = enigo
            .key(Key::Unicode('v'), Direction::Click)
            .map_err(|e| AppError::Paste(format!("key click failed: {e}")));

        // Always release the modifier, even if the click failed
        let release_result = enigo
            .key(modifier, Direction::Release)
            .map_err(|e| AppError::Paste(format!("key release failed: {e}")));

        click_result?;
        release_result?;
        Ok(())
    }

    fn copy(&self) -> Result<(), AppError> {
        let mut enigo = self.enigo.lock().unwrap_or_else(|e| e.into_inner());

        #[cfg(target_os = "macos")]
        let modifier = Key::Meta;
        #[cfg(not(target_os = "macos"))]
        let modifier = Key::Control;

        enigo
            .key(modifier, Direction::Press)
            .map_err(|e| AppError::Paste(format!("key press failed: {e}")))?;

        let click_result = enigo
            .key(Key::Unicode('c'), Direction::Click)
            .map_err(|e| AppError::Paste(format!("key click failed: {e}")));

        // Always release the modifier, even if the click failed
        let release_result = enigo
            .key(modifier, Direction::Release)
            .map_err(|e| AppError::Paste(format!("key release failed: {e}")));

        click_result?;
        release_result?;
        Ok(())
    }
}

/// Keyboard simulator for Wayland using the `wtype` command.
///
/// `wtype` is the Wayland equivalent of `xdotool type` — it injects
/// keystrokes via the Wayland input protocol, which `enigo` cannot
/// reliably do on many Wayland compositors.
///
/// Install: `pacman -S wtype` (Arch) or build from source.
pub struct WtypeKeyboard;

impl WtypeKeyboard {
    pub fn new() -> Result<Self, AppError> {
        // Verify wtype is available at init time for fast failure.
        let check = std::process::Command::new("wtype")
            .arg("--help")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match check {
            Ok(_) => Ok(Self),
            Err(_) => Err(AppError::Paste(
                "wtype not found — install with: pacman -S wtype".to_string(),
            )),
        }
    }

    fn run_wtype(args: &[&str]) -> Result<(), AppError> {
        let output = std::process::Command::new("wtype")
            .args(args)
            .output()
            .map_err(|e| AppError::Paste(format!("wtype execution failed: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Paste(format!("wtype failed: {stderr}")));
        }
        Ok(())
    }
}

impl KeySimulator for WtypeKeyboard {
    fn paste(&self) -> Result<(), AppError> {
        Self::run_wtype(&["-M", "ctrl", "-k", "v", "-m", "ctrl"])
    }

    fn copy(&self) -> Result<(), AppError> {
        Self::run_wtype(&["-M", "ctrl", "-k", "c", "-m", "ctrl"])
    }
}
