use tauri::{path::BaseDirectory, AppHandle, Manager};

/// Persist captured live PCM as a retryable WAV file under app data.
pub fn save_live_recording(
    app: &AppHandle,
    entry_id: &str,
    pcm_data: &[i16],
) -> Result<String, String> {
    let dir = recordings_dir(app)?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("failed to create recordings dir: {e}"))?;
    let path = dir.join(format!("{entry_id}.wav"));
    let wav_data = voxpen_core::audio::encoder::pcm_to_wav(pcm_data);
    std::fs::write(&path, wav_data).map_err(|e| format!("failed to save recording: {e}"))?;
    Ok(path.to_string_lossy().to_string())
}

pub fn read_recording(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("failed to read saved recording: {e}"))
}

fn recordings_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .resolve("recordings", BaseDirectory::AppData)
        .map_err(|e| format!("failed to resolve recordings dir: {e}"))
}
