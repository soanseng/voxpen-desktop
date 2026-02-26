use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use crate::error::AppError;
use crate::pipeline::controller::SttProvider;
use crate::pipeline::state::Language;

/// Local STT provider using whisper.cpp via whisper-rs.
///
/// Lazy-loads the WhisperContext on first transcription to avoid blocking
/// startup. The context is cached and reused across calls. Calling
/// `set_model_path` invalidates the cache so a new model is loaded next time.
pub struct LocalSttProvider {
    context: Arc<Mutex<Option<WhisperContext>>>,
    model_path: Arc<Mutex<PathBuf>>,
    language: Arc<Mutex<Language>>,
}

impl LocalSttProvider {
    /// Create a new local STT provider.
    ///
    /// The WhisperContext is not loaded until the first call to `transcribe()`.
    pub fn new(model_path: PathBuf, language: Language) -> Self {
        Self {
            context: Arc::new(Mutex::new(None)),
            model_path: Arc::new(Mutex::new(model_path)),
            language: Arc::new(Mutex::new(language)),
        }
    }

    /// Update the model path, invalidating any cached context.
    ///
    /// The new model will be loaded on the next `transcribe()` call.
    pub fn set_model_path(&self, path: PathBuf) {
        let mut model_path = self.model_path.lock().expect("model_path lock poisoned");
        *model_path = path;
        // Invalidate cached context so next transcribe() reloads
        let mut ctx = self.context.lock().expect("context lock poisoned");
        *ctx = None;
    }

    /// Update the language for transcription.
    pub fn set_language(&self, language: Language) {
        let mut lang = self.language.lock().expect("language lock poisoned");
        *lang = language;
    }

    /// Ensure the WhisperContext is loaded, returning a clone of the Arc.
    fn ensure_context_loaded(&self) -> Result<Arc<Mutex<Option<WhisperContext>>>, AppError> {
        let mut ctx_guard = self.context.lock().expect("context lock poisoned");
        if ctx_guard.is_none() {
            let path = self.model_path.lock().expect("model_path lock poisoned");
            let path_str = path.to_string_lossy().to_string();

            if !path.exists() {
                return Err(AppError::ModelNotDownloaded(path_str));
            }

            let params = WhisperContextParameters::default();
            let whisper_ctx = WhisperContext::new_with_params(&path_str, params)
                .map_err(|e| AppError::LocalTranscription(format!("failed to load model: {e}")))?;

            *ctx_guard = Some(whisper_ctx);
        }
        drop(ctx_guard);
        Ok(self.context.clone())
    }
}

impl SttProvider for LocalSttProvider {
    fn transcribe(
        &self,
        pcm_data: Vec<i16>,
        vocabulary_hint: Option<String>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, AppError>> + Send>> {
        // Eagerly load the context (fast if already cached)
        let ctx_arc = match self.ensure_context_loaded() {
            Ok(arc) => arc,
            Err(e) => return Box::pin(std::future::ready(Err(e))),
        };

        let language = self
            .language
            .lock()
            .expect("language lock poisoned")
            .clone();

        Box::pin(async move {
            // Convert i16 PCM to f32 samples (whisper.cpp expects f32)
            let f32_data: Vec<f32> = pcm_data.iter().map(|&s| s as f32 / 32768.0).collect();

            if f32_data.is_empty() {
                return Err(AppError::LocalTranscription("empty audio data".to_string()));
            }

            // Run inference on a blocking thread since whisper.cpp is CPU-bound
            tokio::task::spawn_blocking(move || {
                let mut ctx_guard = ctx_arc.lock().expect("context lock poisoned");
                let whisper_ctx = ctx_guard.as_ref().ok_or_else(|| {
                    AppError::LocalTranscription("context was invalidated".to_string())
                })?;

                let mut state = whisper_ctx.create_state().map_err(|e| {
                    AppError::LocalTranscription(format!("failed to create state: {e}"))
                })?;

                let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

                // Set language
                match language.code() {
                    Some(code) => params.set_language(Some(code)),
                    None => {
                        params.set_language(None);
                        params.set_detect_language(true);
                    }
                }

                // Set prompt hint (vocabulary or language default)
                let prompt = vocabulary_hint.unwrap_or_else(|| language.prompt().to_string());
                params.set_initial_prompt(&prompt);

                // Suppress console output
                params.set_print_progress(false);
                params.set_print_realtime(false);
                params.set_print_special(false);
                params.set_print_timestamps(false);

                // Run full transcription
                state
                    .full(params, &f32_data)
                    .map_err(|e| AppError::LocalTranscription(format!("inference failed: {e}")))?;

                // Collect all segments into final text
                let n_segments = state.full_n_segments().map_err(|e| {
                    AppError::LocalTranscription(format!("failed to get segments: {e}"))
                })?;

                let mut text = String::new();
                for i in 0..n_segments {
                    let segment_text = state.full_get_segment_text_lossy(i).map_err(|e| {
                        AppError::LocalTranscription(format!("failed to get segment {i} text: {e}"))
                    })?;
                    text.push_str(&segment_text);
                }

                Ok(text.trim().to_string())
            })
            .await
            .map_err(|e| AppError::LocalTranscription(format!("task join error: {e}")))?
        })
    }
}
