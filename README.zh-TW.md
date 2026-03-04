# VoxPen Desktop (語墨桌面版)

[English](./README.md)

系統列語音轉文字應用程式，支援 Windows 和 Linux。按下全域快捷鍵即可口述，轉錄並潤飾後的文字會自動貼上到游標位置。

使用 **Tauri v2**（Rust 後端 + React 前端）開發，BYOK（自備 API Key）。

## 功能特色

- **全域快捷鍵** — 在任何應用程式中皆可使用，無需切換輸入法
- **按住口述**或**切換**錄音模式
- **語音辨識服務** — Groq Whisper、OpenAI Whisper 或自訂端點
- **LLM 文字潤飾** — 自動移除贅字、修正語法、加入標點符號
- **自動貼上** — 轉錄結果直接貼到游標位置
- **翻譯模式** — 將語音翻譯為目標語言
- **多語言支援** — 自動偵測、中文、English、日本語
- **浮動指示器** — 錄音／處理狀態顯示
- **轉錄紀錄** — 可搜尋的 SQLite 資料庫
- **無遙測** — API Key 僅存於本機（加密儲存）

## 下載

從 [Releases](https://github.com/soanseng/voxpen-desktop/releases) 下載最新版本。

| 平台 | 檔案 | 說明 |
|------|------|------|
| **Windows x64** | `.exe`（NSIS 安裝程式） | 不需管理員權限 |
| **Linux x64** | `.AppImage` / `.deb` | AppImage 適用所有發行版 |

> **Windows**：未經程式碼簽署。若 SmartScreen 攔截，請點「其他資訊」→「仍要執行」。

## 授權方案

VoxPen Desktop 採用免費增值模式，透過 [LemonSqueezy](https://www.lemonsqueezy.com/) 授權金鑰管理。

- **免費方案** — 每日有限使用次數
- **Pro 方案** — 無限使用次數，需授權金鑰

可從應用程式的設定頁面購買授權金鑰，在**設定 → 授權**中輸入金鑰即可啟用 Pro 功能。原始碼完全開源，你可以自行建置與修改。

## 從原始碼建置

### 前置需求

- [Node.js](https://nodejs.org/)（LTS）
- [pnpm](https://pnpm.io/)
- [Rust](https://rustup.rs/)（stable）
- 僅 Linux：`libwebkit2gtk-4.1-dev libgtk-3-dev libappindicator3-dev librsvg2-dev libasound2-dev libxdo-dev patchelf`

### 步驟

```bash
git clone https://github.com/soanseng/voxpen-desktop.git
cd voxpen-desktop
pnpm install
cargo tauri dev          # 開發模式
cargo tauri build        # 正式建置
```

## 貢獻

歡迎提交 Pull Request！

### macOS 建置 — 徵求協助

目前**尚未提供** macOS 版本，因為 CI 環境缺少程式碼簽署與公證設定。如果你有以下經驗：

- 在 GitHub Actions 中設定 Apple Developer 程式碼簽署
- Tauri macOS DMG 建置與公證
- Universal binary 建置（x86_64 + aarch64）

歡迎提交 PR 為 release workflow 加入 macOS 支援。建置矩陣已預先準備好，只需簽署設定即可。請參閱 `.github/workflows/release.yml`。

### 開發

```bash
# 執行測試
cargo test --manifest-path src-tauri/Cargo.toml

# 程式碼檢查
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings

# 前端
pnpm dev
pnpm build
```

## 授權

MIT
