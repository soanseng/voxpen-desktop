# VoxPen Desktop (語墨桌面版)

[繁體中文](./README.zh-TW.md) | [Website](https://voxpen.app/)

System-tray voice-to-text app for Windows and Linux. Press a global hotkey to dictate, and the transcribed + refined text is automatically pasted at the cursor position in any app.

Built with **Tauri v2** (Rust backend + React frontend), BYOK (Bring Your Own Key).

## Screenshots

| General Settings | Speech Settings |
|:---:|:---:|
| ![General](docs/screenshots/settings-general-en.png) | ![Speech](docs/screenshots/settings-speech-en.png) |

| Refinement Settings | License & Usage |
|:---:|:---:|
| ![Refinement](docs/screenshots/settings-refinement-en.png) | ![License](docs/screenshots/settings-license-en.png) |

<p align="center">
  <img src="docs/screenshots/recording-overlay.png" alt="Recording Overlay" width="200">
  <br><em>Floating recording indicator</em>
</p>

## Features

- **Global hotkey** — works system-wide in any app, no input method switching
- **Hold-to-dictate** or **toggle** recording mode
- **STT providers** — Groq Whisper, OpenAI transcription models, or custom endpoint
- **Reliable failures** — long live recordings are chunked; provider errors stay visible and can be retried from history
- **LLM refinement** — auto-remove filler words, fix grammar, add punctuation
- **Auto-paste** — transcription goes straight to cursor position
- **Translation mode** — translate speech to a target language
- **Multi-language** — Auto-detect, 中文, English, 日本語
- **Audio ducking** — automatically lower other apps' volume while recording
- **Floating overlay** — recording/processing status indicator with readable errors
- **Transcription history** — searchable SQLite database with failed-recording retry
- **No telemetry** — your API keys stay local (encrypted storage)

## Download

Download the latest release from [Releases](https://github.com/soanseng/voxpen-desktop/releases).

| Platform | File | Notes |
|----------|------|-------|
| **Windows x64** | `.exe` (NSIS installer) | No admin required |
| **Linux x64** | `.AppImage` / `.deb` | AppImage works on most distros |
| **Linux x64 (Arch)** | `voxpen-desktop` (native binary) | For Arch Linux / rolling-release distros |

> **Windows**: Not code-signed. Click "More info" → "Run anyway" if SmartScreen blocks it.

### Wayland Auto-Paste

On Wayland sessions, VoxPen uses [`ydotool`](https://github.com/ReimuNotMoe/ydotool) (kernel-level `/dev/uinput`) to simulate Ctrl+V for auto-paste. This works on **all** Wayland compositors (KDE Plasma, GNOME, sway, Hyprland, etc.). Falls back to [`wtype`](https://github.com/atx/wtype) on wlroots-based compositors if `ydotool` is unavailable.

| Distro | Install | Setup |
|--------|---------|-------|
| **Arch / Manjaro** | `sudo pacman -S ydotool` | `systemctl --user enable --now ydotool` + `sudo usermod -aG input $USER` (re-login) |
| **Debian / Ubuntu 24.04+** | `sudo apt install ydotool` | `systemctl --user enable --now ydotool` + `sudo usermod -aG input $USER` (re-login) |
| **Fedora** | `sudo dnf install ydotool` | `systemctl --user enable --now ydotool` + `sudo usermod -aG input $USER` (re-login) |

> On X11, neither `ydotool` nor `wtype` is needed — VoxPen uses `enigo` (libxdo) directly.

### Arch Linux / Rolling-Release Distros

The AppImage bundles Ubuntu's WebKit libraries which may be ABI-incompatible with newer system libraries (e.g. on Arch, Fedora Rawhide). Use the native binary instead:

1. Install system dependencies:
   ```bash
   sudo pacman -S webkit2gtk-4.1 libayatana-appindicator ydotool wl-clipboard xdotool
   ```
2. Download `voxpen-desktop` from the [latest release](https://github.com/soanseng/voxpen-desktop/releases)
3. Make it executable and place it in your PATH:
   ```bash
   chmod +x voxpen-desktop
   cp voxpen-desktop ~/.local/bin/voxpen
   ```

> **Note**: The manual copy above installs only the binary. For Wayland auto-paste support, use the install script below, which also installs the required helper script and desktop entry.

#### Install Script (Build from Source)

An install script is provided for Linux users who build from source. It installs the binary, resources (auto-paste helper script), desktop entry, and icon:

```bash
git clone https://github.com/soanseng/voxpen-desktop.git
cd voxpen-desktop
pnpm install
npx tauri build
./scripts/install-linux.sh            # install
./scripts/install-linux.sh uninstall  # remove
```

The script installs to `~/.local/bin/` and checks for required Wayland dependencies.

### Debian / Ubuntu

1. Download the `.deb` or `.AppImage` from the [latest release](https://github.com/soanseng/voxpen-desktop/releases)
2. For Wayland auto-paste support (Ubuntu 24.04+):
   ```bash
   sudo apt install ydotool
   systemctl --user enable --now ydotool
   sudo usermod -aG input $USER   # re-login required
   ```
   > On older Debian/Ubuntu versions, `ydotool` may not be in the official repos. Build from [source](https://github.com/ReimuNotMoe/ydotool) or use X11 where `enigo` works natively.

## Licensing

VoxPen Desktop uses a freemium model with [LemonSqueezy](https://www.lemonsqueezy.com/) license keys.

- **Free tier** — limited daily usage
- **Pro tier** — unlimited usage with a license key

You can purchase a license key from the app's settings page. Enter the key in **Settings → License** to activate Pro features. The source code is fully open — you are free to build and modify the app yourself.

## Build from Source

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS)
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/) (stable)
- Linux (Debian/Ubuntu): `libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libasound2-dev libxdo-dev patchelf`
- Linux (Arch): `webkit2gtk-4.1 libayatana-appindicator`

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
