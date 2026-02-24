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

## Phase 0: Project Scaffold (Day 1-2)

### 0.1 Tauri Project Init
- [ ] `pnpm create tauri-app voxink-desktop` with React + TypeScript template
- [ ] Configure `tauri.conf.json`:
  - App identifier: `com.voxink.desktop`
  - Window: hidden by default (tray-only app)
  - Permissions: microphone, accessibility, global-shortcut, clipboard
- [ ] Add Tauri plugins:
  - `tauri-plugin-global-shortcut`
  - `tauri-plugin-store` (encrypted settings)
  - `tauri-plugin-sql` (SQLite for history)
  - `tauri-plugin-autostart` (launch at login)
- [ ] Add Rust dependencies in `Cargo.toml`:
  - `cpal` (audio capture)
  - `hound` (WAV encoding)
  - `reqwest` + `tokio` (HTTP + async)
  - `serde` + `serde_json` (serialization)
  - `arboard` (clipboard)
  - `enigo` (key simulation)
  - `thiserror` (error handling)
  - `mockall` + `wiremock` + `tokio` (test deps)
- [ ] Set up error types: `AppError` enum with `thiserror`
- [ ] Set up `PipelineState` enum (7 states, matching Android's `ImeUiState`)
- [ ] Verify: `cargo tauri dev` launches blank tray app

### 0.2 Basic System Tray
- [ ] Create tray icon (mic icon, idle state)
- [ ] Tray menu: Settings / Quit
- [ ] Click tray icon → open settings window
- [ ] App starts minimized to tray (no dock/taskbar window)

### Deliverable
Empty Tauri app living in system tray, with core types (`AppError`, `PipelineState`, `Language`, `RecordingMode`) defined and tested.

---

## Phase 1: Hotkey → Record → Transcribe → Paste (Week 1-2)

This is the entire core loop. Get this working end-to-end first.

### 1.1 Audio Module (Rust)
- [ ] `audio/recorder.rs`: Initialize `cpal` with default input device
  - PCM capture: 16kHz, mono, i16
  - Start/stop recording, return `Vec<i16>` PCM data
  - Handle: no microphone, permission denied, device disconnected
  - Max recording duration: 5 minutes, auto-stop
  - Min recording duration: <0.5s → discard
- [ ] `audio/encoder.rs`: PCM → WAV encoding
  - 44-byte RIFF header (same as Android `AudioEncoder.pcmToWav()`)
  - Pure Rust, no external lib for header (or use `hound`)
- [ ] Tests: encoder produces valid WAV headers, known PCM → expected bytes

### 1.2 API Module — Groq STT (Rust)
- [ ] `api/groq.rs`: `reqwest` multipart POST to Groq Whisper API
  - Send WAV audio, receive `WhisperResponse { text: String }`
  - Per-call `"Bearer {api_key}"` header
  - Models (user-selectable): `whisper-large-v3` / `whisper-large-v3-turbo` (default)
  - Response format: `verbose_json`
- [ ] `pipeline/transcribe.rs`: Orchestrate PCM → WAV → STT
  - Compose `encoder::pcm_to_wav()` + `groq::transcribe()`
  - Return `Result<String, AppError>`
- [ ] Language handling:
  - Auto: `language` = None, `prompt` = "繁體中文，可能夾雜英文。"
  - Chinese: `language` = "zh", `prompt` = "繁體中文轉錄。"
  - English: `language` = "en", `prompt` = "Transcribe the following English speech."
  - Japanese: `language` = "ja", `prompt` = "以下の日本語音声を文字起こししてください。"
- [ ] Error handling: 401 (bad key), 413 (too large), timeout, network error
- [ ] Tests: `wiremock` mock server for STT responses, error cases

### 1.3 Global Hotkey
- [ ] `hotkey.rs`: Register default hotkey via `tauri-plugin-global-shortcut`
  - `Cmd+Shift+V` (macOS) / `Ctrl+Shift+V` (Windows/Linux)
  - Detect press and release separately (hold-to-dictate)
- [ ] Handle conflicts: notify user if hotkey is taken
- [ ] macOS: request Accessibility permission on first run

### 1.4 Pipeline Controller (Rust)
- [ ] `pipeline/controller.rs`: State machine orchestrating the full flow
  - Mirrors Android's `RecordingController`
  - `on_start_recording()`: validate API key → start capture → emit `Recording`
  - `on_stop_recording()`: stop capture → emit `Processing` → transcribe → emit `Result`
  - Broadcast state changes via `tokio::sync::watch` + Tauri `app.emit()`
- [ ] Tests: verify state transition sequence (Idle → Recording → Processing → Result)
- [ ] Tests: verify error states (missing API key, network failure)

### 1.5 Auto-Paste (Rust)
- [ ] `input/clipboard.rs`: Save/restore clipboard content via `arboard`
- [ ] `input/paste.rs`: Simulate `Cmd+V`/`Ctrl+V` via `enigo`
- [ ] Paste strategy:
  1. Save current clipboard
  2. Write transcription to clipboard
  3. Simulate paste keystroke
  4. Wait 100ms
  5. Restore original clipboard
- [ ] Fallback: if simulation fails, clipboard-only + system notification

### 1.6 API Key Setup (Minimal)
- [ ] `storage.rs`: Encrypted store for API keys via Tauri store plugin
  - `get_api_key(provider)` / `set_api_key(provider, key)`
  - Key name: `groq_api_key` (matching Android)
- [ ] On first launch: open settings window prompting for Groq API key
- [ ] Validate key with test API call
- [ ] Masked display: first 4 + `••••` + last 4

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
