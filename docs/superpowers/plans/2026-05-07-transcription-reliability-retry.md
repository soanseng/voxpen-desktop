# Transcription Reliability Retry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Windows push-to-talk and hands-free transcription fail visibly, retry transient STT failures, chunk long live recordings, and let users resend saved recordings from history.

**Architecture:** Keep the recording and API pipeline in Rust. Add small pure helpers around recording validation, live PCM chunking, STT retry/error normalization, and failed-recording persistence; expose one Tauri command for retrying a failed history item. React remains a thin UI layer that renders complete error text and calls the retry command.

**Tech Stack:** Tauri v2, Rust 2021, `tokio`, `reqwest`, `rusqlite`, `wiremock`, React 19, TypeScript, Tailwind.

---

## Planning Status

Implementation has been completed on branch `fix/transcription-reliability-retry`. The checklist below is retained as the execution plan and review trace.

## Problem Mapping

The user reports these symptoms:

- Push-to-talk often appears to do nothing.
- Hands-free is also unreliable, especially with Groq.
- Sometimes it looks like the app has microphone access but no API call starts.
- Long recordings may exceed practical STT limits and should be split.
- Short two-or-three-word recordings also fail.
- OpenAI errors show as `Transcription failed: HTTP...` but the overlay truncates the important part.
- Since recordings are already capturable, failed API calls should leave a resend path.
- Follow-up clarification: OpenAI Audio API must be fully supported as a first-class STT provider, not just shown in settings. If OpenAI transcription fails after the user speaks, the user must be able to see the complete failure and manually resend the saved recording.

Current code surfaces found in this branch:

- `src-tauri/src/hotkey.rs` silently drops recordings shorter than 8000 samples, which is 0.5s at 16 kHz.
- `src-tauri/src/hotkey.rs` silently drops recordings below `audio::is_silent()`.
- `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs` sends the whole live recording as one WAV.
- `src-tauri/crates/voxpen-core/src/api/groq.rs` has a 300s STT timeout but no retry/backoff and returns raw HTTP bodies that are too long for the overlay.
- `src-tauri/src/commands.rs::test_api_key()` currently only supports `"groq"` and rejects other STT providers, so OpenAI key testing is not complete even though the settings UI offers OpenAI as an STT option.
- Existing provider path selection in `api::groq::base_url_for_provider()` and `transcribe_with_base_url()` can target OpenAI's `/v1/audio/transcriptions`, but the plan must add explicit OpenAI tests so this does not regress or remain only incidentally supported.
- `src-tauri/src/hotkey.rs` only writes history after successful transcription.
- `src/components/Overlay.tsx` truncates error text with `max-w-[160px] truncate`.

## Explicit OpenAI Audio API Support Requirements

OpenAI support is complete only when all of these are true:

- Selecting `stt_provider = "openai"` uses `https://api.openai.com/v1/audio/transcriptions`.
- OpenAI STT models in settings are valid Audio API transcription models, currently `whisper-1` and `gpt-4o-transcribe`.
- `test_api_key("openai", key)` performs an OpenAI-compatible transcription smoke test instead of returning `unsupported provider`.
- Live push-to-talk and hands-free recordings use the same provider abstraction as Groq, including language, prompt, response format, retry, and full error normalization.
- File transcription and failed-recording retry also work with OpenAI.
- OpenAI failures are persisted as failed history rows with `provider = "openai"`, the complete bounded error message, and the saved WAV path.
- The overlay and history UI show the full readable OpenAI failure, including provider name, HTTP status, and bounded response body snippet.
- Manual resend uses the currently selected STT provider by default. If the user keeps OpenAI selected, retry resends to OpenAI; if they switch to Groq before retrying, retry uses Groq and updates the row provider on success.

## Current Diagnosis: Can Recordings Be Too Long?

Yes, but the more precise answer is:

- **Default live recordings are probably not failing only because of file size.** Current settings default `max_recording_secs` to 360 seconds. At 16 kHz mono i16 PCM, WAV data is about 32 KB/sec, so 6 minutes is roughly 11.5 MB plus a tiny header. That is below OpenAI's documented 25 MB Audio API file limit and below Groq's documented 25 MB free-tier direct upload limit.
- **Long recordings are still a real reliability risk.** The live hotkey path currently sends the whole recording as one STT request. Longer requests increase upload time, provider processing time, timeout risk, rate-limit exposure, and the chance that a transient Groq/OpenAI failure loses the whole utterance.
- **Short phrases failing are a separate bug class.** Groq documents a minimum file length of 0.01 seconds, while this app currently silently drops live recordings under 0.5 seconds and also silently drops low-energy audio. A two-or-three-word phrase should be provider-acceptable if the app actually sends it.
- **The plan should fix both directions.** It should lower/visible-handle too-short recordings, chunk long live recordings, retry transient provider failures, and preserve failed recordings for resend.

External docs checked on 2026-05-07:

- OpenAI Audio API FAQ says the maximum Audio API file size is 25 MB and links long-audio handling guidance: https://help.openai.com/en/articles/7031512-audio-api-faq
- Groq Speech-to-Text docs list max file size as 25 MB on free tier and 100 MB on dev tier, with minimum file length 0.01 seconds and minimum billed length 10 seconds: https://console.groq.com/docs/speech-to-text
- Groq API reference confirms the transcription endpoint and accepted audio file formats: https://console.groq.com/docs/api-reference

Practical implication for VoxPen:

```text
16 kHz mono i16 WAV
  1 second  ~= 32 KB
  1 minute  ~= 1.92 MB
  6 minutes ~= 11.5 MB
  13 min 39 sec ~= 25 MB
```

So the immediate reliability issue is less "all long recordings exceed file size" and more "one large request has poor failure isolation." Chunking still belongs in scope because it makes long recordings recoverable and keeps each provider call small.

## File Structure

- Modify `src-tauri/src/hotkey.rs`
  - Lower the short-recording threshold.
  - Replace silent early returns with visible `PipelineState::Error`.
  - Save the WAV before STT so failed attempts can be retried.

- Modify `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`
  - Split live PCM into 60-second chunks.
  - Transcribe each chunk sequentially.
  - Join successful chunk texts deterministically.

- Modify `src-tauri/crates/voxpen-core/src/api/groq.rs`
  - Retry retryable STT failures once.
  - Normalize provider/status/body errors.
  - Keep body snippets readable and bounded.

- Modify `src-tauri/crates/voxpen-core/src/history.rs`
  - Add `status`, `error_message`, and `audio_path` to `TranscriptionEntry`.
  - Add SQL constants for migration, insert, query, search, and update-after-retry.

- Modify `src-tauri/src/history.rs`
  - Run additive migrations for old history databases.
  - Read and write the new fields.
  - Add `update_after_retry()`.

- Create `src-tauri/src/recording_store.rs`
  - Save live WAV files under the Tauri app data directory.
  - Load/delete saved WAV files by path.

- Modify `src-tauri/src/lib.rs`
  - Register the new `recording_store` module.
  - Register the `retry_transcription` command.

- Modify `src-tauri/src/commands.rs`
  - Make `test_api_key` work for OpenAI as well as Groq.
  - Add `retry_transcription(id: String)`.
  - Reuse the current settings/provider/refinement/history pipeline.

- Modify `src/components/Settings/SttSection.tsx`
  - Verify OpenAI model choices stay limited to models supported by the Audio API.
  - Ensure provider switching resets invalid STT model selections to a valid provider default.

- Modify `src/types/history.ts`
  - Add the new history fields.

- Modify `src/lib/tauri.ts`
  - Add `retryTranscription(id)`.

- Modify `src/components/History/HistoryEntry.tsx`
  - Show failed entries with the full error.
  - Add a Retry button that calls `retryTranscription`.

- Modify `src/components/History/HistoryWindow.tsx`
  - Pass an `onRetry` callback and reload history after retry.

- Modify `src/components/Overlay.tsx`
  - Render multi-line error text instead of truncating it.

- Modify `src/locales/en.json` and `src/locales/zh-TW.json`
  - Add labels for retrying and failed recordings.

---

### Task 0: Make OpenAI STT a First-Class Provider

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`
- Modify: `src/components/Settings/SttSection.tsx`

- [ ] **Step 1: Write failing tests for OpenAI provider routing**

Add this test inside `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs` under the existing test module:

```rust
    #[tokio::test]
    async fn should_call_openai_audio_transcription_endpoint_when_provider_is_openai() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "openai text"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let pcm_data = vec![100i16, 200, 300, -100, -200];

        let result = transcribe_with_base_url(
            &pcm_data,
            &config,
            "openai",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "openai text");
    }
```

- [ ] **Step 2: Run the routing test**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_call_openai_audio_transcription_endpoint_when_provider_is_openai
```

Expected: PASS if endpoint routing already works. If it fails, fix routing before any UI work.

- [ ] **Step 3: Write failing tests for readable OpenAI errors**

Add this test inside `src-tauri/crates/voxpen-core/src/api/groq.rs` under its test module:

```rust
    #[tokio::test]
    async fn should_return_full_bounded_openai_audio_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_string(r#"{"error":{"message":"Invalid file format. Supported formats: flac, mp3, mp4, mpeg, mpga, m4a, ogg, wav, webm."}}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let result = transcribe_with_base_url(
            &config,
            &[1, 2, 3, 4],
            "openai",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::Transcription(message)) => {
                assert!(message.contains("openai HTTP 400"));
                assert!(message.contains("Invalid file format"));
                assert!(message.contains("Supported formats"));
            }
            other => panic!("expected OpenAI transcription error, got {:?}", other),
        }
    }
```

- [ ] **Step 4: Run the error test**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_return_full_bounded_openai_audio_error
```

Expected: FAIL until Task 3's provider/status/body error normalization is implemented.

- [ ] **Step 5: Update `test_api_key` to support OpenAI**

In `src-tauri/src/commands.rs::test_api_key()`, replace the Groq-only guard:

```rust
    if provider != "groq" {
        return Err(format!("unsupported provider: {provider}"));
    }
```

with provider validation that accepts both `"groq"` and `"openai"`:

```rust
    if provider != "groq" && provider != "openai" {
        return Err(format!("unsupported provider: {provider}"));
    }
```

Build the `SttConfig` with a provider-specific default model:

```rust
    let model = match provider.as_str() {
        "openai" => "whisper-1",
        _ => voxpen_core::api::groq::DEFAULT_STT_MODEL,
    };
```

Then call the generic provider path:

```rust
    match groq::transcribe_file(&config, &wav_data, "test.wav", "audio/wav", &provider).await {
        Ok(_) => Ok(true),
        Err(voxpen_core::error::AppError::ApiKeyMissing(_)) => Ok(false),
        Err(_) => Ok(true),
    }
```

- [ ] **Step 6: Verify OpenAI model options in settings**

In `src/components/Settings/SttSection.tsx`, confirm `getModelsForProvider("openai")` returns only:

```tsx
[
  { value: "whisper-1", label: "whisper-1" },
  { value: "gpt-4o-transcribe", label: "gpt-4o-transcribe" },
]
```

If provider switching can leave a Groq model selected under OpenAI, update the provider change handler:

```tsx
function handleProviderChange(provider: string) {
  const models = getModelsForProvider(provider);
  onUpdate("stt_provider", provider);
  if (models.length > 0 && !models.some((m) => m.value === settings.stt_model)) {
    onUpdate("stt_model", models[0].value);
  }
}
```

- [ ] **Step 7: Run focused verification**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_call_openai_audio_transcription_endpoint_when_provider_is_openai
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_return_full_bounded_openai_audio_error
pnpm build
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/crates/voxpen-core/src/api/groq.rs src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs src/components/Settings/SttSection.tsx
git commit -m "fix: fully support openai audio transcription"
```

---

### Task 1: Make Short and Silent Recordings Visible

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1: Write failing tests for recording rejection**

Add this test module at the end of `src-tauri/src/hotkey.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reject_empty_recording_with_message() {
        assert_eq!(
            recording_rejection_message(&[]),
            Some("Recording was too short. Hold the hotkey a little longer and try again.")
        );
    }

    #[test]
    fn should_reject_recording_shorter_than_250ms() {
        let pcm = vec![500i16; MIN_RECORDING_SAMPLES - 1];

        assert_eq!(
            recording_rejection_message(&pcm),
            Some("Recording was too short. Hold the hotkey a little longer and try again.")
        );
    }

    #[test]
    fn should_accept_250ms_non_silent_recording() {
        let pcm = vec![500i16; MIN_RECORDING_SAMPLES];

        assert_eq!(recording_rejection_message(&pcm), None);
    }

    #[test]
    fn should_reject_silent_recording_with_message() {
        let pcm = vec![0i16; 16_000];

        assert_eq!(
            recording_rejection_message(&pcm),
            Some("No speech detected. Check the selected microphone and try again.")
        );
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml should_reject_empty_recording_with_message
```

Expected: FAIL with `cannot find function recording_rejection_message` or `cannot find value MIN_RECORDING_SAMPLES`.

- [ ] **Step 3: Add the recording rejection helper**

Add these constants and helper above `do_stop_recording()` in `src-tauri/src/hotkey.rs`:

```rust
/// Minimum live recording length accepted for STT.
///
/// 250 ms is short enough for two-or-three-word utterances while still filtering
/// accidental key taps that produce unusable audio.
const MIN_RECORDING_SAMPLES: usize = 4_000;

const SHORT_RECORDING_MESSAGE: &str =
    "Recording was too short. Hold the hotkey a little longer and try again.";
const SILENT_RECORDING_MESSAGE: &str =
    "No speech detected. Check the selected microphone and try again.";

fn recording_rejection_message(pcm_data: &[i16]) -> Option<&'static str> {
    if pcm_data.len() < MIN_RECORDING_SAMPLES {
        return Some(SHORT_RECORDING_MESSAGE);
    }

    if voxpen_core::audio::is_silent(pcm_data) {
        return Some(SILENT_RECORDING_MESSAGE);
    }

    None
}
```

- [ ] **Step 4: Replace silent early returns in `do_stop_recording()`**

Replace the existing short-recording and silence checks in `src-tauri/src/hotkey.rs`:

```rust
    // Skip very short recordings (<0.5s at 16kHz)
    if pcm_len < 8000 {
        let ctrl = controller.lock().await;
        ctrl.reset();
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    // Skip silent recordings — prevents Whisper hallucinations when
    // the user presses the hotkey but doesn't speak.
    if voxpen_core::audio::is_silent(&pcm_data) {
        let ctrl = controller.lock().await;
        ctrl.reset();
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }
```

with:

```rust
    if let Some(message) = recording_rejection_message(&pcm_data) {
        let ctrl = controller.lock().await;
        ctrl.emit_error(message.to_string());
        drop(ctrl);
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }
```

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml recording_rejection_message
```

Expected: PASS for the four new tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "fix: show feedback for unusable recordings"
```

---

### Task 2: Add Live PCM Chunking Before STT

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`

- [ ] **Step 1: Write failing unit tests for live chunking**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`:

```rust
    #[test]
    fn should_keep_short_live_recording_as_one_chunk() {
        let pcm = vec![100i16; 16_000 * 10];

        let chunks = split_pcm_for_stt(&pcm);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], pcm.as_slice());
    }

    #[test]
    fn should_split_live_recording_into_60_second_chunks() {
        let pcm = vec![100i16; LIVE_STT_CHUNK_SAMPLES * 2 + 123];

        let chunks = split_pcm_for_stt(&pcm);

        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].len(), LIVE_STT_CHUNK_SAMPLES);
        assert_eq!(chunks[1].len(), LIVE_STT_CHUNK_SAMPLES);
        assert_eq!(chunks[2].len(), 123);
    }

    #[tokio::test]
    async fn should_transcribe_each_live_chunk_and_join_text() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "first"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path("/openai/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "second"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let pcm_data = vec![100i16; LIVE_STT_CHUNK_SAMPLES + 10];

        let result = transcribe_with_base_url(
            &pcm_data,
            &config,
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "first second");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml split_live_recording -- --nocapture
```

Expected: FAIL with missing `split_pcm_for_stt` or `LIVE_STT_CHUNK_SAMPLES`.

- [ ] **Step 3: Add chunk constants and helper**

Add this near the top of `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`:

```rust
/// Live hotkey recordings are chunked by duration, not only provider file size.
/// This avoids provider stalls on long uploads and keeps retry scope bounded.
const LIVE_STT_CHUNK_SECONDS: usize = 60;
const LIVE_STT_CHUNK_SAMPLES: usize =
    crate::audio::encoder::SAMPLE_RATE as usize * LIVE_STT_CHUNK_SECONDS;

fn split_pcm_for_stt(pcm_data: &[i16]) -> Vec<&[i16]> {
    if pcm_data.len() <= LIVE_STT_CHUNK_SAMPLES {
        return vec![pcm_data];
    }

    pcm_data.chunks(LIVE_STT_CHUNK_SAMPLES).collect()
}

fn join_chunk_texts(texts: Vec<String>) -> String {
    texts
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}
```

- [ ] **Step 4: Chunk inside `transcribe()`**

Replace the bottom of `transcribe()` in `src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs`:

```rust
    let wav_data = encoder::pcm_to_wav(pcm_data);
    groq::transcribe_with_base_url(&config, &wav_data, provider, base_url).await
```

with:

```rust
    let chunks = split_pcm_for_stt(pcm_data);
    let mut texts = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        let wav_data = encoder::pcm_to_wav(chunk);
        let text = groq::transcribe_with_base_url(&config, &wav_data, provider, base_url).await?;
        texts.push(text);
    }

    Ok(join_chunk_texts(texts))
```

Also replace the bottom of the test-only `transcribe_with_base_url()` helper with the same loop so tests exercise the production chunking behavior.

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml live_recording
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_transcribe_each_live_chunk_and_join_text
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/transcribe.rs
git commit -m "fix: chunk live recordings before transcription"
```

---

### Task 3: Retry Transient STT Failures and Normalize Errors

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs`

- [ ] **Step 1: Write failing tests for retry and readable errors**

Add these tests inside the existing `#[cfg(test)] mod tests` in `src-tauri/crates/voxpen-core/src/api/groq.rs`. If the file has no test module, add one at the end.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn should_retry_once_on_rate_limit_then_succeed() {
        let server = MockServer::start().await;
        let path_matcher = path("/openai/v1/audio/transcriptions");

        Mock::given(method("POST"))
            .and(path_matcher.clone())
            .respond_with(ResponseTemplate::new(429).set_body_string("rate limit"))
            .expect(1)
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path_matcher)
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"text": "retried text"})),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let result = transcribe_with_base_url(
            &config,
            &[1, 2, 3, 4],
            "groq",
            &format!("{}/", server.uri()),
        )
        .await;

        assert_eq!(result.unwrap(), "retried text");
    }

    #[tokio::test]
    async fn should_include_provider_status_and_body_snippet_in_http_error() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/audio/transcriptions"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_string(r#"{"error":{"message":"invalid model for audio"}}"#),
            )
            .expect(1)
            .mount(&server)
            .await;

        let config = SttConfig::new("test-key".to_string(), Language::English);
        let result = transcribe_with_base_url(
            &config,
            &[1, 2, 3, 4],
            "openai",
            &format!("{}/", server.uri()),
        )
        .await;

        match result {
            Err(AppError::Transcription(message)) => {
                assert!(message.contains("openai HTTP 400"));
                assert!(message.contains("invalid model for audio"));
            }
            other => panic!("expected transcription error, got {:?}", other),
        }
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_retry_once_on_rate_limit_then_succeed
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_include_provider_status_and_body_snippet_in_http_error
```

Expected: first test FAILS because status 429 is returned immediately; second test FAILS because the message is `HTTP 400: ...` without provider context.

- [ ] **Step 3: Add retry/error helpers**

Add these helpers in `src-tauri/crates/voxpen-core/src/api/groq.rs` above `transcribe_with_base_url()`:

```rust
const STT_MAX_ATTEMPTS: usize = 2;
const STT_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(400);
const HTTP_ERROR_BODY_LIMIT: usize = 1200;

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::REQUEST_TIMEOUT
        || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status.is_server_error()
}

fn truncate_body(body: &str) -> String {
    let compact = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= HTTP_ERROR_BODY_LIMIT {
        return compact;
    }

    compact
        .chars()
        .take(HTTP_ERROR_BODY_LIMIT)
        .collect::<String>()
        + "..."
}

fn transcription_http_error(provider: &str, status: reqwest::StatusCode, body: String) -> AppError {
    AppError::Transcription(format!(
        "{provider} HTTP {}: {}",
        status.as_u16(),
        truncate_body(&body)
    ))
}
```

- [ ] **Step 4: Wrap live STT send in a retry loop**

In `transcribe_with_base_url()`, replace the single `.send().await?` through HTTP error handling block with this loop:

```rust
    let mut last_error: Option<AppError> = None;

    for attempt in 1..=STT_MAX_ATTEMPTS {
        let file_part = multipart::Part::bytes(wav_data.to_vec())
            .file_name("recording.wav")
            .mime_str("audio/wav")
            .map_err(|e| AppError::Transcription(e.to_string()))?;

        let mut form = multipart::Form::new()
            .part("file", file_part)
            .text("model", config.model.clone())
            .text("response_format", config.response_format.clone());

        if let Some(code) = config.language.code() {
            form = form.text("language", code.to_string());
        }

        let prompt = config
            .prompt_override
            .as_deref()
            .unwrap_or(config.language.prompt());
        form = form.text("prompt", prompt.to_string());

        let response = match client
            .post(&url)
            .bearer_auth(&config.api_key)
            .multipart(form)
            .send()
            .await
        {
            Ok(response) => response,
            Err(e) => {
                let retryable = e.is_timeout() || e.is_connect();
                if retryable && attempt < STT_MAX_ATTEMPTS {
                    tokio::time::sleep(STT_RETRY_DELAY).await;
                    continue;
                }
                return Err(AppError::Network(e));
            }
        };

        let status = response.status();

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(AppError::ApiKeyMissing(provider.to_string()));
        }

        if status == reqwest::StatusCode::PAYLOAD_TOO_LARGE {
            return Err(AppError::Transcription(format!(
                "{provider} HTTP 413: file too large"
            )));
        }

        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            let err = transcription_http_error(provider, status, body);
            if is_retryable_status(status) && attempt < STT_MAX_ATTEMPTS {
                last_error = Some(err);
                tokio::time::sleep(STT_RETRY_DELAY).await;
                continue;
            }
            return Err(err);
        }

        let whisper: WhisperResponse = response
            .json()
            .await
            .map_err(|e| AppError::Transcription(format!("failed to parse response: {e}")))?;

        return Ok(whisper.text);
    }

    Err(last_error.unwrap_or_else(|| {
        AppError::Transcription(format!("{provider} transcription request did not complete"))
    }))
```

Keep the existing `client`, `path`, and `url` setup above the loop. Remove the original `file_part` and `form` construction before the loop so the multipart body is rebuilt per attempt.

- [ ] **Step 5: Apply the same HTTP error helper to file transcription**

In `transcribe_file_with_base_url()` and `transcribe_file_with_segments_internal()`, replace:

```rust
        return Err(AppError::Transcription(format!(
            "HTTP {}: {}",
            status.as_u16(),
            body
        )));
```

and:

```rust
        return Err(AppError::Transcription(format!("HTTP {}: {}", status.as_u16(), body)));
```

with:

```rust
        return Err(transcription_http_error(provider, status, body));
```

Also replace file-too-large messages with:

```rust
        return Err(AppError::Transcription(format!(
            "{provider} HTTP 413: file too large (max 25MB)"
        )));
```

- [ ] **Step 6: Run focused tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_retry_once_on_rate_limit_then_succeed
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_include_provider_status_and_body_snippet_in_http_error
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/api/groq.rs
git commit -m "fix: retry transient transcription failures"
```

---

### Task 4: Persist Failed Live Recordings in History

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/history.rs`
- Modify: `src-tauri/src/history.rs`
- Create: `src-tauri/src/recording_store.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1: Extend core history types with tests**

In `src-tauri/crates/voxpen-core/src/history.rs`, add this enum above `TranscriptionEntry`:

```rust
/// Status of a history row.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionStatus {
    Completed,
    Failed,
}

impl Default for TranscriptionStatus {
    fn default() -> Self {
        Self::Completed
    }
}
```

Extend `TranscriptionEntry`:

```rust
pub struct TranscriptionEntry {
    pub id: String,
    pub timestamp: i64,
    pub original_text: String,
    pub refined_text: Option<String>,
    pub language: Language,
    pub audio_duration_ms: u64,
    pub provider: String,
    #[serde(default)]
    pub status: TranscriptionStatus,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub audio_path: Option<String>,
}
```

Update `sample_entry()` in the same file:

```rust
    fn sample_entry(refined: Option<&str>) -> TranscriptionEntry {
        TranscriptionEntry {
            id: "abc-123".to_string(),
            timestamp: 1_700_000_000,
            original_text: "raw transcription".to_string(),
            refined_text: refined.map(String::from),
            language: Language::Chinese,
            audio_duration_ms: 5_000,
            provider: "groq".to_string(),
            status: TranscriptionStatus::Completed,
            error_message: None,
            audio_path: Some("/tmp/voxpen.wav".to_string()),
        }
    }
```

Add this test:

```rust
    #[test]
    fn should_serialize_failed_entry_with_error_and_audio_path() {
        let mut entry = sample_entry(None);
        entry.original_text = String::new();
        entry.status = TranscriptionStatus::Failed;
        entry.error_message = Some("groq HTTP 500: upstream failed".to_string());

        let json = serde_json::to_string(&entry).unwrap();

        assert!(json.contains(r#""status":"failed""#));
        assert!(json.contains(r#""error_message":"groq HTTP 500: upstream failed""#));
        assert!(json.contains(r#""audio_path":"/tmp/voxpen.wav""#));
    }
```

- [ ] **Step 2: Run the core history test**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml should_serialize_failed_entry_with_error_and_audio_path
```

Expected: PASS after Step 1, because it only changes pure serialization.

- [ ] **Step 3: Update SQL constants**

Replace the SQL constants in `src-tauri/crates/voxpen-core/src/history.rs` with:

```rust
pub const CREATE_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS transcriptions (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp INTEGER NOT NULL,
    original_text TEXT NOT NULL,
    refined_text TEXT,
    language TEXT NOT NULL,
    audio_duration_ms INTEGER NOT NULL,
    provider TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',
    error_message TEXT,
    audio_path TEXT
)";

pub const MIGRATION_SQL: &str = "\
ALTER TABLE transcriptions ADD COLUMN status TEXT NOT NULL DEFAULT 'completed';
ALTER TABLE transcriptions ADD COLUMN error_message TEXT;
ALTER TABLE transcriptions ADD COLUMN audio_path TEXT;";

pub const INSERT_SQL: &str = "\
INSERT INTO transcriptions (
    id, timestamp, original_text, refined_text, language, audio_duration_ms,
    provider, status, error_message, audio_path
)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

pub const QUERY_SQL: &str = "\
SELECT id, timestamp, original_text, refined_text, language, audio_duration_ms,
       provider, status, error_message, audio_path
FROM transcriptions ORDER BY timestamp DESC LIMIT ? OFFSET ?";

pub const SEARCH_SQL: &str = "\
SELECT id, timestamp, original_text, refined_text, language, audio_duration_ms,
       provider, status, error_message, audio_path
FROM transcriptions
WHERE original_text LIKE ? OR refined_text LIKE ? OR error_message LIKE ?
ORDER BY timestamp DESC LIMIT ? OFFSET ?";

pub const UPDATE_AFTER_RETRY_SQL: &str = "\
UPDATE transcriptions
SET original_text = ?, refined_text = ?, language = ?, audio_duration_ms = ?,
    provider = ?, status = 'completed', error_message = NULL
WHERE id = ?";
```

- [ ] **Step 4: Add additive migration logic**

In `src-tauri/src/history.rs`, add this helper below `open()`:

```rust
fn migrate_schema(conn: &Connection) -> Result<(), String> {
    let columns = conn
        .prepare("PRAGMA table_info(transcriptions)")
        .map_err(|e| format!("schema inspect prepare failed: {e}"))?
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| format!("schema inspect failed: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("schema column read failed: {e}"))?;

    if !columns.iter().any(|c| c == "status") {
        conn.execute(
            "ALTER TABLE transcriptions ADD COLUMN status TEXT NOT NULL DEFAULT 'completed'",
            [],
        )
        .map_err(|e| format!("status migration failed: {e}"))?;
    }
    if !columns.iter().any(|c| c == "error_message") {
        conn.execute("ALTER TABLE transcriptions ADD COLUMN error_message TEXT", [])
            .map_err(|e| format!("error_message migration failed: {e}"))?;
    }
    if !columns.iter().any(|c| c == "audio_path") {
        conn.execute("ALTER TABLE transcriptions ADD COLUMN audio_path TEXT", [])
            .map_err(|e| format!("audio_path migration failed: {e}"))?;
    }

    Ok(())
}
```

Then change `HistoryDb::open()` to call it:

```rust
        conn.execute_batch(CREATE_TABLE_SQL)
            .map_err(|e| format!("failed to create table: {e}"))?;
        migrate_schema(&conn)?;
```

- [ ] **Step 5: Update insert/search/row mapping**

In `src-tauri/src/history.rs`, update `insert()` params:

```rust
            rusqlite::params![
                entry.id,
                entry.timestamp,
                entry.original_text,
                entry.refined_text,
                serde_json::to_string(&entry.language).unwrap_or_default(),
                entry.audio_duration_ms,
                entry.provider,
                serde_json::to_string(&entry.status).unwrap_or_else(|_| "\"completed\"".to_string()).trim_matches('"').to_string(),
                entry.error_message,
                entry.audio_path,
            ],
```

Update `search()` params:

```rust
                rusqlite::params![&pattern, &pattern, &pattern, limit, offset],
```

Update `row_to_entry()`:

```rust
fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<TranscriptionEntry> {
    let status: String = row.get(7)?;
    Ok(TranscriptionEntry {
        id: row.get(0)?,
        timestamp: row.get(1)?,
        original_text: row.get(2)?,
        refined_text: row.get(3)?,
        language: {
            let s: String = row.get(4)?;
            serde_json::from_str(&s).unwrap_or(Language::Auto)
        },
        audio_duration_ms: row.get(5)?,
        provider: row.get(6)?,
        status: match status.as_str() {
            "failed" => voxpen_core::history::TranscriptionStatus::Failed,
            _ => voxpen_core::history::TranscriptionStatus::Completed,
        },
        error_message: row.get(8)?,
        audio_path: row.get(9)?,
    })
}
```

Add `update_after_retry()`:

```rust
    pub fn update_after_retry(&self, entry: &TranscriptionEntry) -> Result<(), String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            voxpen_core::history::UPDATE_AFTER_RETRY_SQL,
            rusqlite::params![
                entry.original_text,
                entry.refined_text,
                serde_json::to_string(&entry.language).unwrap_or_default(),
                entry.audio_duration_ms,
                entry.provider,
                entry.id,
            ],
        )
        .map_err(|e| format!("retry update failed: {e}"))?;
        Ok(())
    }
```

- [ ] **Step 6: Add recording store**

Create `src-tauri/src/recording_store.rs`:

```rust
use std::path::PathBuf;

use tauri::{AppHandle, Manager};
use voxpen_core::audio::encoder;
use voxpen_core::error::AppError;

pub fn save_live_recording(
    app: &AppHandle,
    id: &str,
    pcm_data: &[i16],
) -> Result<PathBuf, AppError> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Storage(format!("app data dir: {e}")))?
        .join("recordings");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Storage(format!("create recordings dir: {e}")))?;

    let path = dir.join(format!("{id}.wav"));
    let wav = encoder::pcm_to_wav(pcm_data);
    std::fs::write(&path, wav)
        .map_err(|e| AppError::Storage(format!("write recording: {e}")))?;
    Ok(path)
}

pub fn read_recording(path: &str) -> Result<Vec<u8>, AppError> {
    std::fs::read(path).map_err(|e| AppError::Storage(format!("read recording: {e}")))
}
```

Add `mod recording_store;` to `src-tauri/src/lib.rs`.

- [ ] **Step 7: Save failed live recordings**

In `src-tauri/src/hotkey.rs`, inside `do_stop_recording()`, create a history id and save audio before calling `ctrl.on_stop_recording()`:

```rust
    let entry_id = uuid::Uuid::new_v4().to_string();
    let audio_path = match crate::recording_store::save_live_recording(&app, &entry_id, &pcm_data) {
        Ok(path) => Some(path.to_string_lossy().to_string()),
        Err(e) => {
            eprintln!("recording save error: {e}");
            None
        }
    };
```

Use `entry_id.clone()` instead of creating a second UUID in the success entry:

```rust
            id: entry_id.clone(),
```

In the `else if let Err(ref e) = result` block, insert a failed history entry:

```rust
        let s = settings.lock().await;
        let entry = voxpen_core::history::TranscriptionEntry {
            id: entry_id,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            original_text: String::new(),
            refined_text: None,
            language: s.stt_language.clone(),
            audio_duration_ms: (pcm_len as u64 * 1000) / 16000,
            provider: s.stt_provider.clone(),
            status: voxpen_core::history::TranscriptionStatus::Failed,
            error_message: Some(e.to_string()),
            audio_path,
        };
        drop(s);
        if let Err(insert_err) = history.insert(&entry) {
            eprintln!("failed history insert error: {insert_err}");
        }
```

Also add these fields to the existing success entry:

```rust
            status: voxpen_core::history::TranscriptionStatus::Completed,
            error_message: None,
            audio_path,
```

- [ ] **Step 8: Run focused tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml history
cargo test --manifest-path src-tauri/Cargo.toml history
```

Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/history.rs src-tauri/src/history.rs src-tauri/src/recording_store.rs src-tauri/src/lib.rs src-tauri/src/hotkey.rs
git commit -m "feat: persist failed recordings for retry"
```

---

### Task 5: Add Retry Command for Failed History Rows

**Files:**
- Modify: `src-tauri/src/history.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `HistoryDb::get()`**

In `src-tauri/crates/voxpen-core/src/history.rs`, add:

```rust
pub const GET_BY_ID_SQL: &str = "\
SELECT id, timestamp, original_text, refined_text, language, audio_duration_ms,
       provider, status, error_message, audio_path
FROM transcriptions WHERE id = ?";
```

In `src-tauri/src/history.rs`, add:

```rust
    pub fn get(&self, id: &str) -> Result<Option<TranscriptionEntry>, String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn
            .prepare(voxpen_core::history::GET_BY_ID_SQL)
            .map_err(|e| format!("get prepare failed: {e}"))?;
        let mut rows = stmt
            .query(rusqlite::params![id])
            .map_err(|e| format!("get failed: {e}"))?;
        if let Some(row) = rows.next().map_err(|e| format!("get row failed: {e}"))? {
            Ok(Some(row_to_entry(row).map_err(|e| format!("row read failed: {e}"))?))
        } else {
            Ok(None)
        }
    }
```

- [ ] **Step 2: Add retry command**

In `src-tauri/src/commands.rs`, add this command near the history commands:

```rust
#[tauri::command]
pub async fn retry_transcription(
    id: String,
    state: tauri::State<'_, crate::state::AppState>,
    app: tauri::AppHandle,
) -> Result<TranscriptionEntry, String> {
    use voxpen_core::api::groq::{self, SttConfig};
    use voxpen_core::history::{TranscriptionEntry, TranscriptionStatus};

    let existing = state
        .history
        .get(&id)?
        .ok_or_else(|| "history entry not found".to_string())?;

    if existing.status != TranscriptionStatus::Failed {
        return Err("only failed transcriptions can be retried".to_string());
    }

    let audio_path = existing
        .audio_path
        .clone()
        .ok_or_else(|| "failed transcription has no saved recording".to_string())?;
    let wav_data = crate::recording_store::read_recording(&audio_path)
        .map_err(|e| e.to_string())?;

    let s = state.settings.lock().await;
    let provider = s.stt_provider.clone();
    let api_key = crate::state::get_api_key_from_handle(&app, &provider)
        .map_err(|e| e.to_string())?;
    let stt_config = SttConfig {
        api_key,
        model: s.stt_model.clone(),
        language: s.stt_language.clone(),
        response_format: "verbose_json".to_string(),
        prompt_override: None,
    };
    let refinement_enabled = s.refinement_enabled;
    let refinement_provider = s.refinement_provider.clone();
    let language = s.stt_language.clone();
    let custom_base_url = s.custom_base_url.clone();
    let tone_preset = s.tone_preset.clone();
    let custom_prompt = s.refinement_prompt.clone();
    let translation_target = if s.translation_enabled {
        Some(s.translation_target.clone())
    } else {
        None
    };
    drop(s);

    let raw_text = groq::transcribe_file(
        &stt_config,
        &wav_data,
        "recording.wav",
        "audio/wav",
        &provider,
    )
    .await
    .map_err(|e| e.to_string())?;

    let refined_text = if refinement_enabled {
        let llm_key = crate::state::get_api_key_from_handle(&app, &refinement_provider)
            .map_err(|e| e.to_string())?;
        let config = groq::ChatConfig {
            api_key: llm_key,
            model: {
                let s = state.settings.lock().await;
                s.refinement_model.clone()
            },
            temperature: groq::LLM_TEMPERATURE,
            max_tokens: groq::LLM_MAX_TOKENS,
        };
        match voxpen_core::pipeline::refine::refine(
            &raw_text,
            &config,
            &language,
            &[],
            &custom_prompt,
            &tone_preset,
            &refinement_provider,
            &custom_base_url,
            translation_target.as_ref(),
        )
        .await
        {
            Ok(text) => Some(text),
            Err(e) => {
                eprintln!("retry refinement failed, keeping raw transcription: {e}");
                None
            }
        }
    } else {
        None
    };

    let completed = TranscriptionEntry {
        id: existing.id,
        timestamp: existing.timestamp,
        original_text: raw_text,
        refined_text,
        language,
        audio_duration_ms: existing.audio_duration_ms,
        provider,
        status: TranscriptionStatus::Completed,
        error_message: None,
        audio_path: existing.audio_path,
    };

    state.history.update_after_retry(&completed)?;
    Ok(completed)
}
```

- [ ] **Step 3: Register the command**

In the `tauri::generate_handler![]` list in `src-tauri/src/lib.rs`, add:

```rust
            commands::retry_transcription,
```

- [ ] **Step 4: Run focused Rust checks**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml history
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/history.rs src-tauri/src/history.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: retry failed transcriptions"
```

---

### Task 6: Add Retry UI and Full Error Display

**Files:**
- Modify: `src/types/history.ts`
- Modify: `src/lib/tauri.ts`
- Modify: `src/components/History/HistoryEntry.tsx`
- Modify: `src/components/History/HistoryList.tsx`
- Modify: `src/components/History/HistoryWindow.tsx`
- Modify: `src/components/Overlay.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

- [ ] **Step 1: Extend TypeScript history type**

Replace `src/types/history.ts` with:

```ts
export type TranscriptionStatus = "completed" | "failed";

export interface TranscriptionEntry {
  id: string;
  timestamp: number;
  original_text: string;
  refined_text: string | null;
  language: string;
  audio_duration_ms: number;
  provider: string;
  status: TranscriptionStatus;
  error_message: string | null;
  audio_path: string | null;
}
```

- [ ] **Step 2: Add Tauri retry wrapper**

In `src/lib/tauri.ts`, add:

```ts
export async function retryTranscription(id: string): Promise<TranscriptionEntry> {
  return invoke<TranscriptionEntry>("retry_transcription", { id });
}
```

- [ ] **Step 3: Thread retry callback through history list**

In `src/components/History/HistoryList.tsx`, change props to:

```tsx
interface HistoryListProps {
  entries: TranscriptionEntry[];
  onDelete: (id: string) => void;
  onRetry: (id: string) => void;
}

export default function HistoryList({ entries, onDelete, onRetry }: HistoryListProps) {
```

Change the entry render to:

```tsx
        <HistoryEntry
          key={entry.id}
          entry={entry}
          onDelete={onDelete}
          onRetry={onRetry}
        />
```

In `src/components/History/HistoryWindow.tsx`, import `retryTranscription`:

```tsx
import { deleteHistoryEntry, getHistory, retryTranscription, searchHistory } from "../../lib/tauri";
```

Add:

```tsx
  async function handleRetry(id: string) {
    await retryTranscription(id);
    await loadEntries();
  }
```

Pass it:

```tsx
        <HistoryList entries={entries} onDelete={handleDelete} onRetry={handleRetry} />
```

- [ ] **Step 4: Render failed state and retry button**

In `src/components/History/HistoryEntry.tsx`, change props:

```tsx
interface HistoryEntryProps {
  entry: TranscriptionEntry;
  onDelete: (id: string) => void;
  onRetry: (id: string) => void;
}
```

Change component signature:

```tsx
export default function HistoryEntry({ entry, onDelete, onRetry }: HistoryEntryProps) {
```

Add state:

```tsx
  const [retrying, setRetrying] = useState(false);
```

Change display text:

```tsx
  const isFailed = entry.status === "failed";
  const displayText = isFailed
    ? (entry.error_message ?? t("historyFailed"))
    : (entry.refined_text ?? entry.original_text);
```

In the expanded body, before original/refined text, render failed details:

```tsx
          {isFailed && (
            <div className="mb-3 rounded border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/30 dark:text-red-300">
              <div className="font-medium">{t("historyFailed")}</div>
              <div className="mt-1 whitespace-pre-wrap break-words">
                {entry.error_message ?? t("error")}
              </div>
            </div>
          )}
```

Wrap the original/refined grid with:

```tsx
          {!isFailed && (
```

and close it after the refined block.

Add a retry button before the copy button:

```tsx
            {isFailed && (
              <button
                type="button"
                disabled={retrying}
                onClick={async (e) => {
                  e.stopPropagation();
                  setRetrying(true);
                  try {
                    await onRetry(entry.id);
                  } finally {
                    setRetrying(false);
                  }
                }}
                className="rounded px-2 py-1 text-xs font-medium text-blue-600 transition-colors hover:bg-blue-50 disabled:opacity-50 dark:text-blue-300 dark:hover:bg-blue-900/30"
              >
                {retrying ? t("retrying") : t("retry")}
              </button>
            )}
```

- [ ] **Step 5: Stop truncating overlay errors**

In `src/components/Overlay.tsx`, replace:

```tsx
            <span className="max-w-[160px] truncate text-xs font-medium text-red-300">
              {state.data?.message ?? t("error")}
            </span>
```

with:

```tsx
            <span className="max-w-[520px] whitespace-normal break-words text-xs font-medium leading-snug text-red-300">
              {state.data?.message ?? t("error")}
            </span>
```

Also change the container class from:

```tsx
          "flex items-center gap-3 rounded-full px-5 py-2 shadow-lg backdrop-blur-md " +
```

to:

```tsx
          "flex max-w-[min(92vw,640px)] items-center gap-3 rounded-xl px-5 py-2 shadow-lg backdrop-blur-md " +
```

- [ ] **Step 6: Add locale labels**

In `src/locales/en.json`, add:

```json
  "retry": "Retry",
  "retrying": "Retrying...",
  "historyFailed": "Transcription failed"
```

In `src/locales/zh-TW.json`, add:

```json
  "retry": "重新傳送",
  "retrying": "重新傳送中...",
  "historyFailed": "轉錄失敗"
```

- [ ] **Step 7: Run frontend build**

Run:

```bash
pnpm build
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/types/history.ts src/lib/tauri.ts src/components/History/HistoryEntry.tsx src/components/History/HistoryList.tsx src/components/History/HistoryWindow.tsx src/components/Overlay.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: retry failed transcriptions from history"
```

---

### Task 7: End-to-End Verification

**Files:**
- No file edits.

- [ ] **Step 1: Run Rust test suite**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 2: Run Rust lint**

Run:

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: PASS.

- [ ] **Step 3: Run frontend build**

Run:

```bash
pnpm build
```

Expected: PASS.

- [ ] **Step 4: Manual Windows QA**

Run a Windows build and test these flows:

```bash
pnpm tauri:build:debug
```

Expected manual results:

- Push-to-talk with a two-or-three-word phrase longer than 250 ms reaches Processing and transcribes.
- A tap shorter than 250 ms shows `Recording was too short...`.
- Muted microphone or wrong microphone shows `No speech detected...`.
- Hands-free recording longer than 60 seconds sends multiple STT calls and joins text in order.
- Simulated Groq 429 or 5xx is retried once before surfacing an error.
- OpenAI 400 shows provider, status, and response body text without overlay truncation.
- Failed Groq/OpenAI STT creates a failed history row with Retry.
- Retry uses the saved WAV and updates the same row to completed on success.

- [ ] **Step 5: Commit verification notes if docs changed during QA**

If manual QA reveals a new platform-specific note, update `CLAUDE.md` under Windows notes and commit:

```bash
git add CLAUDE.md
git commit -m "docs: add windows transcription qa note"
```

If no doc update is needed, do not create a commit for this step.

---

## CEO Review

Skill applied: `plan-ceo-review`. Mode: **HOLD SCOPE** because this is a user-reported reliability bug, not a feature expansion.

### Premise Challenge

The right problem is not only "Groq sometimes fails." The user-facing problem is "VoxPen gives no trustworthy outcome after speech." That includes:

- No visible feedback when the app silently rejects short/silent recordings.
- One-shot STT requests where a transient provider failure loses the whole utterance.
- Error text that is technically present but visually unreadable.
- No recovery path when the audio was captured but STT failed.

### Scope Verdict

Keep the scope, but split it mentally into two release slices:

1. **Reliability hotfix slice:** visible rejection messages, live chunking, STT retry, full overlay error text.
2. **Recovery UX slice:** failed history rows with saved WAV and retry.

This preserves the complete user outcome while reducing implementation risk. If time is tight, ship slice 1 first and keep slice 2 in the same plan as the next commit sequence.

### Alternatives Considered

| Approach | Summary | Effort | Risk | Verdict |
|---|---|---:|---:|---|
| Minimal hotfix | Show errors and retry once, no history retry | S | Low | Too incomplete: still loses captured audio after provider failure |
| Current full plan | Fix short/long/API/error/retry paths | M | Medium | Recommended: solves the full reported failure loop |
| Streaming STT rewrite | Move live recording to streaming transcription | L/XL | High | Not for this bugfix; provider/model support differs and blast radius is too high |

### CEO Findings

1. **The plan originally over-weighted file-size thinking.** Default 6-minute WAVs are under 25 MB, so the stronger framing is request isolation and retryability, not just provider size limits.
2. **The resend button is not a luxury.** It is the trust repair mechanism when a user already spoke and the provider failed.
3. **Local/offline STT is explicitly not in scope.** It may be useful later, but it does not directly solve the Windows cloud-provider failure report in the smallest safe diff.

### NOT in Scope

- Streaming STT or partial live transcripts: larger architecture change, separate provider capability matrix needed.
- Local Whisper fallback: useful future reliability layer, but it changes install size, model lifecycle, and Windows performance assumptions.
- New settings UI for chunk length/retry count: operationally useful later, but defaults are enough for this bugfix.
- Provider failover from Groq to OpenAI: crosses API key/provider preference boundaries and could surprise BYOK users.

### Dream State Delta

```text
CURRENT STATE
  User speaks -> app may silently drop or send one fragile STT call

THIS PLAN
  User speaks -> app validates visibly -> chunks/retries STT -> saves failure -> retry possible

12-MONTH IDEAL
  User speaks -> live confidence/progress -> resumable transcription queue -> provider/local fallback
```

## Design Review

Skill applied: `plan-design-review`. This was a text-only plan review. No mockups were generated because the UI scope is small and the user explicitly asked to write documentation first.

Initial design completeness: **5/10**. The plan names the UI files, but it needs clearer interaction-state specs so the implementer does not ship generic buttons and truncated errors.

### UI Scope

- Overlay error rendering in `src/components/Overlay.tsx`.
- Failed history entry display and retry action in `src/components/History/HistoryEntry.tsx`.
- Loading state while retry is in-flight.
- Localized labels for retry/failure.

### Information Architecture

Target hierarchy:

```text
Overlay error
  1. Error icon/status color
  2. Human-readable provider/status message
  3. Short recovery hint when applicable

History failed row
  1. Timestamp + "Transcription failed"
  2. Provider/status/error detail
  3. Retry action
  4. Delete action
```

### Interaction State Coverage

| Feature | Loading | Empty | Error | Success | Partial |
|---|---|---|---|---|---|
| Overlay STT error | Processing pill remains until failure | Not applicable | Multi-line wrapped message, not truncated | Done state unchanged | If refinement fails, raw transcription still shows success |
| Failed history row | Retry button disabled and label `Retrying...` | No failed entries use existing history empty state | Full error detail in row body | Same row updates to completed transcript | Retry succeeds STT but refinement fails: show completed raw text, no refined text |
| Retry action | Per-row loading only | Missing `audio_path` shows non-retryable error | Command error shown inline or existing row remains failed | Reload history and clear error | Double-click retry blocked by disabled state |

### Design Findings

1. **Overlay error width should be bounded but readable.** Use wrapped text with a max width around 520-640px, not `truncate`.
2. **Failed history rows need a different visual treatment.** Use restrained red/error styling only inside the row body; avoid turning the entire list into warning noise.
3. **Retry must not look like the primary history action for completed entries.** Show it only for failed entries and keep copy/delete behavior unchanged for successful entries.
4. **The plan should specify what happens during retry.** Disable retry while in flight and keep the row expanded if possible so users see continuity.
5. **OpenAI failures must name OpenAI explicitly.** Users should see `openai HTTP 400: ...` or `API key not configured for openai`, not a generic `Transcription failed: HTTP...` fragment.

Updated design completeness after this review: **8/10**. Remaining gap: exact visual polish should be checked after implementation with a live UI/design review.

## Engineering Review

Skill applied: `plan-eng-review`. Review posture: full plan architecture/test review, but document-only.

### Step 0 Scope Challenge

The plan touches more than 8 files, which is usually a smell. Here it is justified because the failure crosses Rust pipeline, provider API, persisted history, Tauri command surface, and React UI. The implementation should still be split into two sequential slices:

```text
Slice 1: live transcription reliability
  hotkey.rs -> transcribe.rs -> groq.rs -> Overlay.tsx

Slice 2: recover failed attempts
  history.rs -> recording_store.rs -> commands.rs -> History UI
```

### What Already Exists

| Existing code | Reuse decision |
|---|---|
| `audio::encoder::pcm_to_wav()` | Reuse for saving live recordings and chunk uploads |
| `audio::chunker::chunk_wav()` | Reuse concepts/tests, but live PCM chunking should stay simpler because source audio is already 16 kHz mono PCM |
| `pipeline::controller::on_stop_recording()` | Keep orchestration here; do not move STT into React |
| `api::groq::transcribe_file_with_segments()` | Reuse for file transcription; do not mix live retry state into file flow unless helper extraction is clean |
| `history::HistoryDb` | Extend schema additively; do not create a parallel failed-recording DB |
| `Overlay.tsx` and `HistoryEntry.tsx` | Reuse existing UI surfaces instead of creating a new failed-jobs screen |

### Architecture Diagram

```text
Hotkey release
  |
  v
recorder.stop() -> Vec<i16>
  |
  +--> recording validation
  |       | short/silent -> PipelineState::Error + no API call
  |
  +--> save WAV to app_data/recordings/{history_id}.wav
  |
  v
PipelineController::on_stop_recording()
  |
  v
pipeline::transcribe()
  |
  +--> split PCM into bounded chunks
  |       |
  |       v
  |     api::groq::transcribe_with_base_url()
  |       | retry transient 408/429/5xx/connect/timeout once
  |
  v
Result/Refined/Error
  |
  +--> success: completed history row + paste
  |
  +--> failure: failed history row + saved audio path + retry UI
```

### Engineering Findings

1. **Chunk duration needs an explicit product default.** The plan currently uses 60 seconds. Keep 60 seconds for simplicity, but add tests that prove multi-chunk text joins in order. Consider 1-second overlap only if QA finds word loss at boundaries; overlap adds duplicate-text cleanup complexity.
2. **Retry should be narrow.** Retry connect errors, timeouts, 408, 429, and 5xx once. Do not retry 400, 401, 403, 413, malformed request, or missing API key.
3. **Failed history rows need schema migration tests.** Old SQLite rows must deserialize as `Completed` with `audio_path = NULL`; otherwise existing users may lose history access.
4. **Saving recordings needs a cleanup policy.** Keep cleanup out of the first implementation if necessary, but record it as NOT in scope. Without cleanup, retry audio can grow over time.
5. **Do not put raw provider response bodies in logs if they can contain user content.** Error snippets shown to the user should be bounded; logs should include provider/status/request context, not API keys or full audio/transcript content.
6. **OpenAI cannot be treated as incidental support.** Add explicit OpenAI routing, key-test, model-selection, live-STT, file-STT, retry, and error-display tests; otherwise the settings UI can advertise a provider path that fails at first use.

### Test Coverage Diagram

```text
CODE PATHS
[+] hotkey recording validation
  ├── [GAP] short recording -> visible Error state
  ├── [GAP] silent recording -> visible Error state
  └── [GAP] 250ms non-silent phrase -> proceeds to STT

[+] live PCM chunking
  ├── [GAP] <= chunk size -> one STT request
  ├── [GAP] > chunk size -> N STT requests
  └── [GAP] chunk texts join in order and trim empty text

[+] STT retry/error mapping
  ├── [GAP] 429 then 200 -> transparent retry success
  ├── [GAP] 500 twice -> normalized provider HTTP error
  ├── [GAP] 401 -> ApiKeyMissing, no retry
  └── [GAP] 413 -> file-too-large message, no retry

[+] OpenAI Audio API support
  ├── [GAP] openai provider -> /v1/audio/transcriptions
  ├── [GAP] OpenAI key test does not return unsupported provider
  ├── [GAP] OpenAI invalid request -> full bounded error is visible
  ├── [GAP] OpenAI live failure -> failed history row with saved WAV
  └── [GAP] OpenAI retry -> resends saved WAV and updates same row

[+] failed recording persistence
  ├── [GAP] successful STT -> completed row with audio_path
  ├── [GAP] failed STT -> failed row with error_message/audio_path
  ├── [GAP] old DB migrates with default completed status
  └── [GAP] retry updates same row to completed

USER FLOWS
[+] Windows push-to-talk
  ├── [MANUAL] short phrase longer than 250ms transcribes
  ├── [MANUAL] quick tap shows visible too-short error
  └── [MANUAL] release-before-start race does not deadlock processing flag

[+] History retry
  ├── [GAP] failed row shows retry
  ├── [GAP] double-click retry disabled while in-flight
  └── [GAP] retry failure leaves row failed and readable
```

### Performance Notes

- A 60-second WAV chunk is about 1.92 MB at 16 kHz mono i16, so memory and upload size are acceptable.
- Saving full live WAVs duplicates audio bytes on disk. This is acceptable for the recovery UX, but needs later retention cleanup.
- Sequential chunking is safer than parallel chunking because Whisper prompt/context and provider rate limits are easier to reason about. Parallel chunking is not recommended for this bugfix.

### Worktree Parallelization Strategy

Sequential implementation is recommended for correctness. If split across worktrees, use two lanes only after Slice 1 API/helper shapes are stable:

| Lane | Modules touched | Depends on |
|---|---|---|
| A: live reliability | `src-tauri/src/hotkey.rs`, `src-tauri/crates/voxpen-core/src/pipeline/`, `src-tauri/crates/voxpen-core/src/api/` | none |
| B: history retry UX | `src-tauri/src/history.rs`, `src-tauri/src/commands.rs`, `src/components/History/`, `src/lib/tauri.ts` | A's persisted audio path/status decisions |

Execution order: implement Lane A first, then Lane B. Parallelizing these too early risks schema/API mismatch.

### Failure Modes Registry

| Codepath | Failure mode | Rescued? | Test? | User sees? | Logged? |
|---|---|---:|---:|---|---|
| Recording validation | Short tap | Yes | Planned | Too-short message | No need |
| Recording validation | Wrong/silent mic | Yes | Planned | No-speech message | Optional |
| STT request | 429 | Yes, retry once | Planned | Nothing if retry succeeds; error if not | Yes, bounded |
| STT request | 401 | Yes, no retry | Planned | API key/provider error | Yes, no key |
| STT request | 413 | Yes, no retry | Planned | File too large/chunking bug | Yes |
| OpenAI key test | Provider rejected as unsupported | Yes | Planned | Settings test fails clearly | Yes |
| OpenAI STT | Invalid model or request body | Yes | Planned | Full `openai HTTP ...` error | Yes, bounded |
| History insert | DB locked/write fails | Partial | Missing | Transcription may still paste but retry row missing | Yes |
| Retry command | Missing saved WAV | Yes | Planned | Cannot retry saved audio missing | Yes |
| Retry UI | Double click | Yes | Planned | Button disabled | No need |

Critical gaps remaining in the plan: **0**, assuming the planned tests are implemented.

### Engineering Review Verdict

Proceed with the full plan, but implement it in two slices. Do not add streaming, provider failover, or local Whisper fallback in this PR.

---

## Self-Review

Spec coverage:

- Push-to-talk no response: Task 1 replaces silent short/silent drops with visible errors and lowers the minimum duration for short phrases.
- Hands-free API unreliability: Tasks 2 and 3 chunk live audio and retry transient provider failures.
- Long recordings: Task 2 chunks live PCM into 60-second API calls.
- API no response: Task 3 bounds transient retry behavior and surfaces provider/status/body details.
- Resend button: Tasks 4, 5, and 6 persist failed recordings and expose a Retry action.
- Short two-or-three-word recordings: Task 1 lowers the threshold from 0.5s to 0.25s and only rejects below that.
- Truncated OpenAI error: Task 6 removes overlay truncation and keeps error text wrapped.

Placeholder scan:

- The plan contains no deferred implementation placeholders and no test-writing steps without concrete test code.

Type consistency:

- Rust status type is `TranscriptionStatus::{Completed, Failed}`.
- Serialized TypeScript status values are `"completed" | "failed"`.
- Retry command is consistently named `retry_transcription` in Rust and `retryTranscription` in TypeScript.
- History fields are consistently named `status`, `error_message`, and `audio_path`.
