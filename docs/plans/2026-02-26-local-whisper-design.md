# Local Whisper Model Integration — Design

## Goal

Add offline speech-to-text via whisper-rs (whisper.cpp Rust bindings) as an alternative to cloud STT providers. Audio never leaves the user's machine, no API key needed, unlimited free usage.

## Architecture

New `LocalSttProvider` implements the existing `SttProvider` trait, sitting alongside `GroqSttProvider`. The pipeline controller, hotkey handler, and all downstream logic remain unchanged.

**whisper-rs in-process** — no sidecar, no IPC. whisper.cpp is a C library linked via whisper-rs (v0.15.1). GPU acceleration via compile-time Cargo features (CoreML/Metal on macOS, CUDA on Windows/Linux).

## Provider Selection

```
Settings.stt_provider: "groq" | "openai" | "local"
```

When `local`: skip API key validation, use `LocalSttProvider`, model file must exist on disk.

## Model Tiers

| Tier | File | Size | RAM | Notes |
|------|------|------|-----|-------|
| Quick | `ggml-small-q5_1.bin` | ~190 MB | ~600 MB | Older hardware |
| Balanced | `ggml-large-v3-turbo-q5_0.bin` | ~574 MB | ~1.5 GB | **Default** |
| Quality | `ggml-large-v3-turbo-q8_0.bin` | ~874 MB | ~2.2 GB | Higher quality |
| Maximum | `ggml-large-v3-q5_0.bin` | ~1.08 GB | ~2.5 GB | Best accuracy |

Models stored in `{app_data_dir}/models/`, downloaded from HuggingFace via reqwest with progress events.

## Audio Conversion

VoxPen captures `Vec<i16>` (16kHz mono). whisper-rs expects `&[f32]` normalized to [-1.0, 1.0].

Conversion: `sample as f32 / 32768.0` — done in `LocalSttProvider::transcribe()`.

## GPU Acceleration

Cargo feature flags:
- macOS: `coreml` + `metal`
- Windows/Linux: `cuda` + CPU fallback

whisper.cpp auto-detects hardware at runtime.

## Cached WhisperContext

Model loading takes 1-3s. `WhisperContext` is loaded once and cached in `AppState`. Reused across transcriptions. Invalidated when user switches model tier.

## Settings UI

When "Local (Whisper)" selected as STT provider:
- Hide API key section
- Show model tier dropdown with size labels
- Show download status + progress bar
- Show delete button to reclaim disk space

## New Tauri Commands

- `get_model_status(model)` → NotDownloaded | Downloading { progress } | Ready { size }
- `download_model(model)` → starts async download with progress events
- `delete_model(model)` → removes model file

## Error Variants

- `ModelNotDownloaded(String)`
- `ModelDownload(String)`
- `LocalTranscription(String)`

## Not In Scope

- Streaming/real-time transcription
- Custom model file path (user BYO)
- Model auto-update
- VAD (voice activity detection)
