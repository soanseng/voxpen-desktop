use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A whisper.cpp GGML model available for download and local inference.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WhisperModel {
    /// Unique identifier (e.g. "quick", "balanced")
    pub id: &'static str,
    /// Human-readable quality tier label
    pub tier: &'static str,
    /// Direct download URL (HuggingFace)
    pub url: &'static str,
    /// Filename on disk
    pub filename: &'static str,
    /// Expected file size in bytes
    pub size_bytes: u64,
}

/// Download / readiness status of a local whisper model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status")]
pub enum ModelStatus {
    NotDownloaded,
    Downloading { progress: f32 },
    Ready { size_bytes: u64 },
}

/// Base URL for all whisper.cpp GGML model files on HuggingFace.
#[cfg(test)]
const HF_BASE: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

/// Catalog of available whisper GGML models, ordered by quality/size.
pub const MODEL_CATALOG: &[WhisperModel] = &[
    WhisperModel {
        id: "quick",
        tier: "Quick",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small-q5_1.bin",
        filename: "ggml-small-q5_1.bin",
        size_bytes: 190_000_000,
    },
    WhisperModel {
        id: "balanced",
        tier: "Balanced",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
        filename: "ggml-large-v3-turbo-q5_0.bin",
        size_bytes: 574_000_000,
    },
    WhisperModel {
        id: "quality",
        tier: "Quality",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q8_0.bin",
        filename: "ggml-large-v3-turbo-q8_0.bin",
        size_bytes: 874_000_000,
    },
    WhisperModel {
        id: "maximum",
        tier: "Maximum",
        url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-q5_0.bin",
        filename: "ggml-large-v3-q5_0.bin",
        size_bytes: 1_080_000_000,
    },
];

/// Look up a model by its unique id. Returns `None` if not found.
pub fn model_by_id(id: &str) -> Option<&'static WhisperModel> {
    MODEL_CATALOG.iter().find(|m| m.id == id)
}

/// The default model for first-time users: "balanced" (large-v3-turbo-q5_0).
pub fn default_local_model() -> &'static WhisperModel {
    model_by_id("balanced").expect("balanced model must exist in catalog")
}

/// Full path where a model file would be stored.
pub fn model_path(models_dir: &Path, filename: &str) -> PathBuf {
    models_dir.join(filename)
}

/// Check the download / readiness status of a model.
pub fn get_model_status(models_dir: &Path, model: &WhisperModel) -> ModelStatus {
    let path = model_path(models_dir, model.filename);
    match std::fs::metadata(&path) {
        Ok(meta) => ModelStatus::Ready {
            size_bytes: meta.len(),
        },
        Err(_) => {
            // Check for in-progress .part file
            let part_path = models_dir.join(format!("{}.part", model.filename));
            if part_path.exists() {
                let downloaded = std::fs::metadata(&part_path)
                    .map(|m| m.len())
                    .unwrap_or(0);
                let progress = if model.size_bytes > 0 {
                    downloaded as f32 / model.size_bytes as f32
                } else {
                    0.0
                };
                ModelStatus::Downloading { progress }
            } else {
                ModelStatus::NotDownloaded
            }
        }
    }
}

/// Delete a downloaded model file. Returns `Ok(())` if the file was deleted
/// or didn't exist. Also cleans up any `.part` file.
pub fn delete_model(models_dir: &Path, model: &WhisperModel) -> Result<(), std::io::Error> {
    let path = model_path(models_dir, model.filename);
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    let part_path = models_dir.join(format!("{}.part", model.filename));
    if part_path.exists() {
        std::fs::remove_file(&part_path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn should_have_four_models_in_catalog() {
        assert_eq!(MODEL_CATALOG.len(), 4);
    }

    #[test]
    fn should_find_model_by_id() {
        let model = model_by_id("quick").unwrap();
        assert_eq!(model.filename, "ggml-small-q5_1.bin");
    }

    #[test]
    fn should_return_none_for_unknown_model_id() {
        assert!(model_by_id("nonexistent").is_none());
    }

    #[test]
    fn should_return_balanced_as_default_model() {
        let model = default_local_model();
        assert_eq!(model.id, "balanced");
        assert_eq!(model.filename, "ggml-large-v3-turbo-q5_0.bin");
    }

    #[test]
    fn should_construct_model_path() {
        let dir = Path::new("/tmp/models");
        let path = model_path(dir, "ggml-small-q5_1.bin");
        assert_eq!(path, PathBuf::from("/tmp/models/ggml-small-q5_1.bin"));
    }

    #[test]
    fn should_report_not_downloaded_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let model = model_by_id("quick").unwrap();
        let status = get_model_status(tmp.path(), model);
        assert_eq!(status, ModelStatus::NotDownloaded);
    }

    #[test]
    fn should_report_ready_when_file_exists() {
        let tmp = TempDir::new().unwrap();
        let model = model_by_id("quick").unwrap();
        let path = model_path(tmp.path(), model.filename);
        std::fs::write(&path, b"fake model data").unwrap();

        let status = get_model_status(tmp.path(), model);
        match status {
            ModelStatus::Ready { size_bytes } => {
                assert_eq!(size_bytes, 15); // "fake model data".len()
            }
            other => panic!("expected Ready, got {:?}", other),
        }
    }

    #[test]
    fn should_report_downloading_when_part_file_exists() {
        let tmp = TempDir::new().unwrap();
        let model = model_by_id("quick").unwrap();
        let part_path = tmp.path().join(format!("{}.part", model.filename));
        std::fs::write(&part_path, vec![0u8; 95_000_000]).unwrap();

        let status = get_model_status(tmp.path(), model);
        match status {
            ModelStatus::Downloading { progress } => {
                assert!(progress > 0.4 && progress < 0.6, "progress was {}", progress);
            }
            other => panic!("expected Downloading, got {:?}", other),
        }
    }

    #[test]
    fn should_delete_model_file() {
        let tmp = TempDir::new().unwrap();
        let model = model_by_id("quick").unwrap();
        let path = model_path(tmp.path(), model.filename);
        std::fs::write(&path, b"data").unwrap();
        assert!(path.exists());

        delete_model(tmp.path(), model).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn should_delete_part_file_along_with_model() {
        let tmp = TempDir::new().unwrap();
        let model = model_by_id("quick").unwrap();
        let path = model_path(tmp.path(), model.filename);
        let part_path = tmp.path().join(format!("{}.part", model.filename));
        std::fs::write(&path, b"data").unwrap();
        std::fs::write(&part_path, b"partial").unwrap();

        delete_model(tmp.path(), model).unwrap();
        assert!(!path.exists());
        assert!(!part_path.exists());
    }

    #[test]
    fn should_succeed_when_deleting_nonexistent_model() {
        let tmp = TempDir::new().unwrap();
        let model = model_by_id("quick").unwrap();
        // No file to delete — should still succeed
        assert!(delete_model(tmp.path(), model).is_ok());
    }

    #[test]
    fn should_have_valid_urls_for_all_models() {
        for model in MODEL_CATALOG {
            assert!(
                model.url.starts_with(HF_BASE),
                "model {} URL does not start with HF base: {}",
                model.id,
                model.url
            );
            assert!(
                model.url.ends_with(model.filename),
                "model {} URL does not end with filename",
                model.id
            );
        }
    }

    #[test]
    fn should_have_unique_ids() {
        let mut ids: Vec<&str> = MODEL_CATALOG.iter().map(|m| m.id).collect();
        let len_before = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), len_before, "duplicate model IDs found");
    }

    #[test]
    fn should_have_unique_filenames() {
        let mut filenames: Vec<&str> = MODEL_CATALOG.iter().map(|m| m.filename).collect();
        let len_before = filenames.len();
        filenames.sort();
        filenames.dedup();
        assert_eq!(
            filenames.len(),
            len_before,
            "duplicate filenames found"
        );
    }

    #[test]
    fn should_serialize_model_status_not_downloaded() {
        let status = ModelStatus::NotDownloaded;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("NotDownloaded"));
    }

    #[test]
    fn should_serialize_model_status_ready() {
        let status = ModelStatus::Ready {
            size_bytes: 190_000_000,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Ready"));
        assert!(json.contains("190000000"));
    }

    #[test]
    fn should_serialize_model_status_downloading() {
        let status = ModelStatus::Downloading { progress: 0.5 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("Downloading"));
        assert!(json.contains("0.5"));
    }
}
