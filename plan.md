# plan.md — VoiceInk Desktop (語墨桌面版) Development Plan

## Vision

System-tray voice dictation for every desktop app. Press a hotkey, speak, text appears at your cursor. No input method switching, no app switching, no friction. Typeless/Wispr Flow experience with BYOK pricing.

## Prerequisites

- VoiceInk Android v1 shipped (prompts, API patterns proven)
- Rust toolchain installed
- Tauri v2 CLI installed
- Node.js + pnpm for React frontend

---

## Phase 0: Project Scaffold (Day 1-2)

### 0.1 Tauri Project Init
- [ ] `pnpm create tauri-app voiceink-desktop` with React + TypeScript template
- [ ] Configure `tauri.conf.json`:
  - App identifier: `com.voiceink.desktop`
  - Window: hidden by default (tray-only app)
  - Permissions: microphone, accessibility, global-shortcut, clipboard
- [ ] Add Tauri plugins:
  - `tauri-plugin-global-shortcut`
  - `tauri-plugin-store` (encrypted settings)
  - `tauri-plugin-shell` (if needed)
  - `tauri-plugin-sql` (SQLite for history)
- [ ] Add Rust dependencies in `Cargo.toml`:
  - `cpal` (audio capture)
  - `reqwest` + `tokio` (HTTP + async)
  - `serde` + `serde_json` (serialization)
  - `arboard` (clipboard)
  - `enigo` (key simulation)
  - `hound` (WAV encoding)
  - `thiserror` (error handling)
- [ ] Verify: `cargo tauri dev` launches blank tray app on Mac/Win

### 0.2 Basic System Tray
- [ ] Create tray icon (mic icon, idle state)
- [ ] Tray menu: Settings / Quit
- [ ] Click tray icon → open settings window
- [ ] App starts minimized to tray (no dock/taskbar window)
- [ ] Launch at login option (Tauri auto-start plugin)

### Deliverable
Empty Tauri app living in system tray, launches on all three platforms.

---

## Phase 1: Hotkey → Record → Transcribe → Paste (Week 1-2)

This is the entire core loop. Get this working end-to-end first.

### 1.1 Global Hotkey
- [ ] Register default hotkey: `Cmd+Shift+V` (Mac) / `Ctrl+Shift+V` (Win/Linux)
- [ ] Detect press and release events separately (for hold-to-dictate)
- [ ] Hotkey works globally, even when app is not focused
- [ ] Handle conflicts gracefully (notify user if hotkey is taken)
- [ ] macOS: request Accessibility permission on first run

### 1.2 Audio Recording (Rust)
- [ ] Initialize `cpal` with default input device
- [ ] On hotkey press → start capture (PCM, 16kHz, mono)
- [ ] On hotkey release → stop capture
- [ ] Encode PCM → WAV in memory (using `hound`)
- [ ] Handle edge cases: no microphone, permission denied, device disconnected
- [ ] Max recording duration: 5 minutes (configurable), auto-stop with warning

### 1.3 Groq Whisper STT (Rust)
- [ ] `reqwest` multipart POST to Groq API
- [ ] Send WAV audio, receive transcription
- [ ] Language modes: Auto / zh / en / ja
- [ ] Mixed-language: omit `language` param + prompt hint
- [ ] Handle errors: 401 (bad key), 413 (file too large), timeout, network error
- [ ] Return raw transcription text

### 1.4 Auto-Paste (Rust)
- [ ] Save current clipboard content
- [ ] Put transcription text into clipboard
- [ ] Simulate `Cmd+V` (Mac) / `Ctrl+V` (Win/Linux) using `enigo`
- [ ] Restore previous clipboard content (after short delay)
- [ ] Fallback: if simulation fails, show notification "Text copied to clipboard"

### 1.5 Minimal Visual Feedback
- [ ] Tray icon changes state: idle → recording (red) → processing (yellow) → done (green, then back to idle)
- [ ] macOS: tray icon is sufficient for MVP
- [ ] Optional: system notification on completion (configurable)

### 1.6 API Key Setup (Minimal)
- [ ] On first launch: open settings window prompting for Groq API key
- [ ] Store encrypted via Tauri store plugin
- [ ] Validate key with test API call

### Deliverable
Working end-to-end: press hotkey → speak → text appears at cursor. This is a usable MVP.

---

## Phase 2: LLM Refinement + Settings UI (Week 3-4)

### 2.1 LLM Refinement Pipeline (Rust)
- [ ] After STT, optionally send raw text to LLM
- [ ] Providers: Groq LLaMA, OpenAI GPT, Anthropic Claude, custom
- [ ] Per-language refinement prompts (zh-TW, en, ja, mixed)
- [ ] Pipeline: STT → raw text → LLM → refined text → paste
- [ ] If refinement fails, fall back to pasting raw text
- [ ] Timeout: if LLM takes > 5s, paste raw text + refine in background

### 2.2 Settings Window (React)
- [ ] **General**:
  - Hotkey configuration (click to record new shortcut)
  - Recording mode: hold vs toggle
  - Auto-paste: on/off
  - Launch at login: on/off
  - Overlay position
- [ ] **STT**:
  - Provider: Groq / OpenAI / Custom
  - API key (masked, with test button)
  - Language: Auto / 中文 / English / 日本語
- [ ] **Refinement**:
  - On/off toggle
  - Provider: Groq / OpenAI / Anthropic / Custom
  - API key
  - Model selector
  - Custom prompt editor (per language)
- [ ] **Appearance**:
  - Theme: system / light / dark
  - Language: 繁體中文 / English
- [ ] All settings saved to encrypted store
- [ ] Changes take effect immediately (no restart needed)

### 2.3 Tray Menu Expansion
- [ ] Quick language switch: Auto / 中文 / English / 日本語
- [ ] Quick refinement toggle: On / Off
- [ ] Status line: "Ready" / "Recording..." / "Processing..."
- [ ] Open History
- [ ] Open Settings

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
- [ ] Minimal: ~80px circle or ~200px pill shape

### 3.2 Transcription History
- [ ] SQLite database via Tauri SQL plugin
- [ ] Schema: id, timestamp, original_text, refined_text, language, audio_duration_ms, provider
- [ ] History window (React):
  - List view with search/filter
  - Click entry → see original + refined side by side
  - Copy button per entry
  - Delete single / clear all
- [ ] Auto-cleanup: configurable retention (7/30/90 days, or keep all)
- [ ] Export: all history as JSON or plain text

### 3.3 Audio File Transcription
- [ ] File transcription window (React)
- [ ] Drag-and-drop or file picker
- [ ] Chunking for files > 25MB (Rust-side splitting)
- [ ] Progress bar per chunk
- [ ] Result display + copy/export
- [ ] Optional LLM refinement toggle

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

### 5.4 Website / Landing Page
- [ ] Product page with demo GIF/video
- [ ] Download links per platform
- [ ] Setup guide (get API key, install, configure)
- [ ] FAQ

### 5.5 Auto-Update
- [ ] Tauri updater plugin
- [ ] Check for updates on launch (configurable)
- [ ] Notify user, download in background, apply on restart

### Deliverable
VoiceInk Desktop v1.0 available for download on all platforms.

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
- [ ] Integrate `whisper.cpp` via Rust bindings (`whisper-rs`)
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

Copying prompts and API patterns is simpler than maintaining a shared KMP module. The two apps will diverge in UX anyway.

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
This is how Typeless/Wispr Flow/1Password all do it. Fragile on some Linux Wayland compositors but works reliably on macOS and Windows.

---

## Risk Assessment

| Risk | Impact | Mitigation |
|------|--------|------------|
| macOS Accessibility permission UX | High | Clear onboarding flow with screenshots |
| Paste simulation fails in some apps | Medium | Clipboard-only fallback + notification |
| Wayland paste issues (Linux) | Medium | Detect Wayland → clipboard-only mode |
| Apple notarization rejection | Medium | Follow guidelines strictly, no private APIs |
| Windows SmartScreen warning | Medium | Consider EV code signing certificate |
| cpal audio device issues | Low | Fallback to system default, clear error messages |

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

---

## Timeline Summary

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Phase 0: Scaffold | 2 days | Tray app runs on all platforms |
| Phase 1: Core loop | 2 weeks | Hotkey → speak → text pasted (MVP) |
| Phase 2: Refinement + Settings | 2 weeks | LLM polish + full settings UI |
| Phase 3: Overlay + History | 1 week | Visual feedback + transcription log |
| Phase 4: Polish + Testing | 2 weeks | Platform-tested, production-ready |
| Phase 5: Distribution | 1 week | Installers + website + auto-update |
| **Total** | **~8 weeks** | **VoiceInk Desktop v1.0** |
