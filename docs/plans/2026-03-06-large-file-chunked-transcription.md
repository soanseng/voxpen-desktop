# Large File Chunked Transcription with SRT/TXT Export

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Remove the 25MB file size limit by auto-chunking large audio files, merging segment timestamps, and adding SRT/TXT export.

**Architecture:** The existing `chunk_wav()` in `audio/chunker.rs` splits WAV files >25MB. We extend `groq::transcribe_file` to return Whisper `verbose_json` segments (with timestamps), add a new `transcribe_file_chunked()` that chunks → transcribes each chunk → merges segments with time offset → returns unified segments. The command layer calls the chunked version for WAV files >25MB and falls through to single-call for smaller files or non-WAV formats. A new `srt` module formats segments into SRT. The frontend gains "Export SRT" and "Export TXT" buttons.

**Tech Stack:** Rust (serde, reqwest, hound), React + TypeScript, Tauri IPC

---

## Task 1: Parse Whisper `verbose_json` segments

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs`

**Step 1: Write the failing test for segment deserialization**

In `src-tauri/crates/voxpen-core/src/api/groq.rs`, add to the `tests` module:

```rust
#[test]
fn should_deserialize_verbose_json_segments() {
    let json = serde_json::json!({
        "text": "Hello world. How are you?",
        "segments": [
            {
                "id": 0,
                "start": 0.0,
                "end": 1.5,
                "text": "Hello world."
            },
            {
                "id": 1,
                "start": 1.5,
                "end": 3.0,
                "text": " How are you?"
            }
        ]
    });
    let resp: WhisperVerboseResponse = serde_json::from_value(json).unwrap();
    assert_eq!(resp.text, "Hello world. How are you?");
    assert_eq!(resp.segments.len(), 2);
    assert_eq!(resp.segments[0].start, 0.0);
    assert_eq!(resp.segments[0].end, 1.5);
    assert_eq!(resp.segments[0].text, "Hello world.");
    assert_eq!(resp.segments[1].start, 1.5);
    assert_eq!(resp.segments[1].text, " How are you?");
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p voxpen-core should_deserialize_verbose_json_segments`
Expected: FAIL — `WhisperVerboseResponse` does not exist.

**Step 3: Add the segment types**

In `src-tauri/crates/voxpen-core/src/api/groq.rs`, add above `WhisperResponse`:

```rust
/// A single segment from Whisper's verbose_json response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhisperSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Full Whisper verbose_json response with segments.
#[derive(Debug, Deserialize)]
pub struct WhisperVerboseResponse {
    pub text: String,
    #[serde(default)]
    pub segments: Vec<WhisperSegment>,
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p voxpen-core should_deserialize_verbose_json_segments`
Expected: PASS

**Step 5: Commit**

```
feat(stt): add WhisperVerboseResponse with segment timestamps
```

---

## Task 2: Add `transcribe_file_with_segments()` API function

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn should_return_segments_from_transcribe_file_with_segments() {
    let server = MockServer::start().await;

    let verbose_response = serde_json::json!({
        "text": "Hello world.",
        "segments": [
            {"id": 0, "start": 0.0, "end": 1.5, "text": "Hello world."}
        ]
    });

    Mock::given(method("POST"))
        .and(path("/openai/v1/audio/transcriptions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(verbose_response))
        .expect(1)
        .mount(&server)
        .await;

    let config = test_config("key", Language::Auto);
    let fake_data = vec![0u8; 100];

    let result = transcribe_file_with_segments_internal(
        &config, &fake_data, "test.wav", "audio/wav", "groq",
        &format!("{}/", server.uri()),
    ).await.unwrap();

    assert_eq!(result.text, "Hello world.");
    assert_eq!(result.segments.len(), 1);
    assert_eq!(result.segments[0].start, 0.0);
    assert_eq!(result.segments[0].end, 1.5);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p voxpen-core should_return_segments_from_transcribe_file_with_segments`
Expected: FAIL — function does not exist.

**Step 3: Implement the function**

Add a new function in `groq.rs` that is almost identical to `transcribe_file_with_base_url` but deserializes into `WhisperVerboseResponse` instead of `WhisperResponse`:

```rust
/// Transcribe a file and return full verbose_json response with segments.
pub async fn transcribe_file_with_segments(
    config: &SttConfig,
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
    provider: &str,
) -> Result<WhisperVerboseResponse, AppError> {
    let base_url = base_url_for_provider(provider);
    transcribe_file_with_segments_internal(config, file_data, filename, mime_type, provider, base_url).await
}

/// Internal: with configurable base URL for testing.
pub(crate) async fn transcribe_file_with_segments_internal(
    config: &SttConfig,
    file_data: &[u8],
    filename: &str,
    mime_type: &str,
    provider: &str,
    base_url: &str,
) -> Result<WhisperVerboseResponse, AppError> {
    let client = reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(AppError::Network)?;

    let file_part = multipart::Part::bytes(file_data.to_vec())
        .file_name(filename.to_string())
        .mime_str(mime_type)
        .map_err(|e| AppError::Transcription(e.to_string()))?;

    let mut form = multipart::Form::new()
        .part("file", file_part)
        .text("model", config.model.clone())
        .text("response_format", "verbose_json".to_string());

    if let Some(code) = config.language.code() {
        form = form.text("language", code.to_string());
    }

    let prompt = config.prompt_override.as_deref().unwrap_or(config.language.prompt());
    form = form.text("prompt", prompt.to_string());

    let path = if provider == "groq" {
        "openai/v1/audio/transcriptions"
    } else {
        "v1/audio/transcriptions"
    };
    let url = format!("{base_url}{path}");

    let response = client.post(&url).bearer_auth(&config.api_key).multipart(form).send().await?;
    let status = response.status();

    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(AppError::ApiKeyMissing(provider.to_string()));
    }
    if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
        return Err(AppError::Transcription("file too large (max 25MB)".to_string()));
    }
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(AppError::Transcription(format!("HTTP {}: {}", status.as_u16(), body)));
    }

    let verbose: WhisperVerboseResponse = response
        .json()
        .await
        .map_err(|e| AppError::Transcription(format!("failed to parse response: {e}")))?;

    Ok(verbose)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p voxpen-core should_return_segments_from_transcribe_file_with_segments`
Expected: PASS

**Step 5: Commit**

```
feat(stt): add transcribe_file_with_segments for verbose_json parsing
```

---

## Task 3: Add SRT formatter module

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/srt.rs`
- Modify: `src-tauri/crates/voxpen-core/src/lib.rs` (add `pub mod srt;`)

**Step 1: Write the failing test**

Create `src-tauri/crates/voxpen-core/src/srt.rs`:

```rust
use crate::api::groq::WhisperSegment;

/// Format segments into SRT subtitle format.
pub fn format_srt(segments: &[WhisperSegment]) -> String {
    todo!()
}

/// Format seconds into SRT timestamp: HH:MM:SS,mmm
fn format_timestamp(seconds: f64) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_format_timestamp_correctly() {
        assert_eq!(format_timestamp(0.0), "00:00:00,000");
        assert_eq!(format_timestamp(1.5), "00:00:01,500");
        assert_eq!(format_timestamp(61.123), "00:01:01,123");
        assert_eq!(format_timestamp(3661.5), "01:01:01,500");
    }

    #[test]
    fn should_format_single_segment_srt() {
        let segments = vec![WhisperSegment {
            start: 0.0,
            end: 1.5,
            text: "Hello world.".to_string(),
        }];
        let srt = format_srt(&segments);
        assert_eq!(srt, "1\n00:00:00,000 --> 00:00:01,500\nHello world.\n");
    }

    #[test]
    fn should_format_multiple_segments_srt() {
        let segments = vec![
            WhisperSegment { start: 0.0, end: 1.5, text: "Hello.".to_string() },
            WhisperSegment { start: 1.5, end: 3.0, text: "World.".to_string() },
        ];
        let srt = format_srt(&segments);
        let expected = "1\n00:00:00,000 --> 00:00:01,500\nHello.\n\n2\n00:00:01,500 --> 00:00:03,000\nWorld.\n";
        assert_eq!(srt, expected);
    }

    #[test]
    fn should_return_empty_string_for_no_segments() {
        assert_eq!(format_srt(&[]), "");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p voxpen-core srt::tests`
Expected: FAIL — `todo!()` panics.

**Step 3: Implement `format_timestamp` and `format_srt`**

```rust
fn format_timestamp(seconds: f64) -> String {
    let total_ms = (seconds * 1000.0).round() as u64;
    let ms = total_ms % 1000;
    let total_secs = total_ms / 1000;
    let s = total_secs % 60;
    let m = (total_secs / 60) % 60;
    let h = total_secs / 3600;
    format!("{h:02}:{m:02}:{s:02},{ms:03}")
}

pub fn format_srt(segments: &[WhisperSegment]) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&format!(
            "{}\n{} --> {}\n{}\n",
            i + 1,
            format_timestamp(seg.start),
            format_timestamp(seg.end),
            seg.text.trim(),
        ));
    }
    out
}
```

**Step 4: Add `pub mod srt;` to `lib.rs`**

In `src-tauri/crates/voxpen-core/src/lib.rs`, add:
```rust
pub mod srt;
```

**Step 5: Run tests to verify they pass**

Run: `cargo test -p voxpen-core srt::tests`
Expected: PASS (all 4 tests)

**Step 6: Commit**

```
feat(srt): add SRT subtitle formatter module
```

---

## Task 4: Chunked transcription with segment merging

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/pipeline/chunked_transcribe.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs` (add `pub mod chunked_transcribe;`)

This module handles: read WAV → chunk via `audio::chunker` → transcribe each chunk with segments → offset timestamps → merge.

**Step 1: Write the failing test**

Create `src-tauri/crates/voxpen-core/src/pipeline/chunked_transcribe.rs`:

```rust
use crate::api::groq::{SttConfig, WhisperSegment, WhisperVerboseResponse};
use crate::audio::chunker;
use crate::error::AppError;

/// Result of a chunked transcription: full text + all segments with correct timestamps.
#[derive(Debug, Clone)]
pub struct ChunkedTranscriptionResult {
    pub text: String,
    pub segments: Vec<WhisperSegment>,
}

/// Merge segments from multiple chunks, offsetting timestamps by each chunk's audio duration.
pub fn merge_segments(
    chunk_results: &[(WhisperVerboseResponse, f64)], // (response, chunk_duration_seconds)
) -> ChunkedTranscriptionResult {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_merge_two_chunks_with_time_offset() {
        let chunk1 = WhisperVerboseResponse {
            text: "Hello.".to_string(),
            segments: vec![WhisperSegment { start: 0.0, end: 1.0, text: "Hello.".to_string() }],
        };
        let chunk2 = WhisperVerboseResponse {
            text: "World.".to_string(),
            segments: vec![WhisperSegment { start: 0.0, end: 1.5, text: "World.".to_string() }],
        };

        // chunk1 duration = 10.0s, so chunk2 segments should be offset by 10.0
        let result = merge_segments(&[(chunk1, 10.0), (chunk2, 10.0)]);

        assert_eq!(result.text, "Hello. World.");
        assert_eq!(result.segments.len(), 2);
        assert_eq!(result.segments[0].start, 0.0);
        assert_eq!(result.segments[0].end, 1.0);
        assert_eq!(result.segments[1].start, 10.0);
        assert_eq!(result.segments[1].end, 11.5);
    }

    #[test]
    fn should_handle_single_chunk() {
        let chunk = WhisperVerboseResponse {
            text: "Hello.".to_string(),
            segments: vec![WhisperSegment { start: 0.0, end: 1.0, text: "Hello.".to_string() }],
        };
        let result = merge_segments(&[(chunk, 5.0)]);
        assert_eq!(result.segments.len(), 1);
        assert_eq!(result.segments[0].start, 0.0);
    }

    #[test]
    fn should_handle_empty_chunks() {
        let result = merge_segments(&[]);
        assert_eq!(result.text, "");
        assert_eq!(result.segments.len(), 0);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p voxpen-core chunked_transcribe::tests`
Expected: FAIL — `todo!()`.

**Step 3: Implement `merge_segments`**

```rust
pub fn merge_segments(
    chunk_results: &[(WhisperVerboseResponse, f64)],
) -> ChunkedTranscriptionResult {
    let mut all_segments = Vec::new();
    let mut full_text = String::new();
    let mut time_offset = 0.0_f64;

    for (response, chunk_duration) in chunk_results {
        if !full_text.is_empty() && !response.text.is_empty() {
            full_text.push(' ');
        }
        full_text.push_str(response.text.trim());

        for seg in &response.segments {
            all_segments.push(WhisperSegment {
                start: seg.start + time_offset,
                end: seg.end + time_offset,
                text: seg.text.clone(),
            });
        }

        time_offset += chunk_duration;
    }

    ChunkedTranscriptionResult {
        text: full_text,
        segments: all_segments,
    }
}
```

**Step 4: Add `pub mod chunked_transcribe;` to pipeline/mod.rs**

**Step 5: Run tests to verify they pass**

Run: `cargo test -p voxpen-core chunked_transcribe::tests`
Expected: PASS

**Step 6: Commit**

```
feat(pipeline): add chunked transcription segment merger
```

---

## Task 5: Calculate WAV chunk duration from header

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/audio/chunker.rs`

We need to calculate each chunk's audio duration in seconds from the WAV header (sample rate, channels, bits per sample, data size).

**Step 1: Write the failing test**

Add to `chunker.rs` tests:

```rust
#[test]
fn should_calculate_wav_duration_correctly() {
    // 16000 Hz, mono, 16-bit = 32000 bytes/sec
    // 64000 bytes of PCM = 2.0 seconds
    let wav = make_wav(32000); // 32000 samples * 2 bytes = 64000 bytes PCM
    let duration = wav_duration_seconds(&wav).unwrap();
    assert!((duration - 2.0).abs() < 0.001);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p voxpen-core chunker::tests::should_calculate_wav_duration`
Expected: FAIL — function does not exist.

**Step 3: Implement `wav_duration_seconds`**

```rust
/// Calculate the duration of a WAV file in seconds from its header.
pub fn wav_duration_seconds(wav_data: &[u8]) -> Result<f64, AppError> {
    if wav_data.len() < WAV_HEADER_SIZE {
        return Err(AppError::Audio("WAV too short for duration calc".to_string()));
    }
    // Bytes 24-27: sample rate (u32 LE)
    let sample_rate = u32::from_le_bytes([wav_data[24], wav_data[25], wav_data[26], wav_data[27]]);
    // Bytes 34-35: bits per sample (u16 LE)
    let bits_per_sample = u16::from_le_bytes([wav_data[34], wav_data[35]]);
    // Bytes 22-23: num channels (u16 LE)
    let num_channels = u16::from_le_bytes([wav_data[22], wav_data[23]]);
    // Bytes 40-43: data size (u32 LE)
    let data_size = u32::from_le_bytes([wav_data[40], wav_data[41], wav_data[42], wav_data[43]]);

    let bytes_per_sample = (bits_per_sample as u32 / 8) * num_channels as u32;
    if sample_rate == 0 || bytes_per_sample == 0 {
        return Err(AppError::Audio("invalid WAV header values".to_string()));
    }
    let total_samples = data_size / bytes_per_sample;
    Ok(total_samples as f64 / sample_rate as f64)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p voxpen-core chunker::tests::should_calculate_wav_duration`
Expected: PASS

**Step 5: Commit**

```
feat(audio): add wav_duration_seconds for chunk time offset calculation
```

---

## Task 6: Update `transcribe_file` command to support chunking + segments

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/types/settings.ts`

**Step 1: Update `FileTranscriptionResult` to include segments and SRT**

In `src-tauri/src/commands.rs`, update:

```rust
use voxpen_core::api::groq::WhisperSegment;

#[derive(Debug, Clone, Serialize)]
pub struct FileTranscriptionResult {
    pub text: String,
    pub refined: Option<String>,
    pub srt: String,
}
```

In `src/types/settings.ts`, update:

```typescript
export interface FileTranscriptionResult {
  text: string;
  refined: string | null;
  srt: string;
}
```

**Step 2: Update `transcribe_file` to use chunking for large WAV files**

Replace the size check and transcription logic in `transcribe_file`:

```rust
    // For WAV files > 25MB, use chunked transcription
    // For other formats or smaller files, use single-call
    let (text, srt_content) = if ext == "wav" && file_data.len() > 25 * 1024 * 1024 {
        use voxpen_core::audio::chunker::{chunk_wav, wav_duration_seconds};
        use voxpen_core::api::groq::transcribe_file_with_segments;
        use voxpen_core::pipeline::chunked_transcribe::merge_segments;
        use voxpen_core::srt::format_srt;

        let chunks = chunk_wav(&file_data).map_err(|e| e.to_string())?;
        let mut chunk_results = Vec::new();

        for chunk in &chunks {
            let duration = wav_duration_seconds(chunk).map_err(|e| e.to_string())?;
            let verbose = transcribe_file_with_segments(
                &config, chunk, "chunk.wav", "audio/wav", &stt_provider,
            ).await.map_err(|e| e.to_string())?;
            chunk_results.push((verbose, duration));
        }

        let merged = merge_segments(&chunk_results);
        let srt = format_srt(&merged.segments);
        (merged.text, srt)
    } else if file_data.len() > 25 * 1024 * 1024 {
        // Non-WAV files > 25MB cannot be chunked
        return Err("File too large (max 25 MB for non-WAV formats). WAV files can be larger.".to_string());
    } else {
        // Single-call: use segments API for SRT generation
        use voxpen_core::api::groq::transcribe_file_with_segments;
        use voxpen_core::srt::format_srt;

        let verbose = transcribe_file_with_segments(
            &config, &file_data, &filename, mime, &stt_provider,
        ).await.map_err(|e| e.to_string())?;
        let srt = format_srt(&verbose.segments);
        (verbose.text, srt)
    };
```

Then update the return to include `srt`:

```rust
    Ok(FileTranscriptionResult {
        text,
        refined,
        srt: srt_content,
    })
```

Remove the old `if file_data.len() > 25 * 1024 * 1024` size check (line 516-518) and the old `groq::transcribe_file` call (line 542-544).

**Step 3: Build and verify**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS

**Step 4: Commit**

```
feat(transcribe): support chunked transcription for large WAV files with SRT output
```

---

## Task 7: Add "Export SRT" and "Export TXT" buttons to frontend

**Files:**
- Modify: `src/components/Settings/FileTranscriptionSection.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add i18n keys**

In `src/locales/en.json`, update the `fileTranscribe` section to add:

```json
"exportSrt": "Export SRT",
"exportTxt": "Export TXT",
"exported": "Exported!",
"supported": "Supported: WAV, MP3, FLAC, M4A, OGG, WebM. WAV files can exceed 25 MB (auto-chunked)."
```

In `src/locales/zh-TW.json`, update:

```json
"exportSrt": "匯出 SRT",
"exportTxt": "匯出 TXT",
"exported": "已匯出！",
"supported": "支援格式：WAV、MP3、FLAC、M4A、OGG、WebM。WAV 檔案可超過 25 MB（自動分割）。"
```

**Step 2: Add export helper and buttons**

In `FileTranscriptionSection.tsx`, add a save helper using Tauri's `save` dialog, and add export buttons below the result display:

```tsx
import { save } from "@tauri-apps/plugin-dialog";
import { writeTextFile } from "@tauri-apps/plugin-fs";

// Add state:
const [exported, setExported] = useState<"srt" | "txt" | null>(null);

// Add handler:
async function handleExport(format: "srt" | "txt") {
  if (!result) return;
  const content = format === "srt" ? result.srt : (result.refined ?? result.text);
  const ext = format;
  const defaultName = selectedFile
    ? selectedFile.split(/[\\/]/).pop()?.replace(/\.[^.]+$/, `.${ext}`) ?? `transcription.${ext}`
    : `transcription.${ext}`;

  const filePath = await save({
    defaultPath: defaultName,
    filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
  });
  if (!filePath) return;

  await writeTextFile(filePath, content);
  setExported(format);
  setTimeout(() => setExported(null), 2000);
}
```

Add export buttons between the refined result and the "Transcribe Another" button:

```tsx
{/* Export buttons */}
<div className="flex gap-2">
  {result.srt && (
    <button
      type="button"
      onClick={() => handleExport("srt")}
      className="flex-1 rounded-lg bg-blue-50 px-4 py-2 text-sm font-medium text-blue-700 hover:bg-blue-100 dark:bg-blue-900/20 dark:text-blue-400 dark:hover:bg-blue-900/30"
    >
      {exported === "srt" ? t("fileTranscribe.exported") : t("fileTranscribe.exportSrt")}
    </button>
  )}
  <button
    type="button"
    onClick={() => handleExport("txt")}
    className="flex-1 rounded-lg bg-blue-50 px-4 py-2 text-sm font-medium text-blue-700 hover:bg-blue-100 dark:bg-blue-900/20 dark:text-blue-400 dark:hover:bg-blue-900/30"
  >
    {exported === "txt" ? t("fileTranscribe.exported") : t("fileTranscribe.exportTxt")}
  </button>
</div>
```

**Step 3: Build frontend**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: PASS

**Step 4: Commit**

```
feat(ui): add SRT and TXT export buttons to file transcription
```

---

## Task 8: Full build verification

**Step 1: Run all Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: All PASS

**Step 2: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: No warnings

**Step 3: Build full app**

Run: `cargo build --manifest-path src-tauri/Cargo.toml && pnpm build`
Expected: PASS

**Step 4: Final commit if any fixes were needed**

```
chore: fix clippy warnings and build issues
```
