# plan.md — VoxInk Desktop (語墨桌面版) Development Plan

## Vision

System-tray voice dictation for every desktop app. Press a hotkey, speak, text appears at your cursor. No input method switching, no app switching, no friction. Typeless/Wispr Flow experience with BYOK pricing.

## Market Context

| Product | Model | Price | Our Advantage |
|---------|-------|-------|---------------|
| Wispr Flow | SaaS, closed | $10/mo | BYOK = near-zero cost, cross-platform |
| Typeless | SaaS, closed | $12-30/mo | BYOK, open, desktop-native |
| macOS Dictation | Built-in, free | Free | Multi-provider, LLM refinement, multilingual |
| Whisper.cpp CLI | Open source | Free | GUI, auto-paste, refinement, no terminal needed |

**Positioning**: Wispr Flow UX + BYOK model + cross-platform (macOS/Windows/Linux)

## Prerequisites

- VoxInk Android v1 shipped (prompts, API patterns proven)
- Rust toolchain installed (`rustup`)
- Tauri v2 CLI installed (`cargo install tauri-cli`)
- Node.js + pnpm for React frontend

---

## Phase 0: Project Scaffold (Day 1-2) ✅ COMPLETE

### 0.1 Tauri Project Init
- [x] Tauri v2 project with React + TypeScript + Tailwind frontend
- [x] Configure `tauri.conf.json`: app id `com.voxink.desktop`, hidden settings window, tray icon
- [x] Capabilities: global-shortcut, store, sql, autostart permissions
- [x] All Rust dependencies in workspace `Cargo.toml` + `voxink-core/Cargo.toml`
- [x] `AppError` enum with `thiserror` (6 variants, serializable for Tauri)
- [x] `PipelineState` enum (7 states matching Android `ImeUiState`)
- [x] `Language` enum with `code()` and `prompt()` methods
- [x] `RecordingMode` enum (HoldToRecord default, Toggle)
- [x] `audio::encoder::pcm_to_wav()` — 44-byte RIFF header, pure Rust
- [x] `pipeline::prompts::for_language()` — 4 prompts verbatim from Android
- [x] API constants: timeouts, base URL matching Android

### 0.2 Basic System Tray
- [x] Tray icon (placeholder, idle state)
- [x] Tray menu: Settings / Quit
- [x] Click Settings → show settings window
- [x] App starts with hidden window (tray-only)

### Architecture Decision: Workspace Crate
Created `src-tauri/crates/voxink-core/` as a separate workspace crate containing all business logic (types, encoder, prompts, API modules). This crate has **no Tauri dependency** and can be built/tested independently without Linux system libraries (libgtk, libwebkit, etc.). The Tauri app crate depends on `voxink-core` and handles only framework glue (tray, hotkey, IPC commands).

### Verification
- 41 unit tests passing
- clippy clean (`-D warnings`)
- Frontend builds (Vite + React + Tailwind)
- Full Tauri binary build requires Linux dev libraries (libglib2.0-dev, libgtk-3-dev, libwebkit2gtk-4.1-dev, libasound2-dev)

### Deliverable
Empty Tauri app living in system tray, with core types (`AppError`, `PipelineState`, `Language`, `RecordingMode`) defined and tested.

---

## Phase 1: Hotkey → Record → Transcribe → Paste (Week 1-2)

This is the entire core loop. Get this working end-to-end first.

> **Note**: `audio/encoder.rs` (pcm_to_wav) already implemented and tested in Phase 0. Phase 1 focuses on the remaining pipeline modules.
> All new modules go in `src-tauri/crates/voxink-core/src/` (framework-independent, testable without Tauri).

### 1.1 Groq STT API Client
**File**: `voxink-core/src/api/groq.rs`
- [ ] Define `SttConfig` struct: `api_key`, `model` (default: `whisper-large-v3-turbo`), `language: Option<Language>`, `response_format` (default: `verbose_json`)
- [ ] Define `WhisperResponse` struct matching Groq API response
- [ ] Implement `pub async fn transcribe(config: &SttConfig, wav_data: &[u8]) -> Result<String, AppError>`
  - Build `reqwest::Client` with `CONNECT_TIMEOUT` / `READ_WRITE_TIMEOUT`
  - Multipart POST to `{GROQ_BASE_URL}openai/v1/audio/transcriptions`
  - Per-call `Authorization: Bearer {api_key}` header
  - Form fields: `file` (wav bytes), `model`, `response_format`, `language` (if set), `prompt` (from `Language::prompt()`)
- [ ] Map HTTP errors: 401 → `AppError::ApiKeyMissing`, 413 → `AppError::Transcription("file too large")`, timeout → `AppError::Network`
- [ ] Tests (wiremock):
  - `should_return_transcription_when_api_responds_successfully`
  - `should_return_api_key_error_on_401`
  - `should_return_transcription_error_on_413`
  - `should_include_language_param_when_set`
  - `should_omit_language_param_for_auto`

### 1.2 Transcription Pipeline
**File**: `voxink-core/src/pipeline/transcribe.rs`
- [ ] Define `TranscribeRequest` struct: `pcm_data: Vec<i16>`, `config: SttConfig`
- [ ] Implement `pub async fn transcribe(request: TranscribeRequest) -> Result<String, AppError>`
  - Compose: `encoder::pcm_to_wav(&request.pcm_data)` → `groq::transcribe(&config, &wav_data)`
- [ ] Validate: empty PCM → `AppError::Audio("no audio data")`
- [ ] Tests:
  - `should_encode_pcm_and_call_stt_api`
  - `should_reject_empty_pcm_data`

### 1.3 Pipeline Controller (State Machine)
**File**: `voxink-core/src/pipeline/controller.rs`
- [ ] Define `PipelineController` struct with:
  - `state_tx: tokio::sync::watch::Sender<PipelineState>` (state broadcast)
  - `state_rx: tokio::sync::watch::Receiver<PipelineState>` (for reading current state)
  - `config: PipelineConfig` (API key, language, model, etc.)
- [ ] Implement `PipelineController::new(config: PipelineConfig) -> Self`
- [ ] Implement `pub async fn on_start_recording(&self) -> Result<(), AppError>`
  - Validate API key present → emit `Recording`
  - If API key missing → emit `Error` + return `Err(AppError::ApiKeyMissing)`
- [ ] Implement `pub async fn on_stop_recording(&self, pcm_data: Vec<i16>) -> Result<String, AppError>`
  - Emit `Processing`
  - Call `pipeline::transcribe()` → on success emit `Result { text }`
  - On failure → emit `Error { message }` + return Err
- [ ] Implement `pub fn current_state(&self) -> PipelineState` (read from watch receiver)
- [ ] Tests:
  - `should_transition_idle_to_recording_on_start`
  - `should_emit_error_when_api_key_missing`
  - `should_transition_recording_to_processing_to_result`
  - `should_emit_error_state_on_transcription_failure`

### 1.4 Audio Recorder (cpal)
**File**: `voxink-core/src/audio/recorder.rs` (trait + types only)
**File**: `src-tauri/src/audio_recorder.rs` (cpal impl, Tauri-specific due to device access)
- [ ] Define trait in voxink-core: `pub trait AudioRecorder: Send + Sync`
  - `fn start(&self) -> Result<(), AppError>`
  - `fn stop(&self) -> Result<Vec<i16>, AppError>`
  - `fn is_recording(&self) -> bool`
- [ ] cpal implementation (in Tauri crate):
  - Default input device, 16kHz mono i16
  - Accumulate samples in `Arc<Mutex<Vec<i16>>>`
  - Handle: no microphone → `AppError::Audio("no input device")`
  - Max duration: 5 min auto-stop
  - Min duration: <0.5s → return empty (caller discards)
- [ ] Tests (trait-based, mockable):
  - `should_start_and_stop_recording`
  - `should_return_pcm_data_on_stop`

### 1.5 Auto-Paste (clipboard + key simulation)
**File**: `voxink-core/src/input/clipboard.rs` (trait)
**File**: `voxink-core/src/input/paste.rs` (orchestration)
- [ ] Define trait: `pub trait ClipboardManager: Send + Sync`
  - `fn get_text(&self) -> Result<Option<String>, AppError>`
  - `fn set_text(&self, text: &str) -> Result<(), AppError>`
- [ ] Define trait: `pub trait KeySimulator: Send + Sync`
  - `fn paste(&self) -> Result<(), AppError>` (simulate Cmd+V / Ctrl+V)
- [ ] Implement `pub fn paste_text(clipboard: &dyn ClipboardManager, keys: &dyn KeySimulator, text: &str) -> Result<(), AppError>`
  - Save → Write → Paste → Sleep(100ms) → Restore
- [ ] arboard + enigo impls in Tauri crate
- [ ] Tests (with mocks):
  - `should_save_and_restore_clipboard`
  - `should_write_text_and_simulate_paste`
  - `should_handle_clipboard_restore_failure_gracefully`

### 1.6 Tauri Integration (wire everything together)
**File**: `src-tauri/src/lib.rs` (update setup)
**File**: `src-tauri/src/commands.rs` (Tauri IPC commands)
- [ ] Register global hotkey in Tauri `setup()` via `tauri-plugin-global-shortcut`
- [ ] On hotkey press → `PipelineController::on_start_recording()` + start audio capture
- [ ] On hotkey release → stop capture → `PipelineController::on_stop_recording(pcm)` → auto-paste
- [ ] Emit `pipeline-state` Tauri events on each state change
- [ ] Tauri command: `save_api_key(provider, key)` → encrypted store
- [ ] Tauri command: `get_api_key(provider)` → read from store
- [ ] Tauri command: `test_api_key(provider, key)` → small test transcription

### 1.7 Minimal Visual Feedback
- [ ] Tray icon state changes: idle → recording (red) → processing (yellow) → done (green → idle)
- [ ] Optional: system notification on completion

### Deliverable
Working end-to-end: press hotkey → speak → text appears at cursor. This is a usable MVP.

---

## Phase 2: LLM Refinement + Settings UI (Week 3-4)

### 2.1 LLM Refinement Pipeline (Rust)
- [ ] `api/groq.rs`: Add chat completion endpoint
  - Models (user-selectable): `openai/gpt-oss-120b` (default) / `openai/gpt-oss-20b`
  - Temperature: `0.3`, max_tokens: `2048`
- [ ] `pipeline/prompts.rs`: 4 prompts (zh/en/ja/mixed) — identical to Android
  - `pub fn for_language(lang: &Language) -> &'static str`
- [ ] `pipeline/refine.rs`: Orchestrate text → LLM → refined text
  - Compose `prompts::for_language()` + `groq::chat_completion()`
  - Return `Result<String, AppError>`
- [ ] Update `PipelineController`:
  - After STT success: if refinement enabled, emit `Refining` → call LLM → emit `Refined`
  - If refinement fails: fall back to `Result` with original text (graceful degradation)
  - Timeout: if LLM takes > 5s, paste raw text
- [ ] Tests: mock LLM responses, verify fallback on failure, verify correct prompt per language

### 2.2 Settings Window (React)
- [ ] **General**:
  - Hotkey configuration (click to record new shortcut)
  - Recording mode: hold vs toggle
  - Auto-paste: on/off
  - Launch at login: on/off
- [ ] **STT**:
  - Provider: Groq / OpenAI / Custom
  - API key (masked, with test button)
  - Language: Auto / 中文 / English / 日本語
- [ ] **Refinement**:
  - On/off toggle
  - Provider: Groq / OpenAI / Anthropic / Custom
  - API key + model selector
  - Custom prompt editor (per language)
- [ ] **Appearance**:
  - Theme: system / light / dark
  - UI Language: 繁體中文 / English
- [ ] All settings saved to Tauri store (encrypted)
- [ ] Changes take effect immediately (no restart)

### 2.3 Tray Menu Expansion
- [ ] Quick language switch: Auto / 中文 / English / 日本語
- [ ] Quick refinement toggle: On / Off
- [ ] Status line: "Ready" / "Recording..." / "Processing..."
- [ ] Open History / Open Settings

### Deliverable
Full-featured voice dictation with refinement and configurable settings.

---

## Phase 3: Floating Overlay + History (Week 5)

### 3.1 Floating Overlay Widget
- [ ] Tauri secondary window: small, frameless, always-on-top, click-through
- [ ] States:
  - **Recording**: red pulsing circle + audio level bars
  - **Processing**: spinner + "Transcribing..."
  - **Done**: brief green checkmark (auto-hide after 1s)
  - **Error**: brief red X + error message
- [ ] Position: anchored to screen corner (user-configurable)
- [ ] Appears on recording start, hides after completion
- [ ] Does NOT steal focus from the active app

### 3.2 Transcription History
- [ ] SQLite database via Tauri SQL plugin or `rusqlite`
- [ ] Schema: `id`, `timestamp`, `original_text`, `refined_text` (nullable), `language`, `audio_duration_ms`, `provider`
- [ ] `display_text` = `refined_text` if present, else `original_text` (matching Android `TranscriptionEntity.displayText`)
- [ ] History window (React):
  - List view with search/filter
  - Click entry → original + refined side by side
  - Copy button per entry
  - Delete single / clear all
- [ ] Auto-cleanup: configurable retention (7/30/90 days, or keep all)
- [ ] Export: JSON or plain text

### 3.3 Audio File Transcription
- [ ] `audio/chunker.rs`: WAV-aware chunking for files > 25MB
  - Read WAV header (channels, sample rate, bits per sample)
  - Split PCM data into chunks, prepend header to each
  - Same logic as Android `AudioChunker`
- [ ] File transcription window (React):
  - Drag-and-drop or file picker
  - Progress bar per chunk
  - Result display + copy/export (plain text, SRT)
  - Optional LLM refinement toggle

### Deliverable
Polished experience with visual feedback, history, and file transcription.

---

## Phase 4: UI Polish + Platform Testing (Week 6-7)

### 4.1 Visual Design
- [ ] App icon (tray icon + app icon)
- [ ] Consistent design language with Android version
- [ ] Smooth animations in overlay (CSS transitions)
- [ ] Dark mode support (follows system)
- [ ] Settings window: clean, modern, minimal

### 4.2 i18n
- [ ] `react-i18next` setup
- [ ] `en.json` + `zh-TW.json` complete
- [ ] Tray menu localized
- [ ] System notifications localized

### 4.3 Platform Testing
- [ ] **macOS** (primary):
  - Accessibility permission flow
  - Microphone permission flow
  - Menu bar behavior (hide dock icon)
  - Code signing + notarization
  - DMG installer
  - Test on Apple Silicon + Intel
- [ ] **Windows**:
  - Global hotkey via RegisterHotKey
  - Paste simulation via SendInput
  - System tray in notification area
  - NSIS/MSI installer
  - Test on Windows 10 + 11
- [ ] **Linux** (secondary):
  - X11 vs Wayland differences
  - Paste simulation (xdotool for X11, wl-copy for Wayland)
  - AppImage packaging
  - Test on Ubuntu 22.04+

### 4.4 Edge Cases
- [ ] No internet → clear error message
- [ ] Invalid API key → prompt to fix in settings
- [ ] Microphone in use by other app → handle gracefully
- [ ] Very long recording (>5 min) → auto-stop + warn
- [ ] Very short recording (<0.5s) → ignore, don't send to API
- [ ] Rapid hotkey presses → debounce
- [ ] System sleep/wake → re-register hotkey

### Deliverable
Production-ready app tested on all three platforms.

---

## Phase 5: Distribution (Week 8)

### 5.1 macOS Distribution
- [ ] Apple Developer account
- [ ] Code signing certificate
- [ ] Notarization via `xcrun notarytool`
- [ ] DMG with drag-to-Applications
- [ ] Homebrew cask formula (optional)
- [ ] Website download page

### 5.2 Windows Distribution
- [ ] Code signing certificate (optional but recommended)
- [ ] NSIS installer or MSI
- [ ] Website download page
- [ ] Windows Defender SmartScreen: may need EV certificate for trust

### 5.3 Linux Distribution
- [ ] AppImage (universal)
- [ ] .deb package for Debian/Ubuntu
- [ ] Flathub (optional, future)

### 5.4 Auto-Update
- [ ] Tauri updater plugin
- [ ] Check for updates on launch (configurable)
- [ ] Notify user, download in background, apply on restart

### 5.5 Website / Landing Page
- [ ] Product page with demo GIF/video
- [ ] Download links per platform
- [ ] Setup guide (get API key, install, configure)
- [ ] FAQ

### Deliverable
VoxInk Desktop v1.0 available for download on all platforms.

---

## Phase 6: Post-Launch (Month 2+)

### 6.1 Selected Text Editing (Typeless Feature)
- [ ] User selects text in any app
- [ ] Press different hotkey (e.g., `Cmd+Shift+E`)
- [ ] Speak editing command: "翻成英文" / "make it shorter" / "改成正式語氣"
- [ ] App reads selected text (simulate `Cmd+C`)
- [ ] Send to LLM: selected text + voice command
- [ ] Replace selection with LLM output (simulate paste)

### 6.2 Streaming Transcription
- [ ] Send audio chunks while recording (real-time STT)
- [ ] Display partial transcription in overlay
- [ ] Finalize on recording stop

### 6.3 On-Device Whisper (Offline)
- [ ] Integrate `whisper.cpp` via `whisper-rs` crate
- [ ] Download model on first use (~1.5GB for large-v3)
- [ ] Fallback when offline
- [ ] User choice: cloud (faster, needs internet) vs local (private, slower)

### 6.4 Additional Languages
- [ ] Add ko, es, fr, de, th, vi with refinement prompts
- [ ] Whisper STT works out-of-box, only prompts need tuning

### 6.5 Taiwanese Hokkien
- [ ] Same research as Android: 意傳科技 API, 教育部 ASR engine
- [ ] If API available, integrate as custom STT provider

### 6.6 Sync with Android
- [ ] Shared prompt library (export/import JSON)
- [ ] Shared history (optional cloud sync, user-hosted)

---

## Technical Decisions Log

### Why a separate `voxink-core` crate?
1. **Testability**: Core business logic (encoder, API client, state machine, prompts) can be tested without Linux system libraries (libgtk, libwebkit, etc.)
2. **Separation of concerns**: Framework-agnostic code (pure Rust) vs framework glue (Tauri setup, IPC commands, device access)
3. **Faster iteration**: `cargo test -p voxink-core` runs in seconds, no Tauri build needed
4. **Trait boundaries**: Hardware-dependent code (audio, clipboard, keys) uses traits in voxink-core, with concrete impls in the Tauri crate
5. **Future portability**: Core crate could be reused in CLI, other frameworks

### Why Tauri over Electron?
1. **Bundle size**: Tauri ~5-15MB vs Electron ~150MB+
2. **Memory usage**: Tauri uses system webview vs Electron's bundled Chromium
3. **Rust backend**: system-level operations (hotkey, paste simulation, audio) are natural in Rust
4. **Startup speed**: Tauri apps launch near-instantly
5. **Security**: Tauri's permission model is stricter than Electron

### Why Rust-side API calls (not React)?
API calls contain secret keys. Running them in Rust means:
1. Keys never enter the webview/JS context
2. No CORS issues (Rust HTTP client is not browser-bound)
3. Can be called without any window open (tray-only mode)
4. Better error handling and retry logic in Rust

### Why not share code with Android (KMP)?
The overlap between Android IME and desktop tray app is minimal:
- **Shared**: API endpoint URLs, prompt text, language list
- **Not shared**: UI (Compose vs React), audio recording (AudioRecord vs cpal), system integration (IME vs hotkey+paste), architecture (MVVM vs Tauri commands)

Copying prompts and API patterns is simpler than maintaining a shared KMP module.

### Why mirror Android's architecture patterns?
1. **Proven patterns**: Android v1 shipped successfully with these patterns
2. **Consistency**: Same team maintains both codebases — familiar mental model
3. **Portability**: Same state machine, same API contracts, same error handling strategy
4. **Testing**: Same TDD approach, same test naming, same coverage targets

### Hold-to-dictate vs Toggle
Default is hold-to-dictate because:
1. Matches muscle memory (like walkie-talkie)
2. Natural endpoint — release = done
3. Prevents accidental long recordings
4. Faster for short dictations (most common use case)

Toggle mode available for accessibility and long dictations.

### Paste Simulation Strategy
```
1. Read current clipboard → save to temp
2. Write transcription to clipboard
3. Simulate Cmd+V / Ctrl+V
4. Wait 100ms
5. Restore original clipboard from temp
```
Same as Typeless/Wispr Flow/1Password. Fragile on some Linux Wayland compositors but reliable on macOS and Windows.

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| macOS Accessibility permission UX | High | Clear first-run permission flow |
| Paste simulation fails in some apps | Medium | Clipboard-only fallback + notification |
| Wayland paste issues (Linux) | Medium | Detect Wayland → clipboard-only mode |
| Apple notarization rejection | Medium | Follow guidelines strictly, no private APIs |
| Windows SmartScreen warning | Medium | Consider EV code signing certificate |
| cpal audio device issues | Low | Fallback to system default, clear error messages |
| Tauri v2 breaking changes | Low | Pin versions, follow release notes |

---

## Success Metrics (v1.0)

- [ ] End-to-end latency < 3s (hotkey release → text pasted) on Groq
- [ ] Works in top 10 desktop apps: VS Code, Chrome, Slack, Notion, Word, LINE, Discord, Terminal, Notes, Mail
- [ ] Zero-crash in 1-week dogfooding
- [ ] macOS + Windows builds stable
- [ ] Auto-paste success rate > 95% on macOS and Windows
- [ ] 繁中 + English UI complete
- [ ] Settings persist across restarts
- [ ] History searchable and exportable
- [ ] Test coverage ≥ 80% for Rust pipeline modules

---

## Timeline Summary

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Phase 0: Scaffold | 2 days | Tray app + core types defined and tested |
| Phase 1: Core loop | 2 weeks | Hotkey → speak → text pasted (MVP) |
| Phase 2: Refinement + Settings | 2 weeks | LLM polish + full settings UI |
| Phase 3: Overlay + History | 1 week | Visual feedback + transcription log |
| Phase 4: Polish + Testing | 2 weeks | Platform-tested, production-ready |
| Phase 5: Distribution | 1 week | Installers + website + auto-update |
| **Total** | **~8 weeks** | **VoxInk Desktop v1.0** |
