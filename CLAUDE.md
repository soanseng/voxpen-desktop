# CLAUDE.md — VoxInk Desktop (語墨桌面版)

## Project Overview

VoxInk Desktop is a system-tray voice-to-text app for macOS, Windows, and Linux. Users press a global hotkey to dictate, and the transcribed + refined text is automatically pasted at the cursor position in any app. Built with Tauri v2 (Rust backend + React frontend), BYOK model.

**App Name**: VoxInk Desktop (語墨)
**App Identifier**: `com.voxink.desktop`
**Relationship to Android**: Separate codebase. Shares API endpoints, refinement prompts, and language list, but no code-level dependency. Architecture mirrors Android's MVVM + Repository pattern, adapted to Rust idioms.

## Tech Stack

- **Framework**: Tauri v2
- **Backend**: Rust (2021 edition)
- **Frontend**: React + TypeScript + Tailwind CSS
- **Build**: Cargo (Rust) + Vite (frontend)
- **Async Runtime**: `tokio`
- **HTTP Client**: `reqwest` (Rust-side API calls, 30s connect / 60s read-write timeout)
- **Audio Capture**: `cpal` crate (cross-platform, 16kHz mono PCM)
- **Audio Encoding**: `hound` crate (PCM → WAV, 44-byte RIFF header)
- **Global Hotkey**: `tauri-plugin-global-shortcut`
- **System Tray**: `tauri-plugin-tray-icon`
- **Clipboard**: `arboard` crate
- **Paste Simulation**: `enigo` crate
- **Settings Storage**: `tauri-plugin-store` (encrypted JSON for API keys)
- **History Database**: SQLite via `rusqlite` or `tauri-plugin-sql`
- **Serialization**: `serde` + `serde_json`
- **Error Handling**: `thiserror` crate
- **Testing**: `mockall` + `wiremock` + `tokio::test`

## Core Architecture

```
┌─────────────────────────────────────────┐
│              System Tray Icon            │
│         (always running, minimal)        │
└──────────────┬──────────────────────────┘
               │
    ┌──────────▼──────────┐
    │   Global Hotkey      │
    │  (Cmd+Shift+V etc.)  │
    └──────────┬──────────┘
               │ press
    ┌──────────▼──────────┐
    │   Audio Recording    │  ← Rust: cpal + PCM capture
    │   (while held/toggle)│
    └──────────┬──────────┘
               │ release
    ┌──────────▼──────────┐
    │   Encode Audio       │  ← Rust: PCM → WAV (hound)
    └──────────┬──────────┘
               │
    ┌──────────▼──────────┐
    │   STT API Call       │  ← Rust: reqwest → Groq/OpenAI
    │   (Whisper)          │
    └──────────┬──────────┘
               │ raw text
    ┌──────────▼──────────┐
    │   LLM Refinement     │  ← Rust: reqwest → Groq/OpenAI/Anthropic
    │   (optional)         │
    └──────────┬──────────┘
               │ refined text
    ┌──────────▼──────────┐
    │   Paste to Cursor    │  ← Rust: clipboard + simulate Cmd+V
    └──────────────────────┘
```

**Key principle**: The entire hotkey → record → transcribe → paste pipeline runs in Rust. The React frontend is only for the settings window and transcription history. This ensures minimal latency and no Electron-like overhead.

## Core Features

### 1. Global Hotkey Voice Input
- System-wide hotkey (default: `Cmd+Shift+V` on Mac, `Ctrl+Shift+V` on Win/Linux)
- Two modes:
  - **Hold-to-dictate**: press and hold hotkey, release to stop (default)
  - **Toggle**: press once to start, press again to stop
- Works in ANY app — no input method switching needed

### 2. Floating Overlay (Recording Indicator)
- Tauri secondary window: small, frameless, always-on-top, click-through
- States: Recording (red pulse) → Processing (spinner) → Done (green check, auto-hide) → Error (red X)
- Position: user-configurable (corner of screen)
- Does NOT steal focus from the active app

### 3. Auto-Paste
- Save current clipboard → put transcription → simulate `Cmd+V`/`Ctrl+V` → restore clipboard
- Fallback: if paste simulation fails, clipboard-only + notification
- User option: auto-paste vs clipboard-only

### 4. STT Providers (BYOK)
- **Groq Whisper** (primary): `whisper-large-v3-turbo`
- **OpenAI Whisper**: `whisper-1`, `gpt-4o-transcribe`
- **Custom Server**: user-defined endpoint

#### Supported Languages (v1)
- **Auto-detect** (default): best for mixed-language (中英混合)
- **中文** (zh): Traditional Chinese with prompt bias
- **English** (en)
- **日本語** (ja)
- Mixed-language: omit `language` param + prompt hint "繁體中文，可能夾雜英文"

### 5. LLM Refinement (BYOK)
- Same refinement pipeline as Android version
- **Providers**: Groq (LLaMA), OpenAI (GPT), Anthropic (Claude), custom endpoint
- **Features**: filler word removal, self-correction detection, auto-punctuation, custom system prompts, mixed-language aware
- **Desktop-specific**: no dual-choice UI — just paste refined text. User can `Cmd+Z` to undo. Original always in history.

### 6. System Tray Menu
- Status line: Ready / Recording... / Processing...
- Quick language switch: Auto / 中文 / English / 日本語
- Quick refinement toggle: On / Off
- Open History / Open Settings
- About / Quit

### 7. Settings Window (React UI)
- Hotkey configuration, recording mode (hold vs toggle), auto-paste on/off
- STT/LLM provider + API key (masked, encrypted storage, test button)
- Language, refinement toggle, custom prompt editor
- Overlay position, launch at login, theme (system/light/dark), i18n (繁中/English)

### 8. Transcription History
- SQLite: id, timestamp, original_text, refined_text, language, audio_duration_ms, provider
- Search/filter, click to copy, export (JSON/plain text), auto-cleanup (configurable retention)

### 9. Audio File Transcription
- Drag-and-drop or file picker, auto-chunking for files > 25MB
- Progress bar, export as text/SRT, optional LLM refinement

## Architecture Pattern Mapping (Android → Rust)

| Android (Kotlin) | Desktop (Rust) | Notes |
|---|---|---|
| `sealed interface ImeUiState` (7 states) | `enum PipelineState` with `#[derive(Serialize, Clone)]` | Idle, Recording, Processing, Result, Refining, Refined, Error |
| `RecordingController` + `CoroutineScope` | `PipelineController` struct + `tokio::sync::watch::Sender<PipelineState>` | State broadcast to frontend via Tauri events |
| `SttRepository` (`@Singleton`, `Result<String>`) | `stt` module with `pub async fn transcribe(...) -> Result<String, AppError>` | Same `reqwest` multipart POST |
| `LlmRepository` (`@Singleton`, `Result<String>`) | `llm` module with `pub async fn refine(...) -> Result<String, AppError>` | Same chat completion request |
| `TranscribeAudioUseCase` (orchestrates PCM→WAV→STT) | `pipeline::transcribe` function | Composes encoder + stt |
| `RefineTextUseCase` (thin delegation) | `pipeline::refine` function | Composes prompts + llm |
| `sealed class SttLanguage(code, prompt)` | `enum Language { Auto, Chinese, English, Japanese }` with `code()` and `prompt()` methods | Same values |
| `object RefinementPrompt` with `forLanguage()` | `prompts::for_language(lang: &Language) -> &'static str` match expression | Same 4 prompts verbatim |
| `enum RecordingMode` | `enum RecordingMode { HoldToRecord, Toggle }` | Same behavior |
| `AudioRecorder` (16kHz, mono, i16 PCM) | `audio::recorder` module using `cpal` | Same sample rate/channels/bit depth |
| `AudioEncoder.pcmToWav()` (44-byte RIFF header) | `audio::encoder::pcm_to_wav()` | Same WAV header construction |
| `AudioChunker` (25MB limit, WAV-aware) | `audio::chunker` module | Same chunking logic |
| `ApiKeyManager` (EncryptedSharedPreferences) | `storage` module using Tauri store plugin | Same key names |
| `PreferencesManager` (DataStore + Flow) | `storage` module using Tauri store plugin | Settings as JSON |
| `AppDatabase` + `TranscriptionDao` (Room) | SQLite via `rusqlite` or Tauri SQL plugin | Same schema |
| Hilt DI (`@Inject`, `@Singleton`, `@Module`) | Rust constructor injection (no DI framework) | Structs with `new()` taking dependencies |
| `StateFlow` → Compose UI | Tauri `app.emit()` → React `listen()` | Same event-driven pattern |
| Retrofit + OkHttp (30s/60s timeout) | `reqwest::Client` with same timeouts | Per-call `Bearer` auth header |

## Project Structure

```
voxink-desktop/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs                    # Tauri entry point
│   │   ├── lib.rs                     # Module exports
│   │   ├── error.rs                   # AppError (thiserror)
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── recorder.rs            # cpal: 16kHz mono i16 PCM capture
│   │   │   ├── encoder.rs             # PCM → WAV (44-byte RIFF header)
│   │   │   └── chunker.rs             # WAV-aware splitting (>25MB)
│   │   ├── api/
│   │   │   ├── mod.rs
│   │   │   ├── groq.rs                # Groq Whisper STT + LLaMA LLM
│   │   │   ├── openai.rs              # OpenAI Whisper + GPT
│   │   │   ├── anthropic.rs           # Claude API
│   │   │   └── custom.rs              # User-defined endpoint
│   │   ├── pipeline/
│   │   │   ├── mod.rs
│   │   │   ├── state.rs               # PipelineState enum (7 states)
│   │   │   ├── controller.rs          # PipelineController (state machine)
│   │   │   ├── transcribe.rs          # STT orchestration (encode → API)
│   │   │   ├── refine.rs              # LLM refinement orchestration
│   │   │   └── prompts.rs             # 4 prompts: zh/en/ja/mixed
│   │   ├── input/
│   │   │   ├── mod.rs
│   │   │   ├── clipboard.rs           # Clipboard save/restore
│   │   │   └── paste.rs               # Simulate Cmd+V / Ctrl+V (enigo)
│   │   ├── tray.rs                    # System tray setup & menu
│   │   ├── hotkey.rs                  # Global shortcut registration
│   │   ├── overlay.rs                 # Floating widget Tauri commands
│   │   ├── storage.rs                 # Encrypted settings + API key manager
│   │   ├── history.rs                 # SQLite transcription history
│   │   └── commands.rs                # Tauri IPC commands
│   └── icons/
├── src/                                # React frontend
│   ├── App.tsx
│   ├── main.tsx
│   ├── components/
│   │   ├── Overlay.tsx                # Floating recording indicator
│   │   ├── Settings/
│   │   │   ├── SettingsWindow.tsx
│   │   │   ├── ApiKeySection.tsx
│   │   │   ├── HotkeySection.tsx
│   │   │   ├── LanguageSection.tsx
│   │   │   └── PromptsSection.tsx
│   │   └── History/
│   │       ├── HistoryWindow.tsx
│   │       ├── HistoryList.tsx
│   │       └── HistoryEntry.tsx
│   ├── hooks/
│   │   ├── useTauriEvents.ts
│   │   └── useSettings.ts
│   ├── lib/
│   │   ├── tauri.ts                   # Tauri invoke wrappers
│   │   └── i18n.ts
│   ├── locales/
│   │   ├── en.json
│   │   └── zh-TW.json
│   └── styles/
│       └── globals.css
├── package.json
├── vite.config.ts
├── tailwind.config.js
└── tsconfig.json
```

## Tauri IPC Commands (Rust ↔ React)

```rust
// Rust commands callable from React via invoke()
#[tauri::command] fn get_settings() -> Settings;
#[tauri::command] fn save_settings(settings: Settings) -> Result<(), String>;
#[tauri::command] fn save_api_key(provider: String, key: String) -> Result<(), String>;
#[tauri::command] fn get_history(limit: u32, offset: u32) -> Vec<TranscriptionEntry>;
#[tauri::command] fn delete_history_entry(id: String) -> Result<(), String>;
#[tauri::command] fn transcribe_file(path: String) -> Result<TranscriptionResult, String>;
#[tauri::command] fn set_hotkey(shortcut: String) -> Result<(), String>;
#[tauri::command] fn test_api_key(provider: String, key: String) -> Result<bool, String>;
```

```rust
// Rust → React events (via emit)
app.emit("pipeline-state", PipelineState::Recording);
app.emit("pipeline-state", PipelineState::Processing);
app.emit("pipeline-state", PipelineState::Refined { original, refined });
app.emit("pipeline-state", PipelineState::Error { message });
```

## API Interaction Patterns

All API calls happen in Rust (not React). Keys never enter the webview/JS context. Authorization is per-call `"Bearer {api_key}"` header (not a global interceptor), matching Android's pattern.

### Groq Whisper STT
```
POST https://api.groq.com/openai/v1/audio/transcriptions
Headers: Authorization: Bearer {GROQ_API_KEY}
Body (multipart/form-data):
  file: recording.wav (audio/wav)
  model: whisper-large-v3-turbo (or whisper-large-v3, user-selectable)
  response_format: verbose_json
  language: zh (or en, ja, omit for auto-detect)
  prompt: per-language prompt hint
```

### LLM Refinement
```
POST https://api.groq.com/openai/v1/chat/completions
Headers: Authorization: Bearer {GROQ_API_KEY}
Body (JSON):
  model: openai/gpt-oss-120b (or openai/gpt-oss-20b, user-selectable)
  messages:
    - { role: "system", content: [refinement prompt per language] }
    - { role: "user", content: [raw transcription text] }
  temperature: 0.3
  max_tokens: 2048
```

### Key Constants (matching Android)

| Constant | Value | Location |
|---|---|---|
| Whisper models | `whisper-large-v3` / `whisper-large-v3-turbo` (default) | `api/groq.rs` |
| STT response format | `verbose_json` | `api/groq.rs` |
| LLM models | `openai/gpt-oss-120b` (default) / `openai/gpt-oss-20b` | `api/groq.rs` |
| LLM temperature | `0.3` | `api/groq.rs` |
| LLM max tokens | `2048` | `api/groq.rs` |
| PCM sample rate | `16000` Hz | `audio/recorder.rs` |
| PCM channels | `1` (mono) | `audio/recorder.rs` |
| PCM bits per sample | `16` | `audio/recorder.rs` |
| WAV header size | `44` bytes | `audio/encoder.rs` |
| Groq base URL | `https://api.groq.com/` | `api/groq.rs` |
| HTTP connect timeout | 30s | `api/mod.rs` |
| HTTP read/write timeout | 60s | `api/mod.rs` |
| Chunk size limit | 25 MB | `audio/chunker.rs` |
| Storage key (Groq) | `groq_api_key` | `storage.rs` |

## Default Refinement Prompts

Identical to Android version. Defined in `src-tauri/src/pipeline/prompts.rs`, user-customizable via settings.

**zh-TW (Traditional Chinese)**
```
你是一個語音轉文字的編輯助手。請將以下口語內容整理為流暢的書面文字：
1. 移除贅字（嗯、那個、就是、然後、對、呃）
2. 如果說話者中途改口，只保留最終的意思
3. 修正語法但保持原意
4. 適當加入標點符號
5. 不要添加原文沒有的內容
6. 保持繁體中文
只輸出整理後的文字，不要加任何解釋。
```

**en (English)**
```
You are a voice-to-text editor. Clean up the following speech transcription into polished written text:
1. Remove filler words (um, uh, like, you know, I mean, basically, actually, so)
2. If the speaker corrected themselves mid-sentence, keep only the final version
3. Fix grammar while preserving the original meaning
4. Add proper punctuation
5. Do not add content that wasn't in the original speech
Output only the cleaned text, no explanations.
```

**ja (Japanese)**
```
あなたは音声テキスト変換の編集アシスタントです。以下の口語内容を整った書き言葉に整理してください：
1. フィラー（えーと、あの、まあ、なんか、ちょっと）を除去
2. 言い直しがある場合は最終的な意味のみ残す
3. 文法を修正し、原意を保持
4. 適切に句読点を追加
5. 原文にない内容を追加しない
整理後のテキストのみ出力し、説明は不要です。
```

**Mixed-language (zh-en, zh-ja, etc.)**
```
你是一個語音轉文字的編輯助手。以下口語內容可能包含多種語言混合使用（如中英混合），請保持原本的語言混合方式，整理為流暢的書面文字：
1. 移除各語言的贅字
2. 如果說話者中途改口，只保留最終的意思
3. 修正語法但保持原意和原本的語言選擇
4. 適當加入標點符號
5. 不要把外語強制翻譯成中文
只輸出整理後的文字，不要加任何解釋。
```

## Per-Language Whisper Prompts

| Language | `language` param | `prompt` param |
|---|---|---|
| Auto | `null` (omit) | `繁體中文，可能夾雜英文。` |
| Chinese | `zh` | `繁體中文轉錄。` |
| English | `en` | `Transcribe the following English speech.` |
| Japanese | `ja` | `以下の日本語音声を文字起こししてください。` |

## Development Guidelines

### Rust Code Style
- Rust 2021 edition idioms
- `thiserror` for custom error types, `Result<T, AppError>` everywhere
- `tokio` for async runtime
- `serde` for serialization (`#[derive(Serialize, Deserialize)]`)
- Prefer `Result<T, E>` over panics — never `unwrap()` in production code
- `clippy` for linting, `rustfmt` for formatting
- Traits for abstraction boundaries (accept traits, return concrete types)

### React/TypeScript Code Style
- Functional components with hooks
- TypeScript strict mode
- Tailwind for styling
- Minimal dependencies — keep bundle small
- No API calls from frontend — always invoke Rust commands

### Key Conventions
- All API calls happen in Rust (not in React frontend)
- React only handles UI rendering and invokes Rust commands via `invoke()`
- Settings stored as encrypted JSON (Tauri store plugin)
- History stored in SQLite
- API keys never logged or exposed in UI beyond masked display (first 4 + `••••` + last 4)
- No analytics, no telemetry, no phone-home
- Per-call `Bearer` auth header (not global interceptor) — same as Android

### Error Handling Pattern
```rust
// Custom error type with thiserror
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("API key not configured")]
    ApiKeyMissing,
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Transcription failed: {0}")]
    Transcription(String),
    #[error("Refinement failed: {0}")]
    Refinement(String),
    #[error("Audio error: {0}")]
    Audio(String),
    #[error("Storage error: {0}")]
    Storage(String),
}
```

### State Machine Pattern
```rust
// Mirrors Android's ImeUiState sealed interface
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum PipelineState {
    Idle,
    Recording,
    Processing,
    Result { text: String },
    Refining { original: String },
    Refined { original: String, refined: String },
    Error { message: String },
}
```

## TDD — Test-Driven Development

開發遵循 TDD，核心循環為 **Red-Green-Refactor**：

1. **Red** — 先寫一個會失敗的測試，定義預期行為
2. **Green** — 寫最少量的程式碼讓測試通過（不多不少）
3. **Refactor** — 在測試保護下重構，消除重複、改善設計

原則：
- 每次只加一個測試，確認失敗後才寫實作
- 不寫沒有對應測試的產品程式碼
- Refactor 階段測試必須持續全綠
- 目標覆蓋率 ≥ 80%

### 測試工具

| 層級 | 工具 |
|------|------|
| Unit tests | `#[cfg(test)]` + `#[test]` / `#[tokio::test]` |
| Trait mocking | `mockall` crate (`#[automock]`) |
| HTTP mocking | `wiremock` crate (MockServer) |
| Assertions | `assert_eq!`, `assert_matches!`, custom matchers |
| Coverage | `cargo-tarpaulin` or `cargo-llvm-cov` |
| Frontend tests | Vitest + React Testing Library |

### 測試命名慣例

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn should_return_transcription_when_api_responds_successfully() { ... }

    #[tokio::test]
    async fn should_emit_error_state_when_network_fails() { ... }

    #[test]
    fn should_encode_pcm_to_valid_wav_header() { ... }
}
```

### Testing Priority
1. **Pipeline modules** (`stt`, `llm`, `transcribe`, `refine`) — unit tests with `wiremock`
2. **Audio encoding** (`encoder`, `chunker`) — unit tests with known PCM data
3. **State machine** (`PipelineController`) — unit tests verifying state transitions
4. **Storage** (`storage`, `history`) — integration tests with temp files/DB
5. **Frontend** — Vitest for React components

### Build Verification

```bash
# After every change:
cargo build --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# Frontend:
pnpm test
pnpm build
```

## Platform-Specific Notes

### macOS (primary)
- Requires **Accessibility permission** for global hotkey + paste simulation
- Requires **Microphone permission**
- App lives in menu bar (no dock icon)
- Code signing + notarization needed for distribution
- DMG installer

### Windows
- Global hotkey via `RegisterHotKey` API
- Paste simulation via `SendInput` API
- System tray in taskbar notification area
- MSI or NSIS installer

### Linux
- Global hotkey via X11 (Wayland support varies)
- PulseAudio/PipeWire for audio capture via `cpal`
- System tray via `libappindicator`
- Wayland paste simulation fragile — clipboard-only fallback
- AppImage or .deb distribution

## Git Workflow
- `main` branch: stable releases
- `dev` branch: active development
- Feature branches: `feature/xxx`
- Commit messages: conventional commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`)

## Known Limitations & Future Work

- **Wayland**: paste simulation unreliable on some compositors — clipboard-only fallback
- **Streaming transcription**: v1 records full audio then sends. Future: streaming audio for real-time STT
- **On-device Whisper**: Future: `whisper-rs` (`whisper.cpp` bindings) for offline mode
- **Selected text editing**: Future: select text → hotkey → speak edit command → replace
- **Taiwanese Hokkien**: same status as Android — post-v1 research
