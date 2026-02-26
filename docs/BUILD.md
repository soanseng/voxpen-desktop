# VoxPen Desktop — Build Guide

Step-by-step instructions for building VoxPen Desktop on each platform.

## Prerequisites (all platforms)

| Tool | Version | Install |
|------|---------|---------|
| Rust | stable (2021 edition) | [rustup.rs](https://rustup.rs/) |
| Node.js | 18+ LTS | [nodejs.org](https://nodejs.org/) |
| pnpm | 9+ | `npm install -g pnpm` |
| Tauri CLI | 2.x | `cargo install tauri-cli` |

Verify prerequisites:

```bash
rustc --version        # rustc 1.8x.x
node --version         # v18.x+ or v20.x+
pnpm --version         # 9.x+
cargo tauri --version  # tauri-cli 2.x
```

## Clone and install frontend dependencies

```bash
git clone https://github.com/anthropics/voxpen-desktop.git
cd voxpen-desktop
pnpm install
```

## Test core library (no system deps needed)

This works on any platform — voxpen-core has no OS-specific dependencies:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml
cargo clippy -p voxpen-core --manifest-path src-tauri/Cargo.toml -- -D warnings
```

---

## Linux (Ubuntu / Debian)

### 1. Install system dependencies

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential \
  pkg-config \
  libglib2.0-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libappindicator3-dev \
  librsvg2-dev \
  libasound2-dev \
  patchelf
```

What each package provides:

| Package | Purpose |
|---------|---------|
| `libgtk-3-dev` | Tauri window toolkit |
| `libwebkit2gtk-4.1-dev` | WebView for React frontend |
| `libappindicator3-dev` | System tray icon |
| `librsvg2-dev` | SVG icon rendering |
| `libasound2-dev` | ALSA audio (cpal crate) |
| `patchelf` | Fix RPATH in AppImage bundles |

### 2. Build

Development (with hot-reload):

```bash
pnpm tauri:dev
```

Release (optimized binary + installers):

```bash
pnpm tauri:build
```

### 3. Output artifacts

```
src-tauri/target/release/bundle/
  deb/voxpen-desktop_0.1.0_amd64.deb    # Debian/Ubuntu installer
  appimage/voxpen-desktop_0.1.0_amd64.AppImage  # Portable
```

Install the `.deb`:

```bash
sudo dpkg -i src-tauri/target/release/bundle/deb/voxpen-desktop_0.1.0_amd64.deb
```

Or run the AppImage directly:

```bash
chmod +x src-tauri/target/release/bundle/appimage/voxpen-desktop_0.1.0_amd64.AppImage
./src-tauri/target/release/bundle/appimage/voxpen-desktop_0.1.0_amd64.AppImage
```

---

## Linux (Arch / Manjaro / EndeavourOS)

### 1. Install system dependencies

```bash
sudo pacman -Syu --needed \
  base-devel \
  webkit2gtk-4.1 \
  gtk3 \
  libappindicator-gtk3 \
  librsvg \
  alsa-lib \
  patchelf \
  pkgconf
```

> **Note**: Arch uses `webkit2gtk-4.1` (not `libwebkit2gtk-4.1-dev`). The Tauri v2 requirement is specifically WebKitGTK 4.1, not the older 4.0.

If `webkit2gtk-4.1` is not found in official repos, install from AUR:

```bash
# Using yay
yay -S webkit2gtk-4.1

# Or using paru
paru -S webkit2gtk-4.1
```

Arch equivalents of Debian packages:

| Debian | Arch |
|--------|------|
| `libgtk-3-dev` | `gtk3` |
| `libwebkit2gtk-4.1-dev` | `webkit2gtk-4.1` |
| `libappindicator3-dev` | `libappindicator-gtk3` |
| `librsvg2-dev` | `librsvg` |
| `libasound2-dev` | `alsa-lib` |
| `patchelf` | `patchelf` |
| `pkg-config` | `pkgconf` |

### 2. Build

Development:

```bash
pnpm tauri:dev
```

Release:

```bash
pnpm tauri:build
```

### 3. Output artifacts

Same as Debian — `.AppImage` and `.deb` are generated. For Arch-native packaging, you can create a PKGBUILD or use the AppImage.

### Optional: Create Arch PKGBUILD

For AUR distribution, create a `PKGBUILD`:

```bash
# pkgname=voxpen-desktop
# pkgver=0.1.0
# Simplified — the actual binary is in target/release/voxpen-desktop
install -Dm755 "src-tauri/target/release/voxpen-desktop" "$pkgdir/usr/bin/voxpen-desktop"
install -Dm644 "src-tauri/icons/128x128.png" "$pkgdir/usr/share/pixmaps/voxpen-desktop.png"
```

---

## Windows

### 1. Install system dependencies

**Option A: Visual Studio Build Tools (recommended)**

Download and install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/). Select:
- "Desktop development with C++" workload
- Windows 10/11 SDK

**Option B: Full Visual Studio**

Any edition (Community is free) with the C++ workload.

**WebView2**: Ships with Windows 10 (1803+) and Windows 11. The NSIS installer includes a bootstrapper that auto-downloads WebView2 if missing.

### 2. Install Rust (Windows)

```powershell
# Download and run rustup-init.exe from https://rustup.rs/
# Select default installation (MSVC toolchain)
rustup default stable-x86_64-pc-windows-msvc
```

### 3. Install Node.js and pnpm

```powershell
# Install Node.js LTS from https://nodejs.org/
npm install -g pnpm
```

### 4. Build

Open PowerShell or CMD in the project directory:

```powershell
pnpm install
pnpm tauri:dev     # Development
pnpm tauri:build   # Release
```

### 5. Output artifacts

```
src-tauri\target\release\bundle\
  nsis\VoxPen Desktop_0.1.0_x64-setup.exe    # NSIS installer
```

The installer:
- Installs per-user (no admin needed) — `currentUser` mode
- Bootstraps WebView2 if not present
- Creates Start Menu shortcut and uninstaller

### Windows code signing (optional)

For production releases without SmartScreen warnings:

```powershell
# Set environment variables before build:
$env:TAURI_SIGNING_PRIVATE_KEY = "your-updater-key"
# For authenticode signing:
# $env:TAURI_SIGNING_IDENTITY = "your-cert-thumbprint"
```

---

## macOS — Without a Mac

Apple does not officially support building macOS apps on non-Mac hardware. However, there are practical approaches:

### Approach 1: GitHub Actions (recommended)

The project's CI already handles this. Push a tag to trigger cross-platform builds:

```bash
git tag v0.1.0
git push origin v0.1.0
```

This triggers `.github/workflows/release.yml` which builds:
- `macOS-arm64` (Apple Silicon) on `macos-latest`
- `macOS-x64` (Intel) on `macos-latest`
- Linux and Windows builds in parallel

**Output**: `.dmg` files in a GitHub draft release.

**Requirements** (set as GitHub repository secrets):

| Secret | Purpose | How to get |
|--------|---------|------------|
| `APPLE_CERTIFICATE` | Base64 Developer ID certificate | Export from Keychain Access on any Mac |
| `APPLE_CERTIFICATE_PASSWORD` | Certificate export password | Set during export |
| `APPLE_SIGNING_IDENTITY` | e.g. `Developer ID Application: Your Name (TEAMID)` | Apple Developer account |
| `APPLE_TEAM_ID` | 10-character team ID | Apple Developer portal |
| `APPLE_ID` | Apple ID email | Your Apple account |
| `APPLE_PASSWORD` | App-specific password | appleid.apple.com > Sign-In & Security |
| `TAURI_SIGNING_PRIVATE_KEY` | Updater signature key | `cargo tauri signer generate` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Key password | Set during generation |

### Approach 2: macOS VM (for local testing)

> **Legal note**: macOS licensing permits running macOS VMs only on Apple hardware. This is for users who have a Mac but want to build in a clean VM.

Using UTM, Parallels, or VMware on a Mac host:

1. Install macOS in a VM
2. Install Xcode Command Line Tools: `xcode-select --install`
3. Install Rust, Node.js, pnpm as normal
4. Clone and build:

```bash
pnpm install
pnpm tauri:build
# Outputs: src-tauri/target/release/bundle/dmg/VoxPen Desktop_0.1.0_aarch64.dmg
```

### Approach 3: Rent a Mac in the Cloud

For one-off builds without owning a Mac:

| Service | Type | Starting Price |
|---------|------|----------------|
| [MacStadium](https://www.macstadium.com/) | Dedicated Mac | ~$60/mo |
| [AWS EC2 Mac](https://aws.amazon.com/ec2/instance-types/mac/) | EC2 Mac instances | ~$1.08/hr (24hr min) |
| [Hetzner](https://www.hetzner.com/cloud/) | Mac Mini M1 | ~$50/mo |
| [GitHub Actions](https://github.com/features/actions) | CI runner | Free for public repos |

For GitHub Actions (free for public repos, 3000 min/mo for private), the release workflow is already configured.

### Approach 4: Cross-compilation from Linux (experimental)

Cross-compiling to macOS from Linux is possible but fragile. Not recommended for production.

```bash
# Install osxcross (macOS cross-compilation toolchain)
# Requires extracting Xcode SDK from a macOS install
# https://github.com/tpoechtrager/osxcross

# Add target
rustup target add x86_64-apple-darwin aarch64-apple-darwin

# Build (requires osxcross in PATH)
CC=x86_64-apple-darwin-clang \
CXX=x86_64-apple-darwin-clang++ \
cargo tauri build --target x86_64-apple-darwin
```

**Limitations**:
- Cannot code sign or notarize without Apple Developer tools
- Cannot produce `.dmg` (needs `hdiutil` which is macOS-only)
- WebView framework linking may fail
- Not recommended — use GitHub Actions instead

---

## macOS — Code Signing and Notarization

If you have Apple Developer access ($99/year):

### 1. Generate signing certificate

1. Go to [Apple Developer Portal](https://developer.apple.com/account/resources/certificates/list)
2. Create a "Developer ID Application" certificate
3. Download and install in Keychain Access
4. Export as `.p12` file

### 2. Generate app-specific password

1. Go to [appleid.apple.com](https://appleid.apple.com)
2. Sign-In & Security > App-Specific Passwords
3. Generate a password for "VoxPen Notarization"

### 3. Set environment variables

For local builds:

```bash
export APPLE_CERTIFICATE="$(base64 -i certificate.p12)"
export APPLE_CERTIFICATE_PASSWORD="your-p12-password"
export APPLE_SIGNING_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export APPLE_TEAM_ID="ABCDE12345"
export APPLE_ID="you@example.com"
export APPLE_PASSWORD="xxxx-xxxx-xxxx-xxxx"
```

For CI, set these as GitHub repository secrets (already configured in release workflow).

### 4. Build signed + notarized

```bash
pnpm tauri:build
# Tauri automatically signs and submits for notarization when env vars are set
```

---

## Updater Signing Key

For the auto-update system (all platforms):

```bash
cargo tauri signer generate -w ~/.tauri/voxpen.key
```

This outputs a public key. Put it in `tauri.conf.json`:

```json
"plugins": {
  "updater": {
    "pubkey": "dW50cnVzdGVkIGNvbW1lbnQ6...<your-public-key>"
  }
}
```

Set the private key as a GitHub secret (`TAURI_SIGNING_PRIVATE_KEY`) for CI releases.

---

## Build Verification Checklist

After building on any platform:

```bash
# 1. Core tests pass
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml

# 2. Clippy clean
cargo clippy -p voxpen-core --manifest-path src-tauri/Cargo.toml -- -D warnings

# 3. Frontend builds
pnpm build

# 4. Full Tauri build succeeds
pnpm tauri:build

# 5. App starts and shows tray icon
# 6. Settings window opens from tray
# 7. Hotkey (Ctrl+Shift+V) triggers recording indicator
# 8. API key can be saved and tested
```

---

## Troubleshooting

### Linux: `webkit2gtk-4.1` not found

Tauri v2 requires WebKitGTK 4.1 (not 4.0). On older Ubuntu (<22.04):

```bash
# Check installed version
pkg-config --modversion webkit2gtk-4.1

# Ubuntu 22.04+ has it. On older versions, upgrade or use a PPA.
```

### Linux: ALSA errors at runtime

If audio recording fails, check ALSA or PulseAudio:

```bash
# List audio devices
arecord -l

# Test recording
arecord -d 3 -f S16_LE -r 16000 -c 1 test.wav
```

### Windows: MSVC linker not found

Install Visual Studio Build Tools with the C++ workload, or:

```powershell
rustup toolchain install stable-x86_64-pc-windows-msvc
```

### Arch: `webkit2gtk-4.1` symbol errors

Ensure you have the correct version. Rebuild from AUR if needed:

```bash
yay -S webkit2gtk-4.1 --rebuild
```

### macOS: "not notarized" warning

The app must be code-signed and notarized for Gatekeeper. Without signing:

```bash
# Users can bypass by right-clicking > Open, or:
xattr -cr /Applications/VoxPen\ Desktop.app
```

### Build fails with `cc` linker error

Ensure C compiler is installed:

```bash
# Linux
sudo apt install build-essential  # Debian
sudo pacman -S base-devel          # Arch

# macOS
xcode-select --install
```
