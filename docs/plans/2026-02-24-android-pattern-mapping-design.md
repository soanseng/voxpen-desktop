# Design: Android Pattern Mapping for VoxInk Desktop

**Date**: 2026-02-24
**Status**: Approved

## Context

VoxInk Android v1.0 is complete. The desktop version (Tauri v2 + Rust + React) should mirror the same architectural patterns, adapted to Rust idioms.

## Decision

Rewrite CLAUDE.md and plan.md from scratch, closely mirroring the Android versions' structure and conventions. Add TDD methodology with Rust-specific tooling.

## Key Architecture Mappings

### State Machine
- **Android**: `sealed interface ImeUiState` (7 states: Idle, Recording, Processing, Result, Refining, Refined, Error)
- **Desktop**: `enum PipelineState` with `#[derive(Serialize, Clone)]` and `#[serde(tag = "type", content = "data")]`

### Repository Pattern
- **Android**: `@Singleton class SttRepository` / `LlmRepository` returning `Result<String>`
- **Desktop**: `pub async fn transcribe()` / `pub async fn refine()` in `api/` modules returning `Result<String, AppError>`

### Pipeline Controller
- **Android**: `RecordingController` with `MutableStateFlow<ImeUiState>` + `CoroutineScope`
- **Desktop**: `PipelineController` struct with `tokio::sync::watch::Sender<PipelineState>` + Tauri `app.emit()`

### Use Cases
- **Android**: `TranscribeAudioUseCase` (PCM → WAV → STT), `RefineTextUseCase` (text → LLM)
- **Desktop**: `pipeline::transcribe()` and `pipeline::refine()` functions composing encoder + API modules

### Audio
- **Android**: `AudioRecorder` (AudioRecord API, 16kHz mono i16), `AudioEncoder.pcmToWav()` (44-byte header)
- **Desktop**: `audio::recorder` (cpal, same params), `audio::encoder::pcm_to_wav()` (same header)

### Storage
- **Android**: `ApiKeyManager` (EncryptedSharedPreferences), `PreferencesManager` (DataStore), `AppDatabase` (Room)
- **Desktop**: `storage` module (Tauri store plugin, encrypted), SQLite via `rusqlite`/Tauri SQL plugin

### DI
- **Android**: Hilt (`@Inject`, `@Singleton`, `@Module`)
- **Desktop**: Rust constructor injection — structs with `new()` taking trait objects

### Testing
- **Android**: JUnit 5 + MockK + Turbine + MockWebServer + Truth (80% coverage)
- **Desktop**: `#[test]` + `mockall` + `wiremock` + `tokio::test` + `assert_eq!` (80% coverage)

## Constants (identical across platforms)

| Constant | Value |
|---|---|
| Whisper models | `whisper-large-v3` / `whisper-large-v3-turbo` (default) |
| LLM models | `openai/gpt-oss-120b` (default) / `openai/gpt-oss-20b` |
| LLM temperature | `0.3` |
| LLM max tokens | `2048` |
| PCM sample rate | `16000` Hz |
| PCM channels | `1` (mono) |
| PCM bits per sample | `16` |
| WAV header | `44` bytes |
| Groq base URL | `https://api.groq.com/` |
| HTTP connect timeout | 30s |
| HTTP read/write timeout | 60s |
| Chunk size limit | 25 MB |

## Refinement Prompts

4 prompts (zh/en/ja/mixed) — copied verbatim from Android's `RefinementPrompt.kt`.

## What's NOT carried over

- **Monetization** (ads, billing, Pro/Free tiers) — desktop is free
- **IME-specific patterns** (InputMethodService, candidate bar, keyboard view)
- **Android-specific DI** (Hilt, EntryPoint) — Rust uses constructor injection
- **Jetpack Compose UI** — React + Tailwind instead
