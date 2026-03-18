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
/// On Linux Wayland sessions, tries in order:
/// 1. `xdotool` — works on KDE Wayland via XWayland compatibility
/// 2. `ydotool` — kernel-level `/dev/uinput`, works on GNOME/sway
/// 3. `wtype` — Wayland virtual-keyboard protocol, wlroots-based only
/// 4. `enigo` — X11 fallback
///
/// On all other platforms (macOS, Windows, Linux X11), uses `enigo`.
pub fn create_keyboard() -> Result<Box<dyn KeySimulator>, AppError> {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        if let Ok(kb) = YdotoolKeyboard::new() {
            eprintln!("keyboard: using ydotool (Wayland)");
            return Ok(Box::new(kb));
        }
        if let Ok(kb) = XdotoolKeyboard::new() {
            eprintln!("keyboard: using xdotool (Wayland/KDE)");
            return Ok(Box::new(kb));
        }
        if let Ok(kb) = WtypeKeyboard::new() {
            eprintln!("keyboard: using wtype (Wayland/wlroots)");
            return Ok(Box::new(kb));
        }
        eprintln!("keyboard: falling back to enigo (Wayland — paste may not work)");
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

/// Keyboard simulator using `ydotool` — works on all Wayland compositors.
///
/// `ydotool` injects input events via the kernel `/dev/uinput` interface,
/// bypassing compositor-specific protocols. Requires `ydotoold` daemon running.
///
/// Setup (pick one):
/// - **System service** (recommended): `sudo systemctl enable --now ydotoold`
///   with `--socket-path /tmp/.ydotool_socket --socket-perm 0666`
/// - **User service**: add user to `input` group, re-login,
///   then `systemctl --user enable --now ydotool`
///
/// Key codes: ydotool uses Linux input event codes (not keysyms).
/// Default: Ctrl = 29, V = 47, C = 46.
///
/// **keyd awareness**: If `keyd` is running and swaps CapsLock ↔ Ctrl,
/// ydotool must send 58 (physical CapsLock) to produce Ctrl, because
/// keyd intercepts uinput events before the compositor sees them.
pub struct YdotoolKeyboard {
    socket_path: String,
    /// Effective evdev code for Left Ctrl (29 normally, 58 if keyd swaps).
    ctrl_code: &'static str,
}

/// Default socket paths to probe, in priority order.
const YDOTOOL_SOCKET_CANDIDATES: &[&str] = &[
    "/tmp/.ydotool_socket",
];

impl YdotoolKeyboard {
    pub fn new() -> Result<Self, AppError> {
        // 1. Check ydotool binary exists.
        if std::process::Command::new("ydotool")
            .args(["key", "--help"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_err()
        {
            return Err(AppError::Paste(
                "ydotool not found — install with: pacman -S ydotool".to_string(),
            ));
        }

        // 2. Find the ydotoold socket.
        let socket_path = Self::find_socket()?;

        // 3. Verify daemon connectivity with a no-op key event (delay-only).
        let check = std::process::Command::new("ydotool")
            .env("YDOTOOL_SOCKET", &socket_path)
            .args(["key", "0"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .status();
        match check {
            Ok(s) if s.success() => {
                let ctrl_code = Self::detect_ctrl_code();
                eprintln!("ydotool: connected via {socket_path} (ctrl=evdev {ctrl_code})");
                Ok(Self { socket_path, ctrl_code })
            }
            Ok(_) => Err(AppError::Paste(format!(
                "ydotool daemon not responding at {socket_path} — is ydotoold running?"
            ))),
            Err(e) => Err(AppError::Paste(format!("ydotool check failed: {e}"))),
        }
    }

    /// Resolve socket path: `$YDOTOOL_SOCKET` → `$XDG_RUNTIME_DIR` → known candidates.
    fn find_socket() -> Result<String, AppError> {
        // Explicit env override.
        if let Ok(p) = std::env::var("YDOTOOL_SOCKET") {
            if std::path::Path::new(&p).exists() {
                return Ok(p);
            }
        }

        // XDG_RUNTIME_DIR (user service default location).
        if let Ok(runtime) = std::env::var("XDG_RUNTIME_DIR") {
            let p = format!("{runtime}/.ydotool_socket");
            if std::path::Path::new(&p).exists() {
                return Ok(p);
            }
        }

        // Well-known candidate paths (system service).
        for candidate in YDOTOOL_SOCKET_CANDIDATES {
            if std::path::Path::new(candidate).exists() {
                return Ok(candidate.to_string());
            }
        }

        Err(AppError::Paste(
            "ydotoold socket not found — start ydotoold: sudo systemctl enable --now ydotoold"
                .to_string(),
        ))
    }

    /// Detect if `keyd` is swapping CapsLock ↔ Ctrl.
    ///
    /// Reads `/etc/keyd/*.conf` for rules like `capslock = leftcontrol`.
    /// If found, ydotool must send evdev 58 (physical CapsLock) to produce Ctrl.
    fn detect_ctrl_code() -> &'static str {
        let Ok(entries) = std::fs::read_dir("/etc/keyd") else {
            return "29";
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "conf") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let has_caps_to_ctrl = content
                        .lines()
                        .any(|l| {
                            let l = l.trim();
                            l.starts_with("capslock") && l.contains("leftcontrol")
                        });
                    if has_caps_to_ctrl {
                        eprintln!("ydotool: keyd swaps CapsLock↔Ctrl, using evdev 58 for Ctrl");
                        return "58";
                    }
                }
            }
        }
        "29"
    }

    fn run_ydotool(&self, args: &[&str]) -> Result<(), AppError> {
        // Use `setsid` to run ydotool in a new session, fully detached from
        // the Tauri app. Without this, KDE Wayland may associate the injected
        // uinput events with VoxPen's process rather than the focused app.
        let output = std::process::Command::new("setsid")
            .arg("ydotool")
            .env("YDOTOOL_SOCKET", &self.socket_path)
            .args(args)
            .output()
            .map_err(|e| AppError::Paste(format!("ydotool execution failed: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Paste(format!("ydotool failed: {stderr}")));
        }
        Ok(())
    }
}

impl KeySimulator for YdotoolKeyboard {
    fn paste(&self) -> Result<(), AppError> {
        let ctrl_press = format!("{}:1", self.ctrl_code);
        let ctrl_release = format!("{}:0", self.ctrl_code);
        self.run_ydotool(&["key", "--key-delay", "100", &ctrl_press, "47:1", "47:0", &ctrl_release])
    }

    fn copy(&self) -> Result<(), AppError> {
        let ctrl_press = format!("{}:1", self.ctrl_code);
        let ctrl_release = format!("{}:0", self.ctrl_code);
        self.run_ydotool(&["key", "--key-delay", "100", &ctrl_press, "46:1", "46:0", &ctrl_release])
    }
}

/// Keyboard simulator using `xdotool` — works on KDE Wayland via XWayland.
///
/// KDE Plasma forwards X11 synthetic input events from XWayland to the
/// focused Wayland window, making `xdotool` a reliable option on KDE.
/// Does NOT work on non-KDE Wayland compositors (GNOME, sway, etc.).
pub struct XdotoolKeyboard;

impl XdotoolKeyboard {
    pub fn new() -> Result<Self, AppError> {
        // xdotool requires DISPLAY (XWayland) to be available.
        if std::env::var("DISPLAY").is_err() {
            return Err(AppError::Paste(
                "xdotool: DISPLAY not set — no XWayland".to_string(),
            ));
        }
        let check = std::process::Command::new("xdotool")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        match check {
            Ok(s) if s.success() => Ok(Self),
            _ => Err(AppError::Paste("xdotool not found".to_string())),
        }
    }

    fn run_xdotool(args: &[&str]) -> Result<(), AppError> {
        let output = std::process::Command::new("xdotool")
            .args(args)
            .output()
            .map_err(|e| AppError::Paste(format!("xdotool execution failed: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(AppError::Paste(format!("xdotool failed: {stderr}")));
        }
        Ok(())
    }
}

impl KeySimulator for XdotoolKeyboard {
    fn paste(&self) -> Result<(), AppError> {
        Self::run_xdotool(&["key", "--clearmodifiers", "ctrl+v"])
    }

    fn copy(&self) -> Result<(), AppError> {
        Self::run_xdotool(&["key", "--clearmodifiers", "ctrl+c"])
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
