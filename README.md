# VoxPen Desktop (語墨桌面版)

[繁體中文](./README.zh-TW.md)

System-tray voice-to-text app for Windows and Linux. Press a global hotkey to dictate, and the transcribed + refined text is automatically pasted at the cursor position in any app.

Built with **Tauri v2** (Rust backend + React frontend), BYOK (Bring Your Own Key).

## Features

- **Global hotkey** — works system-wide in any app, no input method switching
- **Hold-to-dictate** or **toggle** recording mode
- **STT providers** — Groq Whisper, OpenAI Whisper, or custom endpoint
- **LLM refinement** — auto-remove filler words, fix grammar, add punctuation
- **Auto-paste** — transcription goes straight to cursor position
- **Translation mode** — translate speech to a target language
- **Multi-language** — Auto-detect, 中文, English, 日本語
- **Floating overlay** — recording/processing status indicator
- **Transcription history** — searchable SQLite database
- **No telemetry** — your API keys stay local (encrypted storage)

## Download

Download the latest release from [Releases](https://github.com/soanseng/voxpen-desktop/releases).

| Platform | File | Notes |
|----------|------|-------|
| **Windows x64** | `.exe` (NSIS installer) | No admin required |
| **Linux x64** | `.AppImage` / `.deb` | AppImage works on all distros |

> **Windows**: Not code-signed. Click "More info" → "Run anyway" if SmartScreen blocks it.

## Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS)
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) (stable)
- Linux only: `libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libasound2-dev libxdo-dev patchelf`

### Steps

```bash
git clone https://github.com/soanseng/voxpen-desktop.git
cd voxpen-desktop
pnpm install
cargo tauri dev          # development
cargo tauri build        # production build
```

## Contributing

Pull requests are welcome!

### macOS Build — Help Wanted

macOS builds are **not currently available** because the CI environment lacks code signing and notarization setup. If you have experience with:

- Apple Developer code signing in GitHub Actions
- Tauri macOS DMG builds and notarization
- Universal binary builds (x86_64 + aarch64)

Please consider contributing a PR to add macOS support to the release workflow. The build matrix entry is already prepared — it just needs signing configuration. See `.github/workflows/release.yml`.

### Development

```bash
# Run tests
cargo test --manifest-path src-tauri/Cargo.toml

# Lint
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# Frontend
pnpm dev
pnpm build
```

## License

MIT
