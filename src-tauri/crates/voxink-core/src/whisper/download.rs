use std::path::{Path, PathBuf};

use futures_util::StreamExt;

use crate::error::AppError;

/// Download a whisper model file with streaming progress reporting.
///
/// Writes to `{filename}.part` during download, then renames to `{filename}`
/// on completion to ensure atomicity. Calls `on_progress(downloaded_bytes, total_bytes)`
/// after each received chunk so the UI can display a progress bar.
///
/// Creates `models_dir` if it doesn't exist.
pub async fn download_model<F>(
    url: &str,
    filename: &str,
    models_dir: &Path,
    on_progress: F,
) -> Result<PathBuf, AppError>
where
    F: Fn(u64, u64) + Send + 'static,
{
    // Ensure the models directory exists
    tokio::fs::create_dir_all(models_dir)
        .await
        .map_err(|e| AppError::ModelDownload(format!("failed to create models dir: {e}")))?;

    let final_path = models_dir.join(filename);
    let part_path = models_dir.join(format!("{filename}.part"));

    let response = reqwest::get(url).await.map_err(|e| {
        AppError::ModelDownload(format!("failed to start download: {e}"))
    })?;

    let status = response.status();
    if !status.is_success() {
        return Err(AppError::ModelDownload(format!(
            "HTTP {}: download failed",
            status.as_u16()
        )));
    }

    let total_bytes = response.content_length().unwrap_or(0);

    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(&part_path).await.map_err(|e| {
        AppError::ModelDownload(format!("failed to create part file: {e}"))
    })?;

    let mut downloaded: u64 = 0;

    use tokio::io::AsyncWriteExt;

    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| {
            AppError::ModelDownload(format!("download stream error: {e}"))
        })?;

        file.write_all(&chunk).await.map_err(|e| {
            AppError::ModelDownload(format!("failed to write chunk: {e}"))
        })?;

        downloaded += chunk.len() as u64;
        on_progress(downloaded, total_bytes);
    }

    file.flush().await.map_err(|e| {
        AppError::ModelDownload(format!("failed to flush file: {e}"))
    })?;
    drop(file);

    // Atomic rename from .part to final filename
    tokio::fs::rename(&part_path, &final_path).await.map_err(|e| {
        AppError::ModelDownload(format!("failed to rename part file: {e}"))
    })?;

    Ok(final_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn should_download_file_with_progress() {
        let server = MockServer::start().await;
        let body = vec![0u8; 1024];

        Mock::given(method("GET"))
            .and(path("/model.bin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(body.clone())
                    .insert_header("content-length", "1024"),
            )
            .mount(&server)
            .await;

        let tmp = TempDir::new().unwrap();
        let progress_count = Arc::new(AtomicU64::new(0));
        let progress_count_clone = progress_count.clone();

        let result = download_model(
            &format!("{}/model.bin", server.uri()),
            "model.bin",
            tmp.path(),
            move |downloaded, total| {
                assert!(downloaded <= total || total == 0);
                progress_count_clone.fetch_add(1, Ordering::SeqCst);
            },
        )
        .await;

        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap(), "model.bin");

        let content = std::fs::read(&path).unwrap();
        assert_eq!(content.len(), 1024);

        // Progress callback should have been called at least once
        assert!(progress_count.load(Ordering::SeqCst) > 0);

        // .part file should be cleaned up
        let part_path = tmp.path().join("model.bin.part");
        assert!(!part_path.exists());
    }

    #[tokio::test]
    async fn should_create_models_dir_if_not_exists() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/model.bin"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(vec![42u8; 10]),
            )
            .mount(&server)
            .await;

        let tmp = TempDir::new().unwrap();
        let nested_dir = tmp.path().join("nested").join("models");
        assert!(!nested_dir.exists());

        let result = download_model(
            &format!("{}/model.bin", server.uri()),
            "model.bin",
            &nested_dir,
            |_, _| {},
        )
        .await;

        assert!(result.is_ok());
        assert!(nested_dir.exists());
    }

    #[tokio::test]
    async fn should_return_error_on_http_failure() {
        let server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/model.bin"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let tmp = TempDir::new().unwrap();
        let result = download_model(
            &format!("{}/model.bin", server.uri()),
            "model.bin",
            tmp.path(),
            |_, _| {},
        )
        .await;

        match result {
            Err(AppError::ModelDownload(msg)) => {
                assert!(msg.contains("404"), "expected 404 in message: {msg}");
            }
            other => panic!("expected ModelDownload error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_return_error_on_connection_failure() {
        let tmp = TempDir::new().unwrap();
        let result = download_model(
            "http://127.0.0.1:1/model.bin",
            "model.bin",
            tmp.path(),
            |_, _| {},
        )
        .await;

        assert!(matches!(result, Err(AppError::ModelDownload(_))));
    }
}
