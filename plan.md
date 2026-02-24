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

### 1.1 Groq STT API Client ✅ COMPLETE
**File**: `voxink-core/src/api/groq.rs`
- [x] `SttConfig` struct with api_key, model, language, response_format
- [x] `WhisperResponse` struct for Groq API response
- [x] `transcribe()` async fn with reqwest multipart POST, per-call Bearer auth
- [x] HTTP error mapping: 401 → ApiKeyMissing, 413 → Transcription, 500 → Transcription
- [x] 5 wiremock tests: success, 401, 413, 500, custom model

### 1.2 Transcription Pipeline ✅ COMPLETE
**File**: `voxink-core/src/pipeline/transcribe.rs`
- [x] `transcribe()` composing encoder::pcm_to_wav + groq::transcribe
- [x] Empty PCM validation → AppError::Audio
- [x] 3 wiremock tests: encode+call, empty rejection, error propagation

### 1.3 Pipeline Controller (State Machine) ✅ COMPLETE
**File**: `voxink-core/src/pipeline/controller.rs`
- [x] `PipelineController` with `tokio::sync::watch` for state broadcast
- [x] `PipelineConfig` struct (API key, language, model)
- [x] `on_start_recording()`: validates API key → emits Recording (or Error)
- [x] `on_stop_recording(pcm)`: emits Processing → transcribe → Result (or Error)
- [x] `current_state()`, `subscribe()`, `update_config()`, `reset()`
- [x] 7 tests: idle start, recording transition, error on missing key, processing on stop, empty pcm, reset, config update

### 1.4 Audio Recorder Trait ✅ COMPLETE
**File**: `voxink-core/src/audio/recorder.rs`
- [x] `AudioRecorder` trait: start/stop/is_recording (mockall automock)
- [ ] cpal implementation (requires Tauri crate + system audio libs)

### 1.5 Auto-Paste ✅ COMPLETE
**File**: `voxink-core/src/input/clipboard.rs` + `input/paste.rs`
- [x] `ClipboardManager` trait: get_text/set_text (mockall automock)
- [x] `KeySimulator` trait: paste (mockall automock)
- [x] `paste_text()` orchestration: save → write → paste → sleep(100ms) → restore
- [x] 4 mock tests: save/restore, write+paste, graceful restore failure, paste failure propagation
- [ ] arboard + enigo concrete implementations (requires Tauri crate + system libs)

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

## Phase 2: LLM Refinement + Settings UI (Week 3-4) ✅ COMPLETE

### 2.1 LLM Refinement Pipeline (Rust) ✅ COMPLETE
- [x] `api/groq.rs`: Chat completion endpoint (`chat_completion()`, `ChatConfig`)
  - Models: `openai/gpt-oss-120b` (default) / `openai/gpt-oss-20b`
  - Temperature: `0.3`, max_tokens: `2048`
  - 5 wiremock tests: success, 401, 500, default model, empty choices
- [x] `pipeline/refine.rs`: Orchestrate text → LLM → refined text
  - Compose `prompts::for_language()` + `groq::chat_completion()`
  - 3 wiremock tests: success, empty rejection, error propagation
- [x] `LlmProvider` trait + updated `PipelineController<S, L>`
  - After STT success: if refinement enabled, emit `Refining` → call LLM → emit `Refined`
  - If refinement fails: fall back to `Result` with original text (graceful degradation)
  - 5s timeout via `tokio::time::timeout` — falls back to raw text
  - 5 new tests: refining→refined, fallback on failure, skip when disabled, timeout, debug masking
- [x] `pipeline/settings.rs`: `Settings` struct (Serialize/Deserialize/Default)

### 2.2 Settings Window (React) ✅ COMPLETE
- [x] Tabbed sidebar layout (General, Speech, Refinement, Appearance)
- [x] GeneralSection: hotkey display, recording mode, auto-paste, launch at login
- [x] SttSection: provider dropdown, API key (masked + save), language, model
- [x] RefinementSection: enable toggle, provider, API key, model
- [x] AppearanceSection: theme (system/light/dark), UI language
- [x] `useSettings` hook with debounced save
- [x] `src/lib/tauri.ts` invoke wrappers

### 2.3 Tray Menu Types ✅ COMPLETE
- [x] Settings struct with defaults (actual tray menu in Phase 1.6)

### Deliverable
Full-featured voice dictation with refinement and configurable settings.

---

## Phase 3: Floating Overlay + History (Week 5) ✅ COMPLETE

### 3.1 Floating Overlay Widget ✅ COMPLETE
- [x] Overlay React component with pipeline-state event listener
- [x] States: Recording (red pulse), Processing (spinner), Done (green check, auto-hide 1.5s), Error (red X, auto-hide 3s)
- [x] Semi-transparent background with backdrop-blur
- [ ] Tauri secondary window creation (requires Tauri integration, Phase 1.6)

### 3.2 Transcription History ✅ COMPLETE
- [x] `history.rs`: `TranscriptionEntry` struct with `display_text()` method
- [x] SQL schema constants (CREATE, INSERT, QUERY, SEARCH, DELETE, CLEANUP, COUNT)
- [x] History UI (React): search, expand/collapse, copy, delete with confirmation
- [x] History tab integrated into Settings sidebar
- [x] Language badges (colored pills per language)
- [ ] Actual SQLite integration (requires Tauri app crate, Phase 1.6)

### 3.3 Audio File Transcription ✅ COMPLETE
- [x] `audio/chunker.rs`: WAV-aware chunking for files > 25MB
  - Validates RIFF/WAVE headers, splits PCM data, updates chunk headers
  - 6 tests: single chunk, multi-chunk, header preservation, size updates, error cases
- [ ] File transcription UI (deferred to Phase 1.6 integration)

### Deliverable
Polished experience with visual feedback, history, and file transcription.

---

## Phase 4: UI Polish + Platform Testing (Week 6-7) — i18n + Theme COMPLETE

### 4.1 Visual Design ✅ COMPLETE
- [x] Clean, minimal settings window with sidebar navigation
- [x] Smooth animations in overlay (CSS pulse, spin transitions)
- [x] Dark mode support (follows system, manual override)
- [x] `useTheme` hook: system/light/dark with media query listener
- [x] App icons generated (all sizes, .icns, .ico) — completed in Phase 5

### 4.2 i18n ✅ COMPLETE
- [x] `react-i18next` + `i18next` setup
- [x] `en.json` + `zh-TW.json` complete (all UI strings)
- [x] All React components wired with `t()` translation calls
- [x] Language switching in AppearanceSection with `i18n.changeLanguage()`
- [ ] Tray menu localized (requires Tauri integration)
- [ ] System notifications localized (requires Tauri integration)

### 4.3 Platform Testing (requires Phase 1.6 — Tauri integration)
- [ ] **macOS**: accessibility/mic permissions, menu bar, code signing, DMG, Apple Silicon + Intel
- [ ] **Windows**: global hotkey, paste simulation, system tray, NSIS, Win 10+11
- [ ] **Linux**: X11/Wayland, paste simulation, AppImage, Ubuntu 22.04+

### 4.4 Edge Cases (requires Phase 1.6 — Tauri integration)
- [ ] No internet → clear error message
- [ ] Invalid API key → prompt to fix in settings
- [ ] Microphone in use → handle gracefully
- [ ] Very long recording (>5 min) → auto-stop + warn
- [ ] Very short recording (<0.5s) → ignore
- [ ] Rapid hotkey presses → debounce
- [ ] System sleep/wake → re-register hotkey

### Deliverable
Production-ready app tested on all three platforms.

---

## Phase 5: Distribution (Week 8) — Infrastructure COMPLETE

### 5.1 Platform Icons ✅ COMPLETE
- [x] Generated `icon.icns` (macOS), `icon.ico` (Windows) via `tauri icon`
- [x] All icon sizes: 32x32, 128x128, 128x128@2x, icon.png

### 5.2 Bundle Configuration ✅ COMPLETE
- [x] DMG with hardened runtime (macOS, min 10.15)
- [x] NSIS installer with currentUser mode, WebView2 bootstrapper (Windows)
- [x] AppImage + .deb (Linux)
- [x] Updater artifacts enabled (`createUpdaterArtifacts: "v1Compatible"`)

### 5.3 Auto-Update ✅ COMPLETE
- [x] `tauri-plugin-updater` + `tauri-plugin-process` added (Rust + JS)
- [x] Updater plugin registered in `lib.rs`
- [x] Updater + process permissions in `desktop.json` capabilities
- [x] `UpdateChecker` React component with download progress bar
- [x] i18n strings for update UI (en + zh-TW)
- [ ] Updater signing key generation (first real release)
- [ ] GitHub repo URL in updater endpoint (placeholder set)

### 5.4 CI/CD Pipelines ✅ COMPLETE
- [x] `.github/workflows/ci.yml`: test core + build frontend on push/PR
- [x] `.github/workflows/release.yml`: cross-platform build on tag push
  - macOS (arm64 + x64), Windows (x64), Linux (x64)
  - `tauri-apps/tauri-action` with Rust cache
  - Code signing env vars ready (secrets need to be set)
  - Draft releases with platform-specific asset descriptions
- [ ] Apple Developer account + code signing certificate
- [ ] Apple notarization credentials
- [ ] Windows code signing certificate (optional)

### 5.5 Build Scripts ✅ COMPLETE
- [x] `pnpm test:core` — run voxink-core tests
- [x] `pnpm lint:core` — run clippy on voxink-core
- [x] `pnpm tauri:build` / `pnpm tauri:dev` convenience scripts

### 5.6 Website / Landing Page (deferred)
- [ ] Product page with demo GIF/video
- [ ] Download links per platform
- [ ] Setup guide (get API key, install, configure)
- [ ] Homebrew cask formula (optional)

### Deliverable
Distribution infrastructure ready. First release requires: signing keys, Apple Developer account, and GitHub repo configuration.

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

## v1.0 Readiness Assessment

### What's DONE (Phases 0-5 software layer)

| Layer | Status | Details |
|-------|--------|---------|
| Core types & state machine | ✅ 89 tests | AppError, PipelineState, Language, RecordingMode |
| Groq STT API client | ✅ tested | Multipart POST, error mapping, wiremock |
| LLM refinement pipeline | ✅ tested | Chat completion, prompts, 5s timeout fallback |
| Transcription pipeline | ✅ tested | PCM→WAV→STT orchestration |
| Audio encoder | ✅ tested | 44-byte RIFF header, pure Rust |
| Audio chunker | ✅ tested | WAV-aware splitting for >25MB files |
| Pipeline controller | ✅ tested | State machine with SttProvider + LlmProvider traits |
| Auto-paste logic | ✅ tested | save→write→paste→restore with trait mocks |
| Hardware traits | ✅ defined | AudioRecorder, ClipboardManager, KeySimulator |
| Settings UI | ✅ built | 5-tab sidebar: General, Speech, Refinement, Appearance, History |
| Floating overlay | ✅ built | Recording/Processing/Done/Error states with animations |
| History UI | ✅ built | Search, expand, copy, delete with language badges |
| i18n | ✅ built | en + zh-TW, all components wired |
| Theme system | ✅ built | System/light/dark with media query listener |
| Auto-update UI | ✅ built | UpdateChecker with progress bar, bilingual |
| Bundle config | ✅ configured | DMG, NSIS, AppImage, .deb, updater artifacts |
| CI/CD | ✅ configured | GitHub Actions: CI + cross-platform release |
| App icons | ✅ generated | All sizes, .icns, .ico |

### What's NOT DONE (the gap to v1.0)

**Phase 1.6 — Tauri Integration** is the critical missing piece. Everything above is either pure Rust business logic (tested with mocks) or React UI (built but not wired to Rust). Phase 1.6 is the glue that makes it a real app:

| Missing Item | Blocked By | Effort |
|--------------|------------|--------|
| cpal audio recorder (concrete impl) | System audio libs | ~1 day |
| arboard clipboard + enigo paste (concrete impls) | System libs | ~0.5 day |
| Global hotkey registration | Tauri setup() | ~0.5 day |
| Hotkey → record → transcribe → paste wiring | All above | ~1 day |
| Pipeline state → Tauri events → React UI | Tauri emit/listen | ~0.5 day |
| IPC commands (save_api_key, get_api_key, test_api_key) | Tauri plugin-store | ~0.5 day |
| Settings persistence (React ↔ Rust) | IPC commands | ~0.5 day |
| SQLite history (actual DB, not just schema) | Tauri plugin-sql | ~0.5 day |
| Overlay as Tauri secondary window | Tauri window API | ~0.5 day |
| Tray icon state changes | Tauri tray API | ~0.5 day |
| Tray menu localization | Tauri integration | ~0.5 day |
| **Total** | **System libs required** | **~6 days** |

**Phase 4.3-4.4 — Testing & Edge Cases** (~1-2 weeks after integration):
- Platform-specific testing on macOS, Windows, Linux
- Edge case handling (no internet, mic busy, rapid hotkey, etc.)
- 1-week dogfooding period

**Phase 5 — External Setup** (non-code):
- Apple Developer account + signing certificate
- Updater signing key generation
- GitHub repo URL configuration

### Verdict: NOT v1.0 yet

**We're at ~70% of v1.0.** All the hard design work is done — the business logic is proven with 89 tests, the UI is built, the CI/CD is configured. But the app doesn't actually *work* yet because Phase 1.6 (Tauri integration) hasn't been done. It's like having a car engine + body + interior all manufactured and tested separately, but not assembled.

**Estimated remaining effort to v1.0:**
- Phase 1.6 (Tauri integration): ~1 week (requires system libs: libgtk-3-dev, libwebkit2gtk-4.1-dev, libasound2-dev)
- Phase 4.3-4.4 (testing + edge cases): ~1-2 weeks
- Phase 5 external setup: ~1 day
- **Total: ~2-3 weeks to v1.0**

## Success Metrics (v1.0)

- [ ] End-to-end latency < 3s (hotkey release → text pasted) on Groq
- [ ] Works in top 10 desktop apps: VS Code, Chrome, Slack, Notion, Word, LINE, Discord, Terminal, Notes, Mail
- [ ] Zero-crash in 1-week dogfooding
- [ ] macOS + Windows builds stable
- [ ] Auto-paste success rate > 95% on macOS and Windows
- [x] 繁中 + English UI complete
- [ ] Settings persist across restarts
- [ ] History searchable and exportable
- [x] Test coverage ≥ 80% for Rust pipeline modules (89 tests, all core modules covered)

---

## Timeline Summary

| Phase | Duration | Status | Deliverable |
|-------|----------|--------|-------------|
| Phase 0: Scaffold | 2 days | ✅ COMPLETE | Tray app + core types defined and tested |
| Phase 1.1-1.5: Core logic | 2 weeks | ✅ COMPLETE | STT, LLM, pipeline, state machine, paste (89 tests) |
| Phase 1.6-1.7: Tauri integration | ~1 week | ❌ BLOCKED | Hotkey → record → paste (needs system libs) |
| Phase 2: Refinement + Settings | 2 weeks | ✅ COMPLETE | LLM pipeline + full settings UI |
| Phase 3: Overlay + History | 1 week | ✅ COMPLETE | Overlay component + history UI + chunker |
| Phase 4.1-4.2: i18n + Theme | 1 week | ✅ COMPLETE | Bilingual UI + dark mode |
| Phase 4.3-4.4: Platform testing | ~2 weeks | ❌ BLOCKED | Needs Phase 1.6 first |
| Phase 5: Distribution infra | 1 week | ✅ COMPLETE | CI/CD, bundle config, auto-update, icons |
| **Total to v1.0** | **~2-3 weeks remaining** | **~70% done** | Blocked on Phase 1.6 (system libs) |
