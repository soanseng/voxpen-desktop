# Phase 5: Distribution — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Configure cross-platform build pipelines, auto-update infrastructure, icon generation, and installer packaging so VoxPen Desktop can be built and distributed on macOS, Windows, and Linux via GitHub Releases.

**Architecture:** Tauri v2's built-in bundler handles platform-specific installers (DMG, NSIS, AppImage, .deb). The `tauri-plugin-updater` provides delta-aware auto-updates verified by Ed25519 signatures. GitHub Actions with `tauri-apps/tauri-action` orchestrates cross-platform CI/CD, producing signed artifacts and an auto-generated `latest.json` update manifest.

**Tech Stack:** Tauri v2 bundler, tauri-plugin-updater, GitHub Actions, tauri-apps/tauri-action, ImageMagick (icon generation), pnpm

---

## Task 1: Generate Platform Icons (.icns, .ico)

The current icons directory has PNG placeholders but is missing `icon.icns` (macOS) and `icon.ico` (Windows) which are referenced in `tauri.conf.json`.

**Files:**
- Create: `src-tauri/icons/icon.icns`
- Create: `src-tauri/icons/icon.ico`
- Existing: `src-tauri/icons/icon.png`, `src-tauri/icons/32x32.png`, `src-tauri/icons/128x128.png`, `src-tauri/icons/128x128@2x.png`

**Step 1: Generate icon.ico from existing PNGs**

Use ImageMagick to create a multi-resolution .ico file:

```bash
convert src-tauri/icons/32x32.png src-tauri/icons/128x128.png src-tauri/icons/icon.ico
```

If ImageMagick is not available, use `pnpm tauri icon` with the 128x128 source:

```bash
pnpm tauri icon src-tauri/icons/128x128.png
```

This generates all required icon formats in `src-tauri/icons/`.

**Step 2: Verify icons exist**

```bash
ls -la src-tauri/icons/
```

Expected: `icon.icns`, `icon.ico`, `icon.png`, `32x32.png`, `128x128.png`, `128x128@2x.png` all present.

**Step 3: Commit**

```bash
git add src-tauri/icons/
git commit -m "chore: generate platform icons (.icns, .ico) for distribution"
```

---

## Task 2: Add tauri-plugin-updater Dependency

**Files:**
- Modify: `src-tauri/Cargo.toml` (add updater plugin)
- Modify: `package.json` (add JS updater plugin)

**Step 1: Add Rust dependency to Cargo.toml**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
tauri-plugin-updater = "2"
tauri-plugin-process = "2"
```

`tauri-plugin-process` is needed for `relaunch()` after applying an update.

**Step 2: Add JavaScript dependency to package.json**

In `package.json`, add to `dependencies`:

```json
"@tauri-apps/plugin-updater": "^2",
"@tauri-apps/plugin-process": "^2"
```

**Step 3: Install frontend dependencies**

```bash
cd /home/scipio/projects/voxpen-desktop && pnpm install
```

**Step 4: Verify Cargo.toml compiles (core crate only — full Tauri build needs system libs)**

```bash
cargo check -p voxpen-core --manifest-path src-tauri/Cargo.toml
```

Expected: success (core crate unchanged).

**Step 5: Commit**

```bash
git add src-tauri/Cargo.toml package.json pnpm-lock.yaml
git commit -m "feat: add tauri-plugin-updater and tauri-plugin-process dependencies"
```

---

## Task 3: Configure Bundle & Updater in tauri.conf.json

**Files:**
- Modify: `src-tauri/tauri.conf.json`

**Step 1: Update tauri.conf.json with full bundle and updater configuration**

Replace the current `bundle` section and add `plugins.updater`:

```json
{
  "$schema": "https://raw.githubusercontent.com/tauri-apps/tauri/dev/crates/tauri-cli/schema.json",
  "productName": "VoxPen Desktop",
  "version": "0.1.0",
  "identifier": "com.voxpen.desktop",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "pnpm dev",
    "beforeBuildCommand": "pnpm build"
  },
  "app": {
    "windows": [
      {
        "label": "settings",
        "title": "VoxPen Settings",
        "width": 680,
        "height": 520,
        "resizable": true,
        "visible": false,
        "center": true
      }
    ],
    "security": {
      "csp": "default-src 'self'; style-src 'self' 'unsafe-inline'"
    },
    "trayIcon": {
      "iconPath": "icons/icon.png",
      "iconAsTemplate": true,
      "tooltip": "VoxPen"
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ],
    "createUpdaterArtifacts": "v1Compatible",
    "macOS": {
      "minimumSystemVersion": "10.15",
      "hardenedRuntime": true
    },
    "windows": {
      "digestAlgorithm": "sha256",
      "timestampUrl": "http://timestamp.comodoca.com",
      "webviewInstallMode": {
        "type": "downloadBootstrapper",
        "silent": true
      },
      "nsis": {
        "installerMode": "currentUser"
      }
    },
    "linux": {
      "appimage": {
        "bundleMediaFramework": false
      },
      "deb": {
        "desktopFile": null
      }
    }
  },
  "plugins": {
    "updater": {
      "endpoints": [
        "https://github.com/anthropics/voxpen-desktop/releases/latest/download/latest.json"
      ],
      "pubkey": ""
    }
  }
}
```

Notes:
- `createUpdaterArtifacts: "v1Compatible"` generates `.sig` files and `latest.json`.
- `pubkey` is empty placeholder — set during first real release when signing keys are generated.
- `endpoints` URL uses a placeholder GitHub org — update when the real repo is created.
- `hardenedRuntime: true` required for macOS notarization.
- `webviewInstallMode.downloadBootstrapper` keeps Windows installer small.
- `bundleMediaFramework: false` since VoxPen uses `cpal` directly, not GStreamer.

**Step 2: Verify JSON is valid**

```bash
python3 -c "import json; json.load(open('src-tauri/tauri.conf.json'))"
```

Expected: no output (valid JSON).

**Step 3: Commit**

```bash
git add src-tauri/tauri.conf.json
git commit -m "feat: configure bundle targets and updater for cross-platform distribution"
```

---

## Task 4: Register Updater Plugin in Rust Entry Point

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add updater and process plugin registration**

Add the updater and process plugins to the Tauri builder in `lib.rs`:

```rust
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

pub use voxpen_core;

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! VoxPen Desktop is running.", name)
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_sql::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            let settings_item =
                MenuItem::with_id(app, "settings", "Settings...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit VoxPen", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&settings_item, &quit_item])?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .menu_on_left_click(true)
                .tooltip("VoxPen — Ready")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "settings" => {
                        if let Some(window) = app.get_webview_window("settings") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

**Step 2: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: register updater and process plugins in Tauri entry point"
```

---

## Task 5: Add Updater Permissions to Capabilities

**Files:**
- Modify: `src-tauri/capabilities/desktop.json`

**Step 1: Add updater and process permissions**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "desktop-capability",
  "windows": ["settings"],
  "platforms": ["linux", "macOS", "windows"],
  "permissions": [
    "core:default",
    "core:window:default",
    "core:window:allow-show",
    "core:window:allow-hide",
    "core:window:allow-close",
    "core:window:allow-set-focus",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister",
    "store:default",
    "sql:default",
    "autostart:default",
    "updater:default",
    "process:allow-restart"
  ]
}
```

**Step 2: Commit**

```bash
git add src-tauri/capabilities/desktop.json
git commit -m "feat: add updater and process permissions to desktop capabilities"
```

---

## Task 6: Create Update Checker React Component

**Files:**
- Create: `src/components/UpdateChecker.tsx`
- Modify: `src/App.tsx` (mount the component)
- Modify: `src/locales/en.json` (add update strings)
- Modify: `src/locales/zh-TW.json` (add update strings)

**Step 1: Create UpdateChecker component**

```typescript
// src/components/UpdateChecker.tsx
import { useEffect, useState } from "react";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useTranslation } from "react-i18next";

type UpdateStatus = "idle" | "checking" | "available" | "downloading" | "ready" | "error";

export function UpdateChecker() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<UpdateStatus>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState(0);
  const [error, setError] = useState("");

  useEffect(() => {
    checkForUpdates();
  }, []);

  async function checkForUpdates() {
    try {
      setStatus("checking");
      const result = await check();
      if (result) {
        setUpdate(result);
        setStatus("available");
      } else {
        setStatus("idle");
      }
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  async function installUpdate() {
    if (!update) return;
    try {
      setStatus("downloading");
      let totalLength = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started" && event.data.contentLength) {
          totalLength = event.data.contentLength;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (totalLength > 0) {
            setProgress(Math.round((downloaded / totalLength) * 100));
          }
        } else if (event.event === "Finished") {
          setStatus("ready");
        }
      });
      await relaunch();
    } catch (e) {
      setError(String(e));
      setStatus("error");
    }
  }

  if (status === "idle" || status === "checking") return null;

  if (status === "error") {
    return (
      <div className="p-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg text-sm">
        <p className="text-red-700 dark:text-red-300">{t("update.error", { error })}</p>
      </div>
    );
  }

  if (status === "available" && update) {
    return (
      <div className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg text-sm">
        <p className="text-blue-700 dark:text-blue-300 mb-2">
          {t("update.available", { version: update.version })}
        </p>
        <button
          onClick={installUpdate}
          className="px-3 py-1 bg-blue-600 text-white rounded hover:bg-blue-700 text-xs"
        >
          {t("update.install")}
        </button>
      </div>
    );
  }

  if (status === "downloading") {
    return (
      <div className="p-3 bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg text-sm">
        <p className="text-blue-700 dark:text-blue-300 mb-2">{t("update.downloading")}</p>
        <div className="w-full bg-blue-200 dark:bg-blue-800 rounded-full h-2">
          <div
            className="bg-blue-600 h-2 rounded-full transition-all"
            style={{ width: `${progress}%` }}
          />
        </div>
        <p className="text-blue-500 dark:text-blue-400 text-xs mt-1">{progress}%</p>
      </div>
    );
  }

  return null;
}
```

**Step 2: Add i18n strings to en.json**

Add to the existing `en.json`:

```json
"update": {
  "available": "Update v{{version}} available",
  "install": "Install & Restart",
  "downloading": "Downloading update...",
  "error": "Update check failed: {{error}}"
}
```

**Step 3: Add i18n strings to zh-TW.json**

Add to the existing `zh-TW.json`:

```json
"update": {
  "available": "發現新版本 v{{version}}",
  "install": "安裝並重啟",
  "downloading": "正在下載更新...",
  "error": "更新檢查失敗：{{error}}"
}
```

**Step 4: Mount UpdateChecker in App.tsx**

Import and render `<UpdateChecker />` inside the settings window layout (e.g., at the bottom of the sidebar or as a banner).

**Step 5: Verify frontend builds**

```bash
pnpm build
```

Expected: success.

**Step 6: Commit**

```bash
git add src/components/UpdateChecker.tsx src/locales/en.json src/locales/zh-TW.json src/App.tsx
git commit -m "feat: add auto-update checker UI with download progress and i18n"
```

---

## Task 7: Create GitHub Actions CI Workflow

**Files:**
- Create: `.github/workflows/ci.yml`

**Step 1: Create CI workflow for pull requests and pushes to main**

```yaml
name: CI

on:
  push:
    branches: [main, dev]
  pull_request:
    branches: [main]

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  test-core:
    name: Test voxpen-core
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: dtolnay/rust-toolchain@stable

      - uses: swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target

      - name: Run tests
        run: cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml

      - name: Run clippy
        run: cargo clippy -p voxpen-core --manifest-path src-tauri/Cargo.toml -- -D warnings

  test-frontend:
    name: Test frontend
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: pnpm/action-setup@v4
        with:
          version: latest

      - uses: actions/setup-node@v4
        with:
          node-version: lts/*
          cache: pnpm

      - run: pnpm install --frozen-lockfile
      - run: pnpm build
```

**Step 2: Verify YAML is valid**

```bash
python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml'))"
```

**Step 3: Commit**

```bash
git add .github/workflows/ci.yml
git commit -m "ci: add CI workflow for core tests and frontend build"
```

---

## Task 8: Create GitHub Actions Release Workflow

**Files:**
- Create: `.github/workflows/release.yml`

**Step 1: Create release workflow triggered by version tags**

```yaml
name: Release

on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:

concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: true

jobs:
  test-core:
    name: Test voxpen-core
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target
      - run: cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml
      - run: cargo clippy -p voxpen-core --manifest-path src-tauri/Cargo.toml -- -D warnings

  build-release:
    name: Build (${{ matrix.label }})
    needs: test-core
    permissions:
      contents: write
    strategy:
      fail-fast: false
      matrix:
        include:
          - platform: macos-latest
            args: '--target aarch64-apple-darwin'
            rust_target: aarch64-apple-darwin
            label: macOS-arm64
          - platform: macos-latest
            args: '--target x86_64-apple-darwin'
            rust_target: x86_64-apple-darwin
            label: macOS-x64
          - platform: ubuntu-22.04
            args: ''
            rust_target: ''
            label: Linux-x64
          - platform: windows-latest
            args: '--target x86_64-pc-windows-msvc'
            rust_target: x86_64-pc-windows-msvc
            label: Windows-x64

    runs-on: ${{ matrix.platform }}

    steps:
      - name: Checkout
        uses: actions/checkout@v4

      - name: Install Linux dependencies
        if: matrix.platform == 'ubuntu-22.04'
        run: |
          sudo apt-get update
          sudo apt-get install -y \
            libwebkit2gtk-4.1-dev \
            libappindicator3-dev \
            librsvg2-dev \
            patchelf \
            libasound2-dev

      - name: Setup pnpm
        uses: pnpm/action-setup@v4
        with:
          version: latest

      - name: Setup Node.js
        uses: actions/setup-node@v4
        with:
          node-version: lts/*
          cache: pnpm

      - name: Install Rust stable
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.rust_target }}

      - name: Rust cache
        uses: swatinem/rust-cache@v2
        with:
          workspaces: src-tauri -> target

      - name: Install frontend dependencies
        run: pnpm install --frozen-lockfile

      - name: Build and release
        uses: tauri-apps/tauri-action@v0
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          # macOS code signing (set these secrets when ready)
          APPLE_CERTIFICATE: ${{ secrets.APPLE_CERTIFICATE }}
          APPLE_CERTIFICATE_PASSWORD: ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}
          APPLE_SIGNING_IDENTITY: ${{ secrets.APPLE_SIGNING_IDENTITY }}
          APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
          APPLE_ID: ${{ secrets.APPLE_ID }}
          APPLE_PASSWORD: ${{ secrets.APPLE_PASSWORD }}
          # Updater signing
          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
          TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}
        with:
          tagName: v__VERSION__
          releaseName: 'VoxPen Desktop v__VERSION__'
          releaseBody: |
            ## VoxPen Desktop v__VERSION__

            See the assets below to download for your platform:
            - **macOS**: `.dmg` (Apple Silicon or Intel)
            - **Windows**: `.exe` (NSIS installer)
            - **Linux**: `.AppImage` or `.deb`
          releaseDraft: true
          prerelease: false
          args: ${{ matrix.args }}
```

**Step 2: Commit**

```bash
git add .github/workflows/release.yml
git commit -m "ci: add cross-platform release workflow with tauri-action"
```

---

## Task 9: Add Build Scripts for Local Development

**Files:**
- Modify: `package.json` (add distribution scripts)

**Step 1: Add scripts to package.json**

Add these scripts:

```json
"scripts": {
  "dev": "vite",
  "build": "tsc && vite build",
  "preview": "vite preview",
  "tauri": "tauri",
  "tauri:dev": "tauri dev",
  "tauri:build": "tauri build",
  "tauri:build:debug": "tauri build --debug",
  "test:core": "cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml",
  "lint:core": "cargo clippy -p voxpen-core --manifest-path src-tauri/Cargo.toml -- -D warnings"
}
```

**Step 2: Verify scripts are reachable**

```bash
pnpm run test:core
```

Expected: all tests pass.

**Step 3: Commit**

```bash
git add package.json
git commit -m "chore: add convenience scripts for core testing and tauri builds"
```

---

## Task 10: Update plan.md — Mark Phase 5 Items

**Files:**
- Modify: `plan.md`

**Step 1: Update Phase 5 section with completed items**

Mark completed distribution infrastructure items:
- [x] Tauri updater plugin configured
- [x] Bundle configuration for DMG, NSIS, AppImage, .deb
- [x] GitHub Actions CI workflow
- [x] GitHub Actions release workflow (cross-platform)
- [x] Platform icons generated (.icns, .ico)
- [x] Auto-update checker UI with i18n
- [x] Convenience build scripts

Mark items that require external accounts/credentials (deferred):
- [ ] Apple Developer account + code signing certificate
- [ ] Apple notarization credentials
- [ ] Windows code signing certificate (optional)
- [ ] Updater signing key generation (first release)
- [ ] GitHub repository URL in updater endpoint
- [ ] Website / landing page (separate project)
- [ ] Homebrew cask formula

**Step 2: Commit**

```bash
git add plan.md
git commit -m "docs: mark Phase 5 distribution infrastructure complete in plan.md"
```

---

## Execution Summary

| Task | Description | Depends On |
|------|-------------|------------|
| 1 | Generate platform icons | — |
| 2 | Add updater dependencies | — |
| 3 | Configure bundle & updater in tauri.conf.json | — |
| 4 | Register updater plugin in lib.rs | 2 |
| 5 | Add updater permissions to capabilities | — |
| 6 | Create UpdateChecker React component | 2 |
| 7 | Create CI workflow | — |
| 8 | Create release workflow | — |
| 9 | Add build scripts | — |
| 10 | Update plan.md | 1-9 |

**Parallelization:** Tasks 1, 2, 3, 5, 7, 8, 9 are independent and can run in parallel. Tasks 4 and 6 depend on Task 2 (updater dep). Task 10 runs last.

**Note on Tauri build:** The full `cargo build` for the Tauri app crate requires system libraries (`libgtk-3-dev`, `libwebkit2gtk-4.1-dev`, `libasound2-dev`). The voxpen-core crate can be tested independently. CI workflow handles system deps via `apt-get install`.
