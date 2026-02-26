# Local Whisper Model Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add offline speech-to-text via whisper-rs as an alternative STT provider, with model download management and Settings UI.

**Architecture:** New `LocalSttProvider` implements existing `SttProvider` trait. Model files downloaded from HuggingFace, cached in app data dir. WhisperContext cached in AppState for fast reuse. GPU acceleration via compile-time Cargo features.

**Tech Stack:** whisper-rs (whisper.cpp bindings), reqwest (model download), tokio (async), Tauri events (progress), React (Settings UI)

---

### Task 1: Add whisper-rs dependency and feature flags

**Files:**
- Modify: `src-tauri/crates/voxpen-core/Cargo.toml`
- Modify: `src-tauri/Cargo.toml`

**Step 1: Add whisper-rs to voxpen-core Cargo.toml**

Add feature-gated dependency:

```toml
[features]
default = ["local-whisper"]
local-whisper = ["dep:whisper-rs"]

[dependencies]
whisper-rs = { version = "0.13", optional = true }
```

Note: Use whisper-rs 0.13.x (latest stable on crates.io). The `dep:` syntax requires Rust 2021 edition (already in use).

**Step 2: Add GPU feature flags to src-tauri/Cargo.toml**

Forward GPU features from the app crate:

```toml
[features]
local-whisper-cuda = ["voxpen-core/local-whisper", "whisper-rs/cuda"]
local-whisper-coreml = ["voxpen-core/local-whisper", "whisper-rs/coreml"]
local-whisper-metal = ["voxpen-core/local-whisper", "whisper-rs/metal"]
```

Note: The app crate does NOT need `whisper-rs` as a direct dependency — it's only used inside voxpen-core. The GPU feature flags just forward through.

**Step 3: Build to verify**

Run: `cargo build -p voxpen-core --manifest-path src-tauri/Cargo.toml`
Expected: Compiles with whisper-rs downloaded and built. First build may take a few minutes (whisper.cpp C compilation).

If whisper-rs fails to compile due to missing system libs, note the error. On Linux, `cmake` and C compiler are needed. On macOS, Xcode CLT suffices.

**Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/Cargo.toml src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add whisper-rs dependency with feature flags for local STT"
```

---

### Task 2: Add new AppError variants for local whisper

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/error.rs`

**Step 1: Write tests**

Add to the existing `mod tests` in `error.rs`:

```rust
#[test]
fn should_display_model_not_downloaded_error() {
    let err = AppError::ModelNotDownloaded("ggml-large-v3-turbo-q5_0.bin".to_string());
    assert_eq!(err.to_string(), "Model not downloaded: ggml-large-v3-turbo-q5_0.bin");
}

#[test]
fn should_display_model_download_error() {
    let err = AppError::ModelDownload("network timeout".to_string());
    assert_eq!(err.to_string(), "Model download failed: network timeout");
}

#[test]
fn should_display_local_transcription_error() {
    let err = AppError::LocalTranscription("whisper context init failed".to_string());
    assert_eq!(err.to_string(), "Local transcription failed: whisper context init failed");
}
```

**Step 2: Run tests to verify failure**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- error`
Expected: FAIL — variants don't exist yet.

**Step 3: Add error variants**

Add to the `AppError` enum in `error.rs`:

```rust
#[error("Model not downloaded: {0}")]
ModelNotDownloaded(String),

#[error("Model download failed: {0}")]
ModelDownload(String),

#[error("Local transcription failed: {0}")]
LocalTranscription(String),
```

**Step 4: Run tests**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- error`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/error.rs
git commit -m "feat: add AppError variants for local whisper model"
```

---

### Task 3: Create model management module

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/whisper/mod.rs`
- Create: `src-tauri/crates/voxpen-core/src/whisper/models.rs`
- Modify: `src-tauri/crates/voxpen-core/src/lib.rs`

This module defines the model catalog and download/status logic. It is NOT feature-gated — model management (download, status check, delete) works regardless of whether whisper-rs is compiled in.

**Step 1: Write tests for model catalog**

In `whisper/models.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_have_four_model_tiers() {
        assert_eq!(MODEL_CATALOG.len(), 4);
    }

    #[test]
    fn should_find_model_by_id() {
        let model = model_by_id("ggml-large-v3-turbo-q5_0");
        assert!(model.is_some());
        assert_eq!(model.unwrap().tier, "balanced");
    }

    #[test]
    fn should_return_none_for_unknown_model() {
        assert!(model_by_id("nonexistent").is_none());
    }

    #[test]
    fn should_have_balanced_as_default() {
        let default = default_local_model();
        assert_eq!(default.tier, "balanced");
    }

    #[test]
    fn should_report_not_downloaded_when_file_missing() {
        let dir = std::env::temp_dir().join("voxpen-test-models-missing");
        let status = get_model_status("ggml-large-v3-turbo-q5_0", &dir);
        assert!(matches!(status, ModelStatus::NotDownloaded));
    }

    #[test]
    fn should_report_ready_when_file_exists() {
        let dir = std::env::temp_dir().join("voxpen-test-models-ready");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ggml-large-v3-turbo-q5_0.bin");
        std::fs::write(&path, b"fake model data").unwrap();

        let status = get_model_status("ggml-large-v3-turbo-q5_0", &dir);
        match status {
            ModelStatus::Ready { size_bytes } => assert_eq!(size_bytes, 15),
            other => panic!("expected Ready, got {:?}", other),
        }

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn should_delete_model_file() {
        let dir = std::env::temp_dir().join("voxpen-test-models-delete");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ggml-large-v3-turbo-q5_0.bin");
        std::fs::write(&path, b"fake").unwrap();

        let result = delete_model("ggml-large-v3-turbo-q5_0", &dir);
        assert!(result.is_ok());
        assert!(!path.exists());

        std::fs::remove_dir_all(&dir).ok();
    }
}
```

**Step 2: Implement model catalog**

```rust
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::error::AppError;

/// A downloadable Whisper model.
#[derive(Debug, Clone, Serialize)]
pub struct WhisperModel {
    /// Short ID used in settings (e.g., "ggml-large-v3-turbo-q5_0")
    pub id: &'static str,
    /// Human-readable tier name
    pub tier: &'static str,
    /// Download URL
    pub url: &'static str,
    /// File name on disk
    pub filename: &'static str,
    /// Approximate download size in bytes
    pub size_bytes: u64,
}

/// Model download/ready status.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum ModelStatus {
    NotDownloaded,
    Downloading { progress: f32 },
    Ready { size_bytes: u64 },
}

pub const MODEL_CATALOG: &[WhisperModel] = &[
    WhisperModel {
        id: "ggml-small-q5_1",
        tier: "quick",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        filename: "ggml-small-q5_1.bin",
        size_bytes: 190_000_000,
    },
    WhisperModel {
        id: "ggml-large-v3-turbo-q5_0",
        tier: "balanced",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        filename: "ggml-large-v3-turbo-q5_0.bin",
        size_bytes: 574_000_000,
    },
    WhisperModel {
        id: "ggml-large-v3-turbo-q8_0",
        tier: "quality",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
        filename: "ggml-large-v3-turbo-q8_0.bin",
        size_bytes: 874_000_000,
    },
    WhisperModel {
        id: "ggml-large-v3-q5_0",
        tier: "maximum",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        filename: "ggml-large-v3-q5_0.bin",
        size_bytes: 1_080_000_000,
    },
];

/// Find a model by its ID.
pub fn model_by_id(id: &str) -> Option<&'static WhisperModel> {
    MODEL_CATALOG.iter().find(|m| m.id == id)
}

/// The default local model (balanced tier).
pub fn default_local_model() -> &'static WhisperModel {
    model_by_id("ggml-large-v3-turbo-q5_0").expect("default model must exist in catalog")
}

/// Check if a model is downloaded and ready.
pub fn get_model_status(model_id: &str, models_dir: &Path) -> ModelStatus {
    let Some(model) = model_by_id(model_id) else {
        return ModelStatus::NotDownloaded;
    };
    let path = models_dir.join(model.filename);
    if path.exists() {
        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        ModelStatus::Ready { size_bytes: size }
    } else {
        ModelStatus::NotDownloaded
    }
}

/// Get the full path to a model file.
pub fn model_path(model_id: &str, models_dir: &Path) -> Option<PathBuf> {
    model_by_id(model_id).map(|m| models_dir.join(m.filename))
}

/// Delete a downloaded model file.
pub fn delete_model(model_id: &str, models_dir: &Path) -> Result<(), AppError> {
    let Some(model) = model_by_id(model_id) else {
        return Err(AppError::ModelNotDownloaded(model_id.to_string()));
    };
    let path = models_dir.join(model.filename);
    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| AppError::Storage(format!("failed to delete model: {e}")))?;
    }
    Ok(())
}
```

**Step 3: Create whisper/mod.rs**

```rust
pub mod models;

#[cfg(feature = "local-whisper")]
pub mod provider;
```

**Step 4: Register module in lib.rs**

Add `pub mod whisper;` to `src-tauri/crates/voxpen-core/src/lib.rs`.

**Step 5: Run tests**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- whisper::models`
Expected: PASS

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/whisper/ src-tauri/crates/voxpen-core/src/lib.rs
git commit -m "feat: add whisper model catalog and status management"
```

---

### Task 4: Implement model download with progress events

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/whisper/download.rs`
- Modify: `src-tauri/crates/voxpen-core/src/whisper/mod.rs`

The download function streams the model file from HuggingFace and reports progress via a callback. It does NOT depend on whisper-rs (no feature gate needed).

**Step 1: Write download module**

```rust
use std::path::Path;

use crate::error::AppError;

/// Download a model file with progress reporting.
///
/// `on_progress` is called with (bytes_downloaded, total_bytes).
/// The file is written to `{models_dir}/{filename}` with a `.part` suffix
/// during download, then renamed on completion.
pub async fn download_model<F>(
    url: &str,
    filename: &str,
    models_dir: &Path,
    on_progress: F,
) -> Result<std::path::PathBuf, AppError>
where
    F: Fn(u64, u64) + Send + 'static,
{
    let models_dir = models_dir.to_path_buf();
    std::fs::create_dir_all(&models_dir)
        .map_err(|e| AppError::ModelDownload(format!("cannot create models dir: {e}")))?;

    let final_path = models_dir.join(filename);
    let part_path = models_dir.join(format!("{filename}.part"));

    let client = reqwest::Client::builder()
        .connect_timeout(crate::api::CONNECT_TIMEOUT)
        .build()
        .map_err(|e| AppError::ModelDownload(format!("HTTP client error: {e}")))?;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| AppError::ModelDownload(format!("download request failed: {e}")))?;

    if !response.status().is_success() {
        return Err(AppError::ModelDownload(format!(
            "HTTP {}",
            response.status().as_u16()
        )));
    }

    let total = response.content_length().unwrap_or(0);
    let mut downloaded: u64 = 0;

    let mut file = std::fs::File::create(&part_path)
        .map_err(|e| AppError::ModelDownload(format!("cannot create file: {e}")))?;

    use std::io::Write;
    let mut stream = response.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| AppError::ModelDownload(format!("download error: {e}")))?;
        file.write_all(&chunk)
            .map_err(|e| AppError::ModelDownload(format!("write error: {e}")))?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }

    file.flush()
        .map_err(|e| AppError::ModelDownload(format!("flush error: {e}")))?;
    drop(file);

    // Rename .part to final name
    std::fs::rename(&part_path, &final_path)
        .map_err(|e| AppError::ModelDownload(format!("rename error: {e}")))?;

    Ok(final_path)
}
```

Note: This requires `futures-util` for `StreamExt`. Add to Cargo.toml:
```toml
futures-util = "0.3"
```

**Step 2: Add to mod.rs**

```rust
pub mod download;
```

**Step 3: Build**

Run: `cargo build -p voxpen-core --manifest-path src-tauri/Cargo.toml`
Expected: PASS

**Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/whisper/download.rs src-tauri/crates/voxpen-core/src/whisper/mod.rs src-tauri/crates/voxpen-core/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat: add model download with streaming progress"
```

---

### Task 5: Implement LocalSttProvider (whisper-rs transcription)

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/whisper/provider.rs`

This is the core module — feature-gated behind `local-whisper`. It implements `SttProvider` using whisper-rs.

**Step 1: Implement provider**

```rust
//! Local Whisper STT provider using whisper-rs (whisper.cpp bindings).
//!
//! Feature-gated behind `local-whisper`.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::AppError;
use crate::pipeline::controller::SttProvider;
use crate::pipeline::state::Language;

/// Cached whisper context to avoid reloading the model on every transcription.
pub struct LocalSttProvider {
    context: Arc<Mutex<Option<WhisperContext>>>,
    model_path: Arc<Mutex<PathBuf>>,
    language: Arc<Mutex<Language>>,
}

impl LocalSttProvider {
    /// Create a new provider. The model is NOT loaded until the first transcription.
    pub fn new(model_path: PathBuf, language: Language) -> Self {
        Self {
            context: Arc::new(Mutex::new(None)),
            model_path: Arc::new(Mutex::new(model_path)),
            language: Arc::new(Mutex::new(language)),
        }
    }

    /// Update the model path (e.g., when user switches model tier).
    /// Invalidates the cached context so it reloads on next use.
    pub fn set_model_path(&self, path: PathBuf) {
        let mut mp = self.model_path.lock().unwrap();
        *mp = path;
        // Invalidate cached context
        let mut ctx = self.context.lock().unwrap();
        *ctx = None;
    }

    /// Update the language setting.
    pub fn set_language(&self, language: Language) {
        let mut lang = self.language.lock().unwrap();
        *lang = language;
    }

    /// Load or return the cached WhisperContext.
    fn ensure_context(&self) -> Result<Arc<Mutex<Option<WhisperContext>>>, AppError> {
        let mut ctx_guard = self.context.lock().unwrap();
        if ctx_guard.is_none() {
            let mp = self.model_path.lock().unwrap();
            if !mp.exists() {
                return Err(AppError::ModelNotDownloaded(
                    mp.to_string_lossy().to_string(),
                ));
            }
            let params = WhisperContextParameters::default();
            let context = WhisperContext::new_with_params(
                mp.to_str().ok_or_else(|| {
                    AppError::LocalTranscription("invalid model path".to_string())
                })?,
                params,
            )
            .map_err(|e| {
                AppError::LocalTranscription(format!("failed to load whisper model: {e}"))
            })?;
            *ctx_guard = Some(context);
        }
        drop(ctx_guard);
        Ok(self.context.clone())
    }
}

/// Convert i16 PCM samples to f32 normalized to [-1.0, 1.0].
fn i16_to_f32(samples: &[i16]) -> Vec<f32> {
    samples.iter().map(|&s| s as f32 / 32768.0).collect()
}

impl SttProvider for LocalSttProvider {
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let ctx_arc = match self.ensure_context() {
            Ok(c) => c,
            Err(e) => return Box::pin(std::future::ready(Err(e))),
        };
        let language = self.language.lock().unwrap().clone();

        Box::pin(async move {
            // whisper-rs inference is CPU-bound — run on blocking thread
            tokio::task::spawn_blocking(move || {
                let f32_data = i16_to_f32(&pcm_data);

                let ctx_guard = ctx_arc.lock().unwrap();
                let ctx = ctx_guard.as_ref().ok_or_else(|| {
                    AppError::LocalTranscription("context not loaded".to_string())
                })?;

                let mut state = ctx.create_state().map_err(|e| {
                    AppError::LocalTranscription(format!("failed to create state: {e}"))
                })?;

                let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

                // Set language
                if let Some(code) = language.code() {
                    params.set_language(Some(code));
                } else {
                    // Auto-detect
                    params.set_language(None);
                }

                // Set vocabulary/prompt hint
                if let Some(hint) = &vocabulary_hint {
                    params.set_initial_prompt(hint);
                } else {
                    params.set_initial_prompt(language.prompt());
                }

                // Disable printing to stdout
                params.set_print_special(false);
                params.set_print_progress(false);
                params.set_print_realtime(false);
                params.set_print_timestamps(false);

                // Single segment mode for short dictation
                params.set_single_segment(false);

                // Run inference
                state.full(params, &f32_data).map_err(|e| {
                    AppError::LocalTranscription(format!("inference failed: {e}"))
                })?;

                // Collect all segments
                let num_segments = state.full_n_segments().map_err(|e| {
                    AppError::LocalTranscription(format!("failed to get segments: {e}"))
                })?;

                let mut text = String::new();
                for i in 0..num_segments {
                    let segment = state.full_get_segment_text(i).map_err(|e| {
                        AppError::LocalTranscription(format!("segment {i} error: {e}"))
                    })?;
                    text.push_str(&segment);
                }

                Ok(text.trim().to_string())
            })
            .await
            .map_err(|e| AppError::LocalTranscription(format!("task join error: {e}")))?
        })
    }
}
```

**Step 2: Build**

Run: `cargo build -p voxpen-core --manifest-path src-tauri/Cargo.toml`
Expected: PASS (with `local-whisper` feature enabled by default)

**Step 3: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/whisper/provider.rs
git commit -m "feat: implement LocalSttProvider with whisper-rs transcription"
```

---

### Task 6: Add Tauri IPC commands for model management

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs` (register commands)
- Modify: `src-tauri/src/state.rs` (add models_dir and LocalSttProvider to AppState)

**Step 1: Add models_dir to AppState**

In `src-tauri/src/state.rs`, add:
```rust
pub models_dir: PathBuf,
```

In `src-tauri/src/lib.rs` where AppState is constructed, resolve the models directory:
```rust
let models_dir = app.path().app_data_dir().unwrap().join("models");
```

**Step 2: Add LocalSttProvider to AppState**

Add to AppState:
```rust
#[cfg(feature = "local-whisper")]
pub local_stt: Arc<voxpen_core::whisper::provider::LocalSttProvider>,
```

Initialize with the default model path.

**Step 3: Add IPC commands to commands.rs**

```rust
use voxpen_core::whisper::models::{self, ModelStatus, WhisperModel, MODEL_CATALOG};

#[tauri::command]
pub async fn get_whisper_models() -> Vec<&'static WhisperModel> {
    MODEL_CATALOG.iter().collect()
}

#[tauri::command]
pub async fn get_model_status(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<ModelStatus, String> {
    Ok(models::get_model_status(&model_id, &state.models_dir))
}

#[tauri::command]
pub async fn download_model(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    let model = models::model_by_id(&model_id)
        .ok_or_else(|| format!("unknown model: {model_id}"))?;
    let models_dir = state.models_dir.clone();
    let app_clone = app.clone();

    voxpen_core::whisper::download::download_model(
        model.url,
        model.filename,
        &models_dir,
        move |downloaded, total| {
            let progress = if total > 0 {
                downloaded as f32 / total as f32
            } else {
                0.0
            };
            let _ = app_clone.emit("model-download-progress", serde_json::json!({
                "model_id": model_id.clone(),
                "downloaded": downloaded,
                "total": total,
                "progress": progress,
            }));
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub async fn delete_whisper_model(
    state: tauri::State<'_, AppState>,
    model_id: String,
) -> Result<(), String> {
    models::delete_model(&model_id, &state.models_dir).map_err(|e| e.to_string())
}
```

**Step 4: Register commands in lib.rs**

Add to the `.invoke_handler(tauri::generate_handler![...])`:
```rust
commands::get_whisper_models,
commands::get_model_status,
commands::download_model,
commands::delete_whisper_model,
```

**Step 5: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS

**Step 6: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/state.rs src-tauri/src/lib.rs
git commit -m "feat: add Tauri IPC commands for whisper model management"
```

---

### Task 7: Wire LocalSttProvider into the hotkey pipeline

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/hotkey.rs`

Currently `GroqSttProvider` is hardcoded. When `stt_provider == "local"`, the hotkey handler should use `LocalSttProvider` instead.

**Step 1: Add provider dispatch in hotkey handler**

The existing flow in `hotkey.rs` calls `controller.on_stop_recording()` which uses the `SttProvider` stored in the controller. Since the controller is generic over `S: SttProvider`, we need a runtime dispatch approach.

The simplest approach: make the `GroqSttProvider::transcribe()` check `settings.stt_provider` and delegate to local whisper when provider is "local". This avoids changing the generic controller signature.

In `src-tauri/src/state.rs`, modify `GroqSttProvider::transcribe()`:

```rust
impl SttProvider for GroqSttProvider {
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>> {
        let settings = self.settings.clone();
        let app_handle = self.app_handle.clone();
        #[cfg(feature = "local-whisper")]
        let local_stt = self.local_stt.clone();

        Box::pin(async move {
            let s = settings.lock().await;

            // Route to local whisper when provider is "local"
            #[cfg(feature = "local-whisper")]
            if s.stt_provider == "local" {
                let lang = s.stt_language.clone();
                drop(s);
                local_stt.set_language(lang);
                return local_stt.transcribe(pcm_data, vocabulary_hint).await;
            }

            // Cloud provider path (existing logic)
            let api_key = get_api_key(&app_handle, &s.stt_provider)?;
            // ... rest of existing cloud transcription logic
        })
    }
}
```

Add `local_stt: Arc<LocalSttProvider>` field to `GroqSttProvider` struct (feature-gated).

**Step 2: Update model path when settings change**

In `save_settings` command, when `stt_provider == "local"`, update the LocalSttProvider's model path:

```rust
#[cfg(feature = "local-whisper")]
if settings.stt_provider == "local" {
    if let Some(path) = voxpen_core::whisper::models::model_path(&settings.stt_model, &state.models_dir) {
        state.local_stt.set_model_path(path);
    }
}
```

**Step 3: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS

**Step 4: Commit**

```bash
git add src-tauri/src/state.rs src-tauri/src/hotkey.rs src-tauri/src/commands.rs
git commit -m "feat: wire LocalSttProvider into hotkey pipeline with runtime dispatch"
```

---

### Task 8: Add i18n keys for local whisper UI

**Files:**
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add keys to en.json**

```json
"localWhisper": "Local (Whisper)",
"localWhisperHint": "Offline speech-to-text. Audio never leaves your computer.",
"modelTier": {
    "quick": "Quick",
    "balanced": "Balanced",
    "quality": "Quality",
    "maximum": "Maximum"
},
"modelTierSize": "~{{size}} MB",
"modelNotDownloaded": "Not downloaded",
"modelReady": "Ready ({{size}} MB)",
"modelDownloading": "Downloading {{progress}}%...",
"downloadModel": "Download",
"deleteModel": "Delete Model",
"deleteModelConfirm": "Click again to confirm",
"downloadFailed": "Download failed: {{error}}",
"noApiKeyNeeded": "No API key needed — runs entirely offline."
```

**Step 2: Add keys to zh-TW.json**

```json
"localWhisper": "本機 (Whisper)",
"localWhisperHint": "離線語音轉文字。音訊不會離開您的電腦。",
"modelTier": {
    "quick": "快速",
    "balanced": "平衡",
    "quality": "高品質",
    "maximum": "最高品質"
},
"modelTierSize": "約 {{size}} MB",
"modelNotDownloaded": "尚未下載",
"modelReady": "已就緒（{{size}} MB）",
"modelDownloading": "下載中 {{progress}}%...",
"downloadModel": "下載模型",
"deleteModel": "刪除模型",
"deleteModelConfirm": "再按一次確認刪除",
"downloadFailed": "下載失敗：{{error}}",
"noApiKeyNeeded": "無需 API 金鑰 — 完全離線運行。"
```

**Step 3: Commit**

```bash
git add src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add i18n keys for local whisper model UI"
```

---

### Task 9: Add TypeScript types and Tauri invoke wrappers

**Files:**
- Modify: `src/types/settings.ts`
- Modify: `src/lib/tauri.ts`

**Step 1: Add types**

In `src/types/settings.ts`, no changes needed to the Settings interface — `stt_provider: string` already accommodates "local".

Add model types:

```typescript
export interface WhisperModel {
    id: string;
    tier: string;
    url: string;
    filename: string;
    size_bytes: number;
}

export type ModelStatus =
    | { type: "NotDownloaded" }
    | { type: "Downloading"; data: { progress: number } }
    | { type: "Ready"; data: { size_bytes: number } };
```

**Step 2: Add Tauri invoke wrappers**

In `src/lib/tauri.ts`:

```typescript
export async function getWhisperModels(): Promise<WhisperModel[]> {
    return invoke<WhisperModel[]>("get_whisper_models");
}

export async function getModelStatus(modelId: string): Promise<ModelStatus> {
    return invoke<ModelStatus>("get_model_status", { modelId });
}

export async function downloadModel(modelId: string): Promise<void> {
    return invoke("download_model", { modelId });
}

export async function deleteWhisperModel(modelId: string): Promise<void> {
    return invoke("delete_whisper_model", { modelId });
}
```

**Step 3: Commit**

```bash
git add src/types/settings.ts src/lib/tauri.ts
git commit -m "feat: add TypeScript types and invoke wrappers for whisper models"
```

---

### Task 10: Update SttSection UI for local provider

**Files:**
- Modify: `src/components/Settings/SttSection.tsx`

**Step 1: Add "Local (Whisper)" to provider list**

```typescript
const STT_PROVIDERS = [
    { value: "groq", label: "Groq" },
    { value: "openai", label: "OpenAI" },
    { value: "local", label: t("localWhisper") },
    { value: "custom", label: "Custom Server" },
];
```

Note: `label` needs to use `t()` so the provider list must be a function or useMemo.

**Step 2: Conditionally render based on provider**

When `stt_provider === "local"`:
- **Hide** the API key section entirely
- **Show** instead: a hint "No API key needed — runs entirely offline."
- **Replace** model dropdown with model tier selector (Quick/Balanced/Quality/Maximum)
- **Show** model status: NotDownloaded / Ready / Downloading progress bar
- **Show** Download or Delete button based on status

When `stt_provider !== "local"`:
- Show existing API key + model UI (unchanged)

**Step 3: Add model status state and download handler**

```typescript
const [modelStatus, setModelStatus] = useState<ModelStatus | null>(null);
const [downloading, setDownloading] = useState(false);
const [downloadError, setDownloadError] = useState<string | null>(null);
const [deleteConfirm, setDeleteConfirm] = useState(false);

// Load model status when provider is local
useEffect(() => {
    if (settings.stt_provider === "local") {
        getModelStatus(settings.stt_model).then(setModelStatus).catch(() => {});
    }
}, [settings.stt_provider, settings.stt_model]);

// Listen for download progress events
useEffect(() => {
    if (settings.stt_provider !== "local") return;
    const unlisten = listen("model-download-progress", (event) => {
        const { progress } = event.payload as { progress: number };
        setModelStatus({ type: "Downloading", data: { progress: Math.round(progress * 100) } });
    });
    return () => { unlisten.then(fn => fn()); };
}, [settings.stt_provider]);
```

**Step 4: Model tier selector**

Map model catalog to dropdown options with size labels:
```typescript
const LOCAL_MODELS = [
    { value: "ggml-small-q5_1", tier: "quick", sizeMb: 190 },
    { value: "ggml-large-v3-turbo-q5_0", tier: "balanced", sizeMb: 574 },
    { value: "ggml-large-v3-turbo-q8_0", tier: "quality", sizeMb: 874 },
    { value: "ggml-large-v3-q5_0", tier: "maximum", sizeMb: 1080 },
];
```

Dropdown label: `t(`modelTier.${m.tier}`) — ~${m.sizeMb} MB`

When user selects a model tier, call `onUpdate("stt_model", m.value)`.

**Step 5: Download/delete buttons**

```tsx
{modelStatus?.type === "NotDownloaded" && (
    <button onClick={handleDownload} disabled={downloading}>
        {t("downloadModel")}
    </button>
)}
{modelStatus?.type === "Downloading" && (
    <div className="...">
        <div className="..." style={{ width: `${modelStatus.data.progress}%` }} />
        <span>{t("modelDownloading", { progress: modelStatus.data.progress })}</span>
    </div>
)}
{modelStatus?.type === "Ready" && (
    <div>
        <span>{t("modelReady", { size: Math.round(modelStatus.data.size_bytes / 1_000_000) })}</span>
        <button onClick={handleDelete}>
            {deleteConfirm ? t("deleteModelConfirm") : t("deleteModel")}
        </button>
    </div>
)}
```

**Step 6: Build frontend**

Run: `pnpm build`
Expected: PASS, no TypeScript errors

**Step 7: Commit**

```bash
git add src/components/Settings/SttSection.tsx
git commit -m "feat: add local whisper model UI to SttSection"
```

---

### Task 11: Integration test and cleanup

**Files:** All modified files

**Step 1: Run all Rust tests**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests pass (existing 220+ tests + new whisper model tests)

**Step 2: Run clippy**

Run: `cargo clippy -p voxpen-core --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: No warnings

**Step 3: Build full Tauri app**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: Compiles successfully

**Step 4: Build frontend**

Run: `pnpm build`
Expected: No TypeScript errors

**Step 5: Run frontend checks**

Run: `pnpm tsc --noEmit`
Expected: No type errors

**Step 6: Final commit if any fixes needed**

```bash
git commit -m "chore: integration test fixes for local whisper"
```
