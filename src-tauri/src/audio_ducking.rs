use std::sync::Arc;

use voxpen_core::audio::ducking::AudioDucker;

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
use voxpen_core::audio::ducking::NoOpDucker;

/// Create the platform-appropriate audio ducker.
pub fn create_audio_ducker() -> Arc<dyn AudioDucker> {
    #[cfg(target_os = "windows")]
    {
        Arc::new(WindowsAudioDucker::new())
    }
    #[cfg(target_os = "linux")]
    {
        match LinuxAudioDucker::new() {
            Some(d) => Arc::new(d),
            None => {
                eprintln!("pactl not found — audio ducking disabled");
                Arc::new(voxpen_core::audio::ducking::NoOpDucker)
            }
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        Arc::new(NoOpDucker)
    }
}

// ---------------------------------------------------------------------------
// Windows: WASAPI session volume control
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod windows_impl {
    use std::sync::Mutex;

    use voxpen_core::audio::ducking::AudioDucker;
    use voxpen_core::error::AppError;
    use windows::Win32::Media::Audio::{
        eMultimedia, eRender, IAudioSessionControl, IAudioSessionControl2,
        IAudioSessionEnumerator, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
    };
    use windows::core::Interface;

    /// Saved volume for a single audio session, identified by session ID.
    struct SavedVolume {
        session_id: String,
        original_volume: f32,
    }

    /// RAII guard that calls `CoUninitialize` on drop only if we initialized COM.
    struct ComGuard {
        needs_uninit: bool,
    }

    /// RPC_E_CHANGED_MODE (0x80010106): COM already initialized with a
    /// different apartment model on this thread. Safe to proceed — we just
    /// must not call CoUninitialize since we didn't initialize it.
    const RPC_E_CHANGED_MODE: i32 = 0x80010106u32 as i32;

    impl ComGuard {
        unsafe fn init() -> Result<Self, AppError> {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_ok() {
                Ok(Self { needs_uninit: true })
            } else if hr.0 == RPC_E_CHANGED_MODE {
                // COM already initialized as STA on this thread (e.g. by
                // cpal or enigo). We can still use COM APIs.
                eprintln!("audio ducking: COM already STA, proceeding");
                Ok(Self { needs_uninit: false })
            } else {
                Err(AppError::Audio(format!("CoInitializeEx: {hr}")))
            }
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.needs_uninit {
                unsafe { CoUninitialize() };
            }
        }
    }

    pub struct WindowsAudioDucker {
        saved: Mutex<Vec<SavedVolume>>,
    }

    impl WindowsAudioDucker {
        pub fn new() -> Self {
            Self {
                saved: Mutex::new(Vec::new()),
            }
        }

        fn current_pid() -> u32 {
            std::process::id()
        }
    }

    impl AudioDucker for WindowsAudioDucker {
        fn duck(&self, volume_percent: u8) -> Result<(), AppError> {
            let target = (volume_percent.min(100) as f32) / 100.0;
            let our_pid = Self::current_pid();

            unsafe {
                let _com = ComGuard::init()?;

                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                        .map_err(|e| AppError::Audio(format!("COM enumerator: {e}")))?;

                let device = enumerator
                    .GetDefaultAudioEndpoint(eRender, eMultimedia)
                    .map_err(|e| AppError::Audio(format!("default device: {e}")))?;

                let mgr: IAudioSessionManager2 = device
                    .Activate(CLSCTX_ALL, None)
                    .map_err(|e| AppError::Audio(format!("session manager: {e}")))?;

                let session_enum: IAudioSessionEnumerator = mgr
                    .GetSessionEnumerator()
                    .map_err(|e| AppError::Audio(format!("session enum: {e}")))?;

                let count = session_enum
                    .GetCount()
                    .map_err(|e| AppError::Audio(format!("session count: {e}")))?;

                let mut saved = self
                    .saved
                    .lock()
                    .map_err(|e| AppError::Audio(format!("lock: {e}")))?;
                saved.clear();

                for i in 0..count {
                    let session: IAudioSessionControl = match session_enum.GetSession(i) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    // Skip our own process
                    let session2: IAudioSessionControl2 = match session.cast() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let pid = match session2.GetProcessId() {
                        Ok(p) => p,
                        Err(_) => continue,
                    };
                    if pid == our_pid {
                        continue;
                    }

                    let volume: ISimpleAudioVolume = match session.cast() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    let current = match volume.GetMasterVolume() {
                        Ok(v) => v,
                        Err(_) => continue,
                    };

                    // Only duck if current volume is above the target
                    if current <= target {
                        continue;
                    }

                    let session_id = match session2.GetSessionIdentifier() {
                        Ok(id) => id.to_string().unwrap_or_else(|_| format!("pid-{pid}")),
                        Err(_) => format!("pid-{pid}"),
                    };

                    saved.push(SavedVolume {
                        session_id,
                        original_volume: current,
                    });

                    let _ = volume.SetMasterVolume(target, std::ptr::null());
                }
            }

            Ok(())
        }

        fn restore(&self) -> Result<(), AppError> {
            // Recover from a poisoned mutex — restoring volumes is more
            // important than propagating a panic from a previous duck() call.
            let mut saved = self
                .saved
                .lock()
                .unwrap_or_else(|e| e.into_inner());

            if saved.is_empty() {
                return Ok(());
            }

            // Best-effort: if a session ended between duck() and restore(),
            // its saved entry simply won't match anything. This is expected.
            unsafe {
                let _com = ComGuard::init()?;

                let enumerator: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                        .map_err(|e| AppError::Audio(format!("COM enumerator: {e}")))?;

                let device = enumerator
                    .GetDefaultAudioEndpoint(eRender, eMultimedia)
                    .map_err(|e| AppError::Audio(format!("default device: {e}")))?;

                let mgr: IAudioSessionManager2 = device
                    .Activate(CLSCTX_ALL, None)
                    .map_err(|e| AppError::Audio(format!("session manager: {e}")))?;

                let session_enum: IAudioSessionEnumerator = mgr
                    .GetSessionEnumerator()
                    .map_err(|e| AppError::Audio(format!("session enum: {e}")))?;

                let count = session_enum
                    .GetCount()
                    .map_err(|e| AppError::Audio(format!("session count: {e}")))?;

                for i in 0..count {
                    let session: IAudioSessionControl = match session_enum.GetSession(i) {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    let session2: IAudioSessionControl2 = match session.cast() {
                        Ok(s) => s,
                        Err(_) => continue,
                    };

                    let session_id = match session2.GetSessionIdentifier() {
                        Ok(id) => {
                            let pid = session2.GetProcessId().unwrap_or(0);
                            id.to_string().unwrap_or_else(|_| format!("pid-{pid}"))
                        }
                        Err(_) => {
                            let pid = session2.GetProcessId().unwrap_or(0);
                            format!("pid-{pid}")
                        }
                    };

                    if let Some(entry) = saved.iter().find(|s| s.session_id == session_id) {
                        let volume: ISimpleAudioVolume = match session.cast() {
                            Ok(v) => v,
                            Err(_) => continue,
                        };
                        let _ = volume.SetMasterVolume(entry.original_volume, std::ptr::null());
                    }
                }
            }

            saved.clear();
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::WindowsAudioDucker;

// ---------------------------------------------------------------------------
// Linux: PulseAudio / PipeWire via pactl
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
mod linux_impl {
    use std::process::Command;
    use std::sync::Mutex;

    use voxpen_core::audio::ducking::AudioDucker;
    use voxpen_core::error::AppError;

    /// Saved volume for a PulseAudio sink-input.
    struct SavedSinkInput {
        index: u32,
        volume: String, // raw volume string from pactl, e.g. "50%"
    }

    pub struct LinuxAudioDucker {
        saved: Mutex<Vec<SavedSinkInput>>,
        our_pid: String,
    }

    impl LinuxAudioDucker {
        /// Returns `None` if `pactl` is not available on this system.
        pub fn new() -> Option<Self> {
            // Check that pactl exists before committing to this implementation
            let ok = Command::new("pactl")
                .arg("--version")
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            if !ok {
                return None;
            }

            Some(Self {
                saved: Mutex::new(Vec::new()),
                our_pid: std::process::id().to_string(),
            })
        }

        /// Parse `pactl list sink-inputs` output to extract (index, volume, pid) tuples.
        /// Returns `(sink_input_index, volume_percent_string, optional_pid_string)`.
        fn parse_sink_inputs(output: &str) -> Vec<(u32, String, Option<String>)> {
            let mut results = Vec::new();
            let mut current_index: Option<u32> = None;
            let mut current_volume: Option<String> = None;
            let mut current_pid: Option<String> = None;

            for line in output.lines() {
                let trimmed = line.trim();

                // New sink-input block — flush the previous one
                if let Some(rest) = trimmed.strip_prefix("Sink Input #") {
                    if let (Some(idx), Some(vol)) = (current_index, current_volume.take()) {
                        results.push((idx, vol, current_pid.take()));
                    }
                    current_index = rest.parse().ok();
                    current_volume = None;
                    current_pid = None;
                }

                // Volume: front-left: 65536 / 100% / 0.00 dB, ...
                if trimmed.starts_with("Volume:") && current_volume.is_none() {
                    if let Some(pct) = trimmed.split('/').nth(1) {
                        current_volume = Some(pct.trim().to_string());
                    }
                }

                // application.process.id = "12345"
                if trimmed.starts_with("application.process.id") {
                    if let Some(val) = trimmed.split('=').nth(1) {
                        let pid = val.trim().trim_matches('"').to_string();
                        current_pid = Some(pid);
                    }
                }
            }

            // Flush last block
            if let (Some(idx), Some(vol)) = (current_index, current_volume) {
                results.push((idx, vol, current_pid));
            }

            results
        }

        /// Parse a percentage string like "50%" into a u8 value.
        fn parse_volume_percent(s: &str) -> Option<u8> {
            s.strip_suffix('%')?.trim().parse().ok()
        }
    }

    impl AudioDucker for LinuxAudioDucker {
        fn duck(&self, volume_percent: u8) -> Result<(), AppError> {
            let target_pct = volume_percent.min(100);
            let target = format!("{target_pct}%");

            let output = Command::new("pactl")
                .args(["list", "sink-inputs"])
                .output()
                .map_err(|e| AppError::Audio(format!("pactl list: {e}")))?;

            if !output.status.success() {
                return Err(AppError::Audio(
                    "pactl list sink-inputs failed".to_string(),
                ));
            }

            let stdout = String::from_utf8_lossy(&output.stdout);
            let inputs = Self::parse_sink_inputs(&stdout);

            // Recover from a poisoned mutex
            let mut saved = self
                .saved
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            saved.clear();

            for (index, volume, pid) in inputs {
                // Skip our own process
                if pid.as_deref() == Some(&self.our_pid) {
                    continue;
                }

                // Only duck if current volume is above the target
                if let Some(current_pct) = Self::parse_volume_percent(&volume) {
                    if current_pct <= target_pct {
                        continue;
                    }
                }

                saved.push(SavedSinkInput {
                    index,
                    volume,
                });

                let _ = Command::new("pactl")
                    .args(["set-sink-input-volume", &index.to_string(), &target])
                    .output();
            }

            Ok(())
        }

        fn restore(&self) -> Result<(), AppError> {
            // Recover from a poisoned mutex
            let mut saved = self
                .saved
                .lock()
                .unwrap_or_else(|e| e.into_inner());

            for entry in saved.iter() {
                let _ = Command::new("pactl")
                    .args([
                        "set-sink-input-volume",
                        &entry.index.to_string(),
                        &entry.volume,
                    ])
                    .output();
            }

            saved.clear();
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn should_parse_empty_output() {
            let inputs = LinuxAudioDucker::parse_sink_inputs("");
            assert!(inputs.is_empty());
        }

        #[test]
        fn should_parse_single_sink_input_with_pid() {
            let output = r#"Sink Input #42
	Driver: protocol-native.c
	Owner Module: 9
	Client: 25
	Sink: 0
	Sample Specification: float32le 2ch 44100Hz
	Channel Map: front-left,front-right
	Corked: no
	Mute: no
	Volume: front-left: 65536 / 100% / 0.00 dB,   front-right: 65536 / 100% / 0.00 dB
	        balance 0.00
	Properties:
		media.name = "Playback"
		application.name = "Firefox"
		application.process.id = "1234"
"#;
            let inputs = LinuxAudioDucker::parse_sink_inputs(output);
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].0, 42);
            assert_eq!(inputs[0].1, "100%");
            assert_eq!(inputs[0].2.as_deref(), Some("1234"));
        }

        #[test]
        fn should_parse_multiple_sink_inputs() {
            let output = r#"Sink Input #10
	Volume: front-left: 32768 / 50% / -18.06 dB,   front-right: 32768 / 50% / -18.06 dB
	Properties:
		application.process.id = "111"

Sink Input #20
	Volume: front-left: 48000 / 73% / -7.63 dB,   front-right: 48000 / 73% / -7.63 dB
	Properties:
		application.process.id = "222"
"#;
            let inputs = LinuxAudioDucker::parse_sink_inputs(output);
            assert_eq!(inputs.len(), 2);
            assert_eq!(inputs[0].0, 10);
            assert_eq!(inputs[0].1, "50%");
            assert_eq!(inputs[0].2.as_deref(), Some("111"));
            assert_eq!(inputs[1].0, 20);
            assert_eq!(inputs[1].1, "73%");
            assert_eq!(inputs[1].2.as_deref(), Some("222"));
        }

        #[test]
        fn should_handle_missing_pid() {
            let output = "Sink Input #5\n\tVolume: front-left: 65536 / 100% / 0.00 dB\n";
            let inputs = LinuxAudioDucker::parse_sink_inputs(output);
            assert_eq!(inputs.len(), 1);
            assert_eq!(inputs[0].0, 5);
            assert!(inputs[0].2.is_none());
        }

        #[test]
        fn should_parse_volume_percent() {
            assert_eq!(LinuxAudioDucker::parse_volume_percent("100%"), Some(100));
            assert_eq!(LinuxAudioDucker::parse_volume_percent("50%"), Some(50));
            assert_eq!(LinuxAudioDucker::parse_volume_percent("0%"), Some(0));
            assert_eq!(LinuxAudioDucker::parse_volume_percent("abc"), None);
            assert_eq!(LinuxAudioDucker::parse_volume_percent(""), None);
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::LinuxAudioDucker;
