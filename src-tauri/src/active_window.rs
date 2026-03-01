//! Cross-platform detection of the currently focused application name.
//!
//! Returns `None` gracefully if detection fails (Wayland, permission denied, etc.).
//! The returned name is lowercased and suitable for substring matching.

/// Returns the lowercase name of the currently active application, or `None`
/// if detection is unavailable or fails.
pub fn get_active_app_name() -> Option<String> {
    platform::get_active_app_name()
}

#[cfg(target_os = "linux")]
mod platform {
    pub fn get_active_app_name() -> Option<String> {
        // Only works on X11. Wayland lacks a stable API for this.
        if std::env::var("WAYLAND_DISPLAY").is_ok() && std::env::var("DISPLAY").is_err() {
            return None;
        }

        use x11rb::{
            connection::Connection,
            protocol::xproto::{AtomEnum, ConnectionExt, Window},
            rust_connection::RustConnection,
        };

        let (conn, screen_num) = RustConnection::connect(None).ok()?;
        let screen = &conn.setup().roots[screen_num];

        // Prefer _NET_ACTIVE_WINDOW (set by EWMH-compliant window managers).
        let net_active_atom = conn
            .intern_atom(false, b"_NET_ACTIVE_WINDOW")
            .ok()?
            .reply()
            .ok()?
            .atom;

        let active_prop = conn
            .get_property(false, screen.root, net_active_atom, AtomEnum::WINDOW, 0, 1)
            .ok()?
            .reply()
            .ok()?;

        let focused: Window = if active_prop.value.len() >= 4 {
            let bytes: [u8; 4] = active_prop.value[..4].try_into().ok()?;
            u32::from_ne_bytes(bytes)
        } else {
            conn.get_input_focus().ok()?.reply().ok()?.focus
        };

        if focused == 0 || focused == screen.root {
            return None;
        }

        get_wm_class(&conn, focused, screen.root)
    }

    fn get_wm_class(
        conn: &x11rb::rust_connection::RustConnection,
        mut window: x11rb::protocol::xproto::Window,
        root: x11rb::protocol::xproto::Window,
    ) -> Option<String> {
        use x11rb::protocol::xproto::{AtomEnum, ConnectionExt};

        // Walk up the window tree (max 10 levels) to find a window with WM_CLASS.
        for _ in 0..10 {
            let prop = conn
                .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 1024)
                .ok()?
                .reply()
                .ok()?;

            if !prop.value.is_empty() {
                let raw = String::from_utf8_lossy(&prop.value);
                let parts: Vec<&str> = raw.split('\0').filter(|s| !s.is_empty()).collect();
                let name = parts.get(1).or_else(|| parts.first())?;
                return Some(name.to_lowercase());
            }

            let tree = conn.query_tree(window).ok()?.reply().ok()?;
            if tree.parent == root || tree.parent == 0 {
                break;
            }
            window = tree.parent;
        }

        None
    }
}

#[cfg(target_os = "macos")]
mod platform {
    pub fn get_active_app_name() -> Option<String> {
        let output = std::process::Command::new("osascript")
            .arg("-e")
            .arg(
                "tell application \"System Events\" \
                 to get name of first process whose frontmost is true",
            )
            .output()
            .ok()?;

        if output.status.success() {
            let name = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_lowercase();
            if !name.is_empty() {
                return Some(name);
            }
        }
        None
    }
}

#[cfg(target_os = "windows")]
mod platform {
    pub fn get_active_app_name() -> Option<String> {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::{
                ProcessStatus::GetModuleFileNameExW,
                Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ},
            },
            UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        };

        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }

            let mut pid: u32 = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if pid == 0 {
                return None;
            }

            let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
            if handle.is_null() {
                return None;
            }

            let mut buf = vec![0u16; 512];
            let len = GetModuleFileNameExW(handle, std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
            CloseHandle(handle);

            if len == 0 {
                return None;
            }

            let os_str = OsString::from_wide(&buf[..len as usize]);
            let path_str = os_str.to_string_lossy().to_string();
            let path = std::path::Path::new(&path_str);
            Some(path.file_stem()?.to_string_lossy().to_lowercase())
        }
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
mod platform {
    pub fn get_active_app_name() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_option_without_panicking() {
        let _ = get_active_app_name();
    }
}
