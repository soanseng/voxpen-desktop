# CLAUDE.md — VoiceInk Desktop (語墨桌面版)

## Project Overview

VoiceInk Desktop is a system-tray voice-to-text app for macOS, Windows, and Linux. Users press a global hotkey to dictate, and the transcribed + refined text is automatically pasted at the cursor position in any app. Built with Tauri v2 (Rust backend + React frontend), BYOK model.

**App Name**: VoiceInk Desktop (語墨)
**Repo**: `voiceink-desktop`
**Relationship to Android**: Separate codebase. Shares API logic concepts and refinement prompts, but no code-level dependency.

## Tech Stack

- **Framework**: Tauri v2
- **Backend**: Rust
- **Frontend**: React + TypeScript + Tailwind CSS
- **Build**: Cargo (Rust) + Vite (frontend)
- **Audio**: `cpal` crate (cross-platform audio capture)
- **Global Hotkey**: Tauri's `global-shortcut` plugin
- **System Tray**: Tauri's `tray-icon` plugin
- **Clipboard / Paste Simulation**: `arboard` crate + `enigo` crate
- **Local Storage**: Tauri's `store` plugin (JSON file, encrypted for API keys)
- **HTTP Client**: `reqwest` crate (Rust-side API calls)
- **Audio Encoding**: `opus` or WAV encoding in Rust

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
    │   Encode Audio       │  ← Rust: PCM → WAV/Opus
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
- Visual feedback: floating overlay widget (small, corner-anchored)

### 2. Floating Overlay (Recording Indicator)
- Appears when recording starts
- Shows:
  - 🔴 Recording state with audio level meter / waveform
  - ⏳ Processing state (spinner)
  - ✅ Done (brief flash, then auto-hide)
- Always-on-top, click-through (doesn't steal focus)
- Position: user-configurable (corner of screen)
- Minimal size: ~80x80px circle or pill shape

### 3. Auto-Paste
- After transcription + refinement, text goes to clipboard
- Simulate `Cmd+V` (Mac) / `Ctrl+V` (Win/Linux) to paste at cursor
- Preserve previous clipboard content (save → paste → restore)
- Fallback: if paste simulation fails, just put in clipboard and notify user
- User option: auto-paste vs clipboard-only

### 4. STT Providers (BYOK)
- **Groq Whisper** (primary): `whisper-large-v3-turbo`
- **OpenAI Whisper**: `whisper-1`, `gpt-4o-transcribe`
- **Custom Server**: user-defined endpoint

#### Supported Languages (v1)
- **Auto-detect** (default): best for mixed-language
- **中文** (zh-TW): Traditional Chinese with prompt bias
- **English** (en)
- **日本語** (ja)
- Mixed-language (中英混合) works with auto-detect + prompt hint

### 5. LLM Refinement (BYOK)
- Same refinement pipeline as Android version
- **Providers**: Groq (LLaMA), OpenAI (GPT), Anthropic (Claude), custom endpoint
- **Features**:
  - Remove filler words (per-language)
  - Self-correction detection
  - Auto-punctuation
  - Custom system prompts
  - Translation mode
  - Mixed-language aware
- **Desktop-specific**: no dual-choice UI needed — just paste refined text
  - User can `Cmd+Z` to undo if they prefer original
  - Original always available in history

### 6. System Tray Menu
```
VoiceInk 🎤
├── Listening... (status)
├── ─────────────
├── Transcription History  →  (opens window)
├── Settings               →  (opens window)
├── ─────────────
├── Language: Auto-detect   →  中文 / English / 日本語 / Auto
├── Refinement: On          →  On / Off
├── ─────────────
├── About VoiceInk
└── Quit
```

### 7. Settings Window (React UI)
- **Hotkey configuration**: record/change global shortcut
- **Recording mode**: hold vs toggle
- **STT provider**: Groq / OpenAI / Custom
- **LLM provider**: Groq / OpenAI / Anthropic / Custom
- **API keys**: per-provider, masked display, encrypted storage
- **Language**: Auto / 中文 / English / 日本語
- **Refinement**: on/off toggle
- **Custom prompts**: edit/save refinement system prompts
- **Auto-paste**: on/off (fallback: clipboard only)
- **Overlay position**: top-left / top-right / bottom-left / bottom-right
- **Launch at login**: on/off
- **Theme**: system / light / dark
- **i18n**: 繁體中文 / English

### 8. Transcription History
- Local log of all transcriptions (SQLite via Tauri)
- Each entry: timestamp, original text, refined text, language, duration
- Search / filter
- Click to copy
- Export as text file
- Auto-cleanup: keep last N days (configurable)

### 9. Audio File Transcription
- Drag-and-drop or file picker for audio/video files
- Same chunking logic as Android (>25MB auto-split)
- Progress bar, export as text/SRT
- Optional LLM refinement

## Project Structure

```
voiceink-desktop/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── src/
│   │   ├── main.rs                    # Tauri entry point
│   │   ├── lib.rs                     # Module exports
│   │   ├── audio/
│   │   │   ├── mod.rs
│   │   │   ├── recorder.rs            # cpal-based audio capture
│   │   │   ├── encoder.rs             # PCM → WAV/Opus
│   │   │   └── chunker.rs             # Split large files
│   │   ├── api/
│   │   │   ├── mod.rs
│   │   │   ├── groq.rs                # Groq Whisper + LLM
│   │   │   ├── openai.rs              # OpenAI Whisper + GPT
│   │   │   ├── anthropic.rs           # Claude API
│   │   │   └── custom.rs              # User-defined endpoint
│   │   ├── pipeline/
│   │   │   ├── mod.rs
│   │   │   ├── transcribe.rs          # STT pipeline
│   │   │   ├── refine.rs              # LLM refinement pipeline
│   │   │   └── prompts.rs             # Default prompts per language
│   │   ├── input/
│   │   │   ├── mod.rs
│   │   │   ├── clipboard.rs           # Clipboard save/restore/paste
│   │   │   └── simulate.rs            # Simulate Cmd+V / Ctrl+V
│   │   ├── tray.rs                    # System tray setup & menu
│   │   ├── hotkey.rs                  # Global shortcut registration
│   │   ├── overlay.rs                 # Floating widget commands
│   │   ├── storage.rs                 # Encrypted settings + history DB
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
│   │   ├── useTauriEvents.ts          # Listen to Rust events
│   │   └── useSettings.ts
│   ├── lib/
│   │   ├── tauri.ts                   # Tauri invoke wrappers
│   │   └── i18n.ts                    # i18n setup
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
#[tauri::command]
fn get_settings() -> Settings { ... }

#[tauri::command]
fn save_settings(settings: Settings) -> Result<(), String> { ... }

#[tauri::command]
fn save_api_key(provider: String, key: String) -> Result<(), String> { ... }

#[tauri::command]
fn get_history(limit: u32, offset: u32) -> Vec<TranscriptionEntry> { ... }

#[tauri::command]
fn delete_history_entry(id: String) -> Result<(), String> { ... }

#[tauri::command]
fn transcribe_file(path: String) -> Result<TranscriptionResult, String> { ... }

#[tauri::command]
fn set_hotkey(shortcut: String) -> Result<(), String> { ... }

#[tauri::command]
fn test_api_key(provider: String, key: String) -> Result<bool, String> { ... }
```

```rust
// Rust → React events (via emit)
app.emit("recording-started", ());
app.emit("recording-stopped", AudioInfo { duration, size });
app.emit("transcription-progress", TranscriptionProgress { stage, text });
app.emit("transcription-complete", TranscriptionResult { original, refined });
app.emit("error", ErrorInfo { message, code });
```

## API Interaction

Same as Android version:

### Groq Whisper STT
```
POST https://api.groq.com/openai/v1/audio/transcriptions
Headers: Authorization: Bearer {GROQ_API_KEY}
Body (multipart/form-data):
  file: audio.wav
  model: whisper-large-v3-turbo
  language: zh (or en, ja, omit for auto-detect)
  response_format: verbose_json
  prompt: "繁體中文轉錄。" (for zh)
         "繁體中文，可能夾雜英文。" (for mixed, use auto-detect)
```

### LLM Refinement
```
POST https://api.groq.com/openai/v1/chat/completions
Body:
  model: llama-3.3-70b-versatile
  messages:
    - system: [refinement system prompt per language]
    - user: [raw transcription]
  temperature: 0.3
  max_tokens: 2048
```

## Refinement Prompts

Same prompts as Android version (zh-TW, en, ja, mixed-language). Defined in `src-tauri/src/pipeline/prompts.rs` and user-customizable via settings.

## Platform-Specific Notes

### macOS
- Requires **Accessibility permission** (System Preferences → Privacy → Accessibility) for:
  - Global hotkey capture
  - Simulating `Cmd+V` paste
- Requires **Microphone permission**
- App lives in menu bar (no dock icon option)
- Code signing + notarization needed for distribution
- DMG installer

### Windows
- Global hotkey via Windows API (RegisterHotKey)
- Paste simulation via `SendInput` API
- Microphone access prompt on first use
- System tray in taskbar notification area
- MSI or NSIS installer
- Optional: Windows Store distribution

### Linux
- Global hotkey via X11/Wayland (may need `xdotool` fallback for Wayland paste)
- PulseAudio/PipeWire for audio capture
- System tray via `libappindicator`
- AppImage or .deb distribution
- Note: Wayland paste simulation is fragile — may need clipboard-only mode

## Development Guidelines

### Rust Code Style
- Follow Rust 2021 edition idioms
- Use `thiserror` for error types
- Use `tokio` for async runtime
- Use `serde` for serialization
- Prefer `Result<T, E>` over panics
- `clippy` for linting, `rustfmt` for formatting

### React/TypeScript Code Style
- Functional components with hooks
- TypeScript strict mode
- Tailwind for styling
- `@tanstack/react-query` for data fetching if needed
- Minimal dependencies — keep bundle small

### Key Conventions
- All API calls happen in Rust (not in React frontend)
- React only handles UI rendering and invokes Rust commands
- Settings stored as encrypted JSON (Tauri store plugin)
- History stored in SQLite (via `rusqlite` or Tauri SQL plugin)
- No analytics, no telemetry, no phone-home

### Git Workflow
- `main` branch: stable releases
- `dev` branch: active development
- Feature branches: `feature/xxx`
- Conventional commits

## Known Limitations & Future Work

- **Wayland**: paste simulation unreliable on some Wayland compositors, clipboard-only fallback
- **Streaming transcription**: v1 records full audio then sends. Future: streaming audio to API for real-time transcription
- **On-device Whisper**: Future: `whisper.cpp` integration for offline mode
- **Selected text editing**: Future: select text → hotkey → speak edit command → replace (like Typeless)
- **Taiwanese Hokkien**: same status as Android — post-v1 research
- **iOS companion**: Future consideration
