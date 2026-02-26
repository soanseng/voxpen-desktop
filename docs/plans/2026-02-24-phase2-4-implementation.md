# Phase 2-4 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Implement LLM refinement pipeline, Settings UI, Tray menu expansion (Phase 2), Floating overlay + History (Phase 3), and UI polish + i18n (Phase 4) — all within voxpen-core for business logic + React frontend for UI.

**Architecture:** Business logic lives in `voxpen-core` crate (no Tauri dep). React frontend communicates via Tauri IPC commands. All API calls stay in Rust. Settings stored encrypted. History in SQLite.

**Tech Stack:** Rust (tokio, reqwest, serde, thiserror, mockall, wiremock), React 19, TypeScript, Tailwind CSS, Tauri v2 plugins (store, sql, global-shortcut, autostart).

**Note:** Full Tauri app build requires system libs not available in this environment. All Rust work targets `voxpen-core` (buildable/testable independently). Frontend builds via `pnpm build`. Tauri integration (Phase 1.6-1.7) deferred until system libs available.

---

## Phase 2: LLM Refinement + Settings UI

### Task 2.1: Groq Chat Completion API Client

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/api/groq.rs` (extend existing)
- Test: inline `#[cfg(test)]` module

**What:** Add `chat_completion()` async fn to existing `groq.rs` — sends chat messages to Groq's `/openai/v1/chat/completions` endpoint. Returns refined text.

**Step 1: Write failing tests**

Add 5 tests to `groq.rs`:
- `should_return_refined_text_when_chat_api_responds_successfully`
- `should_return_api_key_error_on_chat_401`
- `should_return_refinement_error_on_chat_500`
- `should_use_correct_model_and_temperature`
- `should_use_default_llm_model`

**Step 2: Run tests — verify they fail**

```bash
cargo test --manifest-path src-tauri/crates/voxpen-core/Cargo.toml -- groq
```

**Step 3: Implement**

```rust
// Constants
pub const DEFAULT_LLM_MODEL: &str = "openai/gpt-oss-120b";
pub const LLM_TEMPERATURE: f32 = 0.3;
pub const LLM_MAX_TOKENS: u32 = 2048;

// Config
pub struct ChatConfig {
    pub api_key: String,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

// Request/Response types
#[derive(Serialize)]
struct ChatRequest { model, messages, temperature, max_tokens }
#[derive(Deserialize)]
struct ChatResponse { choices: Vec<ChatChoice> }
#[derive(Deserialize)]
struct ChatChoice { message: ChatMessage }
#[derive(Serialize, Deserialize)]
struct ChatMessage { role: String, content: String }

// Function
pub async fn chat_completion(config: &ChatConfig, system_prompt: &str, user_text: &str) -> Result<String, AppError>
```

**Step 4: Run tests — verify they pass**

**Step 5: Clippy + commit**

---

### Task 2.2: LLM Refinement Pipeline Orchestration

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/pipeline/refine.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs` (add `pub mod refine;`)

**What:** `refine()` fn composing `prompts::for_language()` + `groq::chat_completion()`. Mirrors Android's `RefineTextUseCase`.

**Step 1: Write failing tests**

- `should_refine_text_using_correct_language_prompt`
- `should_propagate_refinement_errors`
- `should_use_configured_model`

**Step 2: Implement**

```rust
pub async fn refine(text: &str, config: &ChatConfig, language: &Language) -> Result<String, AppError> {
    let system_prompt = prompts::for_language(language);
    groq::chat_completion(config, system_prompt, text).await
}
```

Plus `refine_with_base_url` for testing.

**Step 3: Run tests, clippy, commit**

---

### Task 2.3: LLM Provider Trait + Update Controller

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/controller.rs`

**What:** Add `LlmProvider` trait (mirroring `SttProvider`), update `PipelineController` to support optional refinement after STT. On refinement failure, fall back to raw text (graceful degradation). 5s timeout for LLM.

**Step 1: Write failing tests**

- `should_transition_through_refining_to_refined_when_enabled`
- `should_fallback_to_raw_text_when_refinement_fails`
- `should_skip_refinement_when_disabled`
- `should_timeout_refinement_after_5_seconds`

**Step 2: Implement**

```rust
#[cfg_attr(test, mockall::automock)]
pub trait LlmProvider: Send + Sync {
    fn refine(&self, text: String, language: Language) -> Pin<Box<dyn Future<Output = Result<String, AppError>> + Send>>;
}

// Update PipelineConfig
pub struct PipelineConfig {
    pub groq_api_key: Option<String>,
    pub language: Language,
    pub stt_model: String,
    pub refinement_enabled: bool,  // NEW
    pub llm_api_key: Option<String>,  // NEW (can differ from STT key)
    pub llm_model: String,  // NEW
}

// Update PipelineController<S: SttProvider, L: LlmProvider>
// on_stop_recording: after STT, if refinement_enabled → Refining → refine → Refined
// On failure → fall back to Result with raw text
// 5s timeout via tokio::time::timeout
```

**Step 3: Run tests, clippy, commit**

---

### Task 2.4: Settings UI — React Components

**Files:**
- Create: `src/components/Settings/SettingsWindow.tsx`
- Create: `src/components/Settings/GeneralSection.tsx`
- Create: `src/components/Settings/SttSection.tsx`
- Create: `src/components/Settings/RefinementSection.tsx`
- Create: `src/components/Settings/AppearanceSection.tsx`
- Create: `src/hooks/useSettings.ts`
- Create: `src/lib/tauri.ts`
- Create: `src/types/settings.ts`
- Modify: `src/App.tsx`

**What:** Full settings window with tabbed navigation. Sections: General, STT, Refinement, Appearance. Settings type definitions. Tauri invoke wrapper. Dark mode support.

**Implementation:**

Settings type:
```typescript
interface Settings {
  hotkey: string;
  recordingMode: 'HoldToRecord' | 'Toggle';
  autoPaste: boolean;
  launchAtLogin: boolean;
  sttProvider: 'groq' | 'openai' | 'custom';
  sttLanguage: 'Auto' | 'Chinese' | 'English' | 'Japanese';
  sttModel: string;
  refinementEnabled: boolean;
  refinementProvider: 'groq' | 'openai' | 'anthropic' | 'custom';
  refinementModel: string;
  theme: 'system' | 'light' | 'dark';
  uiLanguage: 'en' | 'zh-TW';
}
```

Tabbed UI with Tailwind styling. Each section as a component. `useSettings` hook manages load/save via Tauri invoke.

**Step: Build frontend**

```bash
cd /home/scipio/projects/voxpen-desktop && pnpm build
```

**Commit after build passes.**

---

### Task 2.5: Tray Menu Expansion (Rust-side types only)

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`

**What:** Define `Settings` struct in voxpen-core (Serialize/Deserialize) for IPC. Define defaults. The actual Tauri tray menu code goes in the app crate (Phase 1.6).

**Commit after tests pass.**

---

## Phase 3: Floating Overlay + History

### Task 3.1: Transcription History Types + SQLite Schema

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/history.rs`
- Modify: `src-tauri/crates/voxpen-core/src/lib.rs`

**What:** Define `TranscriptionEntry` struct, SQL schema constants, and query builder helpers. Actual SQLite access happens in Tauri crate (needs tauri-plugin-sql).

```rust
pub struct TranscriptionEntry {
    pub id: String,
    pub timestamp: i64,
    pub original_text: String,
    pub refined_text: Option<String>,
    pub language: Language,
    pub audio_duration_ms: u64,
    pub provider: String,
}

impl TranscriptionEntry {
    pub fn display_text(&self) -> &str {
        self.refined_text.as_deref().unwrap_or(&self.original_text)
    }
}
```

SQL schema, creation, insert, query, delete, search constants.

**Commit after tests pass.**

---

### Task 3.2: Audio File Chunking

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/audio/chunker.rs`
- Modify: `src-tauri/crates/voxpen-core/src/audio/mod.rs`

**What:** WAV-aware chunking for files > 25MB. Read WAV header, split PCM data into chunks, prepend header to each. Mirrors Android's `AudioChunker`.

**Tests:** chunk splitting, header preservation, boundary conditions, small file passthrough.

**Commit after tests pass.**

---

### Task 3.3: History UI — React Components

**Files:**
- Create: `src/components/History/HistoryWindow.tsx`
- Create: `src/components/History/HistoryList.tsx`
- Create: `src/components/History/HistoryEntry.tsx`

**What:** History list with search, click-to-copy, delete, export. Uses Tauri invoke for data.

**Commit after frontend builds.**

---

### Task 3.4: Overlay Component — React

**Files:**
- Create: `src/components/Overlay.tsx`

**What:** Floating recording indicator component. States: Recording (red pulse), Processing (spinner), Done (green check, auto-hide), Error (red X). Pure UI component driven by pipeline-state events.

**Commit after frontend builds.**

---

## Phase 4: UI Polish + i18n

### Task 4.1: i18n Setup

**Files:**
- Create: `src/lib/i18n.ts`
- Create: `src/locales/en.json`
- Create: `src/locales/zh-TW.json`
- Modify: `src/main.tsx`
- Modify: `package.json` (add `react-i18next`, `i18next`)

**What:** Full i18n with react-i18next. All UI strings externalized. Settings, History, Overlay, tray menu labels.

**Commit after frontend builds.**

---

### Task 4.2: Dark Mode + Theme System

**Files:**
- Create: `src/hooks/useTheme.ts`
- Modify: `src/styles/globals.css`
- Modify: `src/App.tsx`

**What:** Theme system following system preference with manual override. CSS custom properties for consistent theming.

**Commit after frontend builds.**

---

### Task 4.3: UI Polish — Animations + Design

**Files:**
- Modify: `src/styles/globals.css`
- Modify: `src/components/Overlay.tsx`
- Modify: `src/components/Settings/*.tsx`

**What:** Smooth CSS transitions, consistent spacing, polished form controls, loading states. Recording pulse animation, processing spinner, done/error transitions.

**Commit after frontend builds.**

---

## Execution Strategy

- Use subagent-driven development (this session)
- Fresh subagent per task + code review
- voxpen-core tasks: build + test + clippy after each
- Frontend tasks: `pnpm build` after each
- Commit after each task
