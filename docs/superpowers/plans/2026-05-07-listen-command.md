# Listen to My Command Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Listen to My Command" mode that records a spoken instruction, transcribes it, asks the configured LLM to produce a useful deliverable such as code, a spec, an email, a task list, or a meeting summary, then pastes that result into the user's current app.

**Architecture:** Keep the whole hotkey -> record -> STT -> command LLM -> history -> paste pipeline in Rust, matching VoxPen's existing dictation and Voice Edit architecture. Add a new command-mode hotkey path and a purpose-built `task_command` LLM module instead of reusing `refine::refine()`, because command mode must follow the user's spoken task while still refusing to claim it executed OS actions. Command mode has its own provider/model settings, independent from STT and refinement settings. React remains a settings-only UI for enabling the mode, configuring the hotkey, and selecting an OpenAI-compatible command provider/model.

**Tech Stack:** Tauri v2, Rust 2021, `tokio`, `reqwest`, `thiserror`, `rusqlite`, `wiremock`, React 19, TypeScript, Tailwind, i18next.

---

## Planning Status

This file is the plan only. Implementation should happen in a dedicated worktree:

```bash
git worktree add ../voxpen-desktop-listen-command -b feat/listen-command main
cd ../voxpen-desktop-listen-command
```

Do not start implementation until this plan is accepted.

## Product Definition

### User-Facing Name

- English: `Listen to My Command`
- zh-TW: `聽我指令`

### What It Does

The user presses a separate hotkey, speaks an instruction, releases the hotkey, and VoxPen pastes a generated deliverable into the current app.

Examples:

- "Write a TypeScript function that debounces search input."
- "Turn these requirements into a concise implementation checklist."
- "Write a polite customer email saying the bug is fixed."
- "Draft a PR description for a retryable transcription failure fix."

### What It Does Not Do in v1

- It does not run shell commands.
- It does not edit files directly.
- It does not click UI, browse websites, or automate other apps.
- It does not read IDE buffers or terminal output as context.
- It does not promise that generated code compiles.

This is intentional. OpenAI's current desktop Work with Apps flow can read/edit supported apps and generate diffs, but its own help docs state voice mode does not yet support code edits directly. VoxPen should start with the safer universal desktop surface: pasteable output in any app. Source checked 2026-05-07: https://help.openai.com/en/articles/10119604

## Landscape Check

- **Layer 1: Tried and true.** Existing AI dictation apps win by turning speech into pasteable text and letting users stay in their current app. VoxPen already has this foundation.
- **Layer 2: Current market.** 2026 dictation tools are converging on context-aware modes, prompt presets, custom vocabulary, and ramble-to-polished output. Product Hunt's 2026 alternatives list calls out modes/prompts and context-aware dictation as differentiators. Source checked 2026-05-07: https://www.producthunt.com/products/wisprflow/alternatives
- **Layer 3: First principles.** "Execute a task" is too broad for a global hotkey app. The first safe and useful product is "turn spoken intent into a pasteable artifact." Direct OS execution should be a later, explicit, confirm-before-action feature.

## CEO Review

### Premise Challenge

The right problem is not "add another text cleanup prompt." The real user outcome is reducing the distance from idea -> useful artifact. VoxPen already removes typing friction; this feature should remove the "now I need to open ChatGPT and craft a prompt" step.

### Implementation Alternatives

| Approach | Summary | Effort | Risk | Completeness | Reuses |
|---|---|---:|---:|---:|---|
| A. Prompt preset inside existing refinement | Add another tone/custom prompt and make users toggle it manually. | S | Medium | 4/10 | `refine::refine`, existing settings |
| B. Dedicated command hotkey and command LLM module | Add `hotkey_listen_command`, command-mode settings, a new hotkey handler, and a purpose-built prompt. | M | Medium | 9/10 | hotkey, recorder, STT-only, LLM chat, history, paste |
| C. Full agent mode | Let spoken commands edit files, run shell, or control apps. | XL | High | 10/10 for ambition, 3/10 for safe v1 | none safely |

**Recommendation:** Approach B. It is the right first version: meaningful product leap, bounded blast radius, and still uses the architecture VoxPen already trusts.

### 12-Month Dream State

```text
CURRENT STATE                 THIS PLAN                         12-MONTH IDEAL
Dictation + cleanup    ->     Spoken task -> artifact     ->    Voice-driven AI workbench
Voice Edit selected text      pasted into current app           with explicit safe actions,
                              with history and retry            app context, and approvals
```

### Scope Decisions

Accepted:

- New "Listen to My Command" hotkey and settings.
- Spoken instruction -> STT -> LLM-generated pasteable output.
- Purpose-built command prompt, not generic refinement.
- Separate command LLM provider/model settings. Transcription may use Groq while command generation uses OpenAI, OpenRouter, or any OpenAI-compatible custom provider.
- Built-in command model list plus a custom model ID field so users can enter newly released or provider-specific model names without an app update.
- History entries tagged with a new kind so UI can show "Instruction" and "Result."
- Command history entries record command LLM provider/model separately from STT provider.
- Full Rust tests for prompt, settings migration, hotkey registration, history migration, and LLM failures.

Deferred:

- Direct file edits and shell execution.
- Reading active app content beyond active app name.
- Preset library for meeting/tasks/spec/email/code modes.
- Streaming LLM output in the overlay.

## Design Review

### Initial Design Score

Design completeness: 8/10.

What makes it a 10: the settings UI must make the safety boundary obvious, the history UI must label command entries correctly, and empty/error states must avoid making users think VoxPen executed anything outside paste.

### Information Architecture

Place the enable/hotkey controls in `GeneralSection.tsx` near the existing hotkeys, and place command LLM configuration in a dedicated command settings section. It is a system behavior and an AI action, not a refinement tone.

```text
Settings
├── General
    ├── Push-to-Talk Hotkey
    ├── Hands-free Hotkey
    ├── Auto-paste
    ├── Voice Commands
    ├── Voice Edit Hotkey
    ├── Listen to My Command
    │   ├── Toggle
    │   ├── Hotkey picker
    │   └── Hint: "Generates pasteable text. It does not run commands or edit files."
    └── Audio Ducking
└── Listen Command
    ├── Command provider
    │   ├── OpenAI
    │   ├── Groq
    │   ├── OpenRouter
    │   └── Custom OpenAI-compatible
    ├── Command model
    │   ├── Built-in model list for the selected provider
    │   └── Custom model ID input
    └── Custom base URL, shown only for Custom provider
```

### Interaction States

| Feature | Loading | Empty | Error | Success | Partial |
|---|---|---|---|---|---|
| Settings toggle | Existing settings loading spinner | Hotkey field can be empty only if toggle off | Hotkey registration error text | Toggle persists immediately | Toggle on + missing command LLM key shows hint |
| Command provider/model | Existing settings loading spinner | Model can be custom text when not in list | Missing API key/base URL shows inline hint | Provider/model persists immediately | Command provider can differ from STT provider |
| Command recording | Existing overlay Recording | Too short / silent error | STT or LLM error in overlay | Done state then paste | LLM succeeds but paste fails -> clipboard fallback |
| History command row | Existing history loading | "No command outputs yet" comes from general empty state | Shows failed command error | Labels instruction/result | Long output collapses under existing card layout |

### AI Slop Risk

Do not add a large marketing-style card. This is a dense settings surface. Use the existing `ToggleSwitch`, `HotkeyPicker`, provider select, and custom model input patterns, with one concise safety hint.

### Accessibility and Responsive

- Use a visible `<label>` for the toggle.
- Keep the hotkey picker keyboard-operable by reusing the existing component.
- Body text remains at existing settings text sizes and contrast.
- Long zh-TW hint wraps in the same two-column settings row without clipping.

## Engineering Review

### System Architecture

```text
Command hotkey press
  │
  ▼
HotkeyManager::register_command_combo()
  │
  ▼
handle_listen_command_hotkey_event()
  │ press                                      release
  ├── controller.on_start_recording()          ├── recorder.stop()
  ├── recorder.start()                         ├── validate pcm
  └── overlay Recording                        ├── on_stop_recording_stt_only()
                                                ├── task_command::generate()
                                                ├── history.insert(kind=listen_command)
                                                ├── paste generated output
                                                └── overlay Done/Error
```

### Error and Rescue Map

| Codepath | What can go wrong | Error class | Rescue action | User sees |
|---|---|---|---|---|
| `set_hotkey(kind="listen_command")` | empty shortcut | `String` error | reject before persistence | "Hotkey cannot be empty" |
| command hotkey registration | OS/global shortcut failure | `String` error | collect with other hotkey errors | settings error |
| recorder start | microphone permission/device failure | `AppError::Audio` | emit formatted audio error | overlay error |
| recorder stop | backend stop failure | `AppError::Audio` | reset pipeline and log | overlay clears |
| STT command transcription | timeout/401/429/5xx/malformed JSON | `AppError::Transcription` / `ApiKeyMissing` / `Network` | existing STT path handles visible error | overlay error, failed history if audio persisted |
| LLM command generation | missing key | `AppError::ApiKeyMissing` | emit error, do not paste | overlay error |
| LLM command generation | empty prompt | `AppError::Command` | reject before API call | overlay error |
| LLM command generation | empty response | `AppError::Command` | emit error, do not paste | overlay error |
| paste | clipboard or key simulation failure | `AppError::Paste` | existing clipboard fallback where available | output remains on clipboard or error log |

### Security Model

- API keys remain Rust-only.
- Spoken commands are user intent, but the system prompt must say the model only generates pasteable text and must not claim it executed actions.
- No arbitrary command execution.
- No file reads.
- No active clipboard capture for command mode in v1.
- No app content capture beyond active app name, which is already collected for paste focus and auto-tone.

### Test Coverage Goal

Every new branch gets tests:

```text
CODE PATHS                                           USER FLOWS
[+] settings.rs                                      [+] Enable Listen to My Command
  ├── default disabled + default hotkey                  ├── settings roundtrip
  ├── old JSON migration                                 └── UI toggle updates setting
[+] prompts/task_command.rs                         [+] Speak instruction
  ├── empty command rejected                             ├── STT success -> LLM output -> paste
  ├── system prompt forbids OS execution claim           ├── too short -> visible error
  ├── LLM success                                        ├── silence -> visible error
  ├── LLM empty choices                                  └── LLM error -> no paste
  └── provider route path
[+] history.rs                                      [+] History
  ├── kind default migration                             ├── command row labels Instruction/Result
  └── command kind serializes                            └── old rows still render
```

## File Structure

Create:

- `src-tauri/crates/voxpen-core/src/pipeline/task_command.rs`
  - Owns command-mode prompt, user message formatting, LLM generation wrapper, empty output validation, and tests.

Modify:

- `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`
  - Export `task_command`.
- `src-tauri/crates/voxpen-core/src/error.rs`
  - Add `AppError::Command` so user-facing failures say "Command failed" instead of "Refinement failed."
- `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`
  - Add `listen_command_enabled`, `hotkey_listen_command`, `listen_command_provider`, `listen_command_model`, and `listen_command_custom_base_url`.
- `src-tauri/crates/voxpen-core/src/history.rs`
  - Add `TranscriptionKind`, `kind`, `llm_provider`, and `llm_model` fields with serde defaults.
- `src-tauri/src/history.rs`
  - Add SQLite migrations for `kind`, `llm_provider`, and `llm_model`.
- `src-tauri/src/hotkey.rs`
  - Register the command hotkey and add command-mode press/release handler.
- `src-tauri/src/commands.rs`
  - Support `set_hotkey(kind="listen_command")`.
- `src-tauri/src/lib.rs`
  - Register the new hotkey during startup.
- `src/types/settings.ts`
  - Add new settings fields and defaults.
- `src/types/history.ts`
  - Add `kind`.
- `src/components/Settings/GeneralSection.tsx`
  - Add toggle and hotkey picker.
- `src/components/Settings/ListenCommandSection.tsx`
  - Add command provider/model selector, custom model input, and custom OpenAI-compatible base URL input.
- `src/components/Settings/SettingsWindow.tsx`
  - Add the Listen Command section/tab to the settings navigation, following the app's existing Settings structure.
- `src/components/History/HistoryEntry.tsx`
  - Label command entries as Instruction/Result.
- `src/locales/en.json`
  - Add English copy.
- `src/locales/zh-TW.json`
  - Add Traditional Chinese copy.
- `docs/ROADMAP.md`
  - Mark the new feature as planned after implementation.
- `docs/voice-commands.md`
  - Add a clarification that "Voice Commands" means punctuation commands, not "Listen to My Command."

---

### Task 1: Add Command Mode Settings

**Files:**

- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`
- Modify: `src/types/settings.ts`

- [ ] **Step 1: Write failing Rust settings tests**

Add to the existing `#[cfg(test)] mod tests` in `settings.rs`:

```rust
#[test]
fn should_default_listen_command_to_disabled_with_combo_hotkey() {
    let s = Settings::default();
    assert!(!s.listen_command_enabled);
    assert_eq!(s.hotkey_listen_command, "CommandOrControl+Shift+L");
    assert_eq!(s.listen_command_provider, "openai");
    assert_eq!(s.listen_command_model, "gpt-5.2");
    assert_eq!(s.listen_command_custom_base_url, "");
}

#[test]
fn should_roundtrip_listen_command_settings() {
    let mut s = Settings::default();
    s.listen_command_enabled = true;
    s.hotkey_listen_command = "CommandOrControl+Alt+L".to_string();
    s.listen_command_provider = "openrouter".to_string();
    s.listen_command_model = "anthropic/claude-haiku-4.5".to_string();
    s.listen_command_custom_base_url = "https://openrouter.ai/api/".to_string();

    let json = serde_json::to_string(&s).unwrap();
    let back: Settings = serde_json::from_str(&json).unwrap();

    assert!(back.listen_command_enabled);
    assert_eq!(back.hotkey_listen_command, "CommandOrControl+Alt+L");
    assert_eq!(back.listen_command_provider, "openrouter");
    assert_eq!(back.listen_command_model, "anthropic/claude-haiku-4.5");
    assert_eq!(back.listen_command_custom_base_url, "https://openrouter.ai/api/");
}

#[test]
fn should_deserialize_old_settings_without_listen_command_fields() {
    let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
    let s: Settings = serde_json::from_str(json).unwrap();

    assert!(!s.listen_command_enabled);
    assert_eq!(s.hotkey_listen_command, "CommandOrControl+Shift+L");
    assert_eq!(s.listen_command_provider, "openai");
    assert_eq!(s.listen_command_model, "gpt-5.2");
    assert_eq!(s.listen_command_custom_base_url, "");
}
```

- [ ] **Step 2: Run the failing tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml listen_command -- --nocapture
```

Expected: FAIL with missing listen-command settings fields.

- [ ] **Step 3: Add Rust settings fields**

Add these fields after `hotkey_edit` in `Settings`:

```rust
/// Whether "Listen to My Command" mode is enabled.
/// This mode turns a spoken instruction into a pasteable LLM-generated artifact.
#[serde(default)]
pub listen_command_enabled: bool,
/// Hotkey for "Listen to My Command" mode. Combo shortcuts only.
#[serde(default = "default_hotkey_listen_command")]
pub hotkey_listen_command: String,
/// LLM provider used only for Listen to My Command generation.
/// This is independent from STT and refinement providers.
#[serde(default = "default_listen_command_provider")]
pub listen_command_provider: String,
/// LLM model used only for Listen to My Command generation.
#[serde(default = "default_listen_command_model")]
pub listen_command_model: String,
/// Custom OpenAI-compatible base URL used when `listen_command_provider == "custom"`.
#[serde(default)]
pub listen_command_custom_base_url: String,
```

Add the default function near the existing hotkey defaults:

```rust
fn default_hotkey_listen_command() -> String {
    "CommandOrControl+Shift+L".to_string()
}

fn default_listen_command_provider() -> String {
    "openai".to_string()
}

fn default_listen_command_model() -> String {
    // OpenAI model list checked 2026-05-07:
    // https://platform.openai.com/docs/models
    "gpt-5.2".to_string()
}
```

Add to `impl Default for Settings`:

```rust
listen_command_enabled: false,
hotkey_listen_command: default_hotkey_listen_command(),
listen_command_provider: default_listen_command_provider(),
listen_command_model: default_listen_command_model(),
listen_command_custom_base_url: String::new(),
```

- [ ] **Step 4: Add TypeScript settings fields**

Add to `src/types/settings.ts`:

```ts
listen_command_enabled: boolean;
hotkey_listen_command: string;
listen_command_provider: string;
listen_command_model: string;
listen_command_custom_base_url: string;
```

Add to `defaultSettings`:

```ts
listen_command_enabled: false,
hotkey_listen_command: "CommandOrControl+Shift+L",
listen_command_provider: "openai",
listen_command_model: "gpt-5.2",
listen_command_custom_base_url: "",
```

- [ ] **Step 5: Verify tests pass**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml listen_command -- --nocapture
pnpm build
```

Expected: Rust tests PASS and TypeScript build PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs src/types/settings.ts
git commit -m "feat: add listen command settings"
```

---

### Task 2: Add Transcription History Kind and Command LLM Metadata

**Files:**

- Modify: `src-tauri/crates/voxpen-core/src/history.rs`
- Modify: `src-tauri/src/history.rs`
- Modify: `src/types/history.ts`

- [ ] **Step 1: Write failing core history tests**

Add to `src-tauri/crates/voxpen-core/src/history.rs` tests:

```rust
fn command_entry() -> TranscriptionEntry {
    TranscriptionEntry {
        id: "cmd-123".to_string(),
        timestamp: 1_700_000_002,
        original_text: "write a debounce function".to_string(),
        refined_text: Some("function debounce() {}".to_string()),
        language: Language::Auto,
        audio_duration_ms: 3_000,
        provider: "groq".to_string(),
        status: TranscriptionStatus::Completed,
        error_message: None,
        audio_path: None,
        kind: TranscriptionKind::ListenCommand,
        llm_provider: Some("openai".to_string()),
        llm_model: Some("gpt-5.2".to_string()),
    }
}

#[test]
fn should_serialize_listen_command_kind() {
    let entry = command_entry();
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains(r#""kind":"listen_command""#));
}

#[test]
fn should_default_missing_kind_to_dictation() {
    let json = r#"{"id":"old","timestamp":1,"original_text":"hello","refined_text":null,"language":"Auto","audio_duration_ms":1000,"provider":"groq","status":"completed","error_message":null,"audio_path":null}"#;
    let entry: TranscriptionEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.kind, TranscriptionKind::Dictation);
    assert_eq!(entry.llm_provider, None);
    assert_eq!(entry.llm_model, None);
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml history::tests::should_serialize_listen_command_kind history::tests::should_default_missing_kind_to_dictation -- --nocapture
```

Expected: FAIL because `TranscriptionKind`, `kind`, `llm_provider`, and `llm_model` do not exist.

- [ ] **Step 3: Add core history type and SQL**

Add after `TranscriptionStatus`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptionKind {
    Dictation,
    VoiceEdit,
    ListenCommand,
}

impl TranscriptionKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Dictation => "dictation",
            Self::VoiceEdit => "voice_edit",
            Self::ListenCommand => "listen_command",
        }
    }

    pub fn from_db(value: &str) -> Self {
        match value {
            "voice_edit" => Self::VoiceEdit,
            "listen_command" => Self::ListenCommand,
            _ => Self::Dictation,
        }
    }
}

fn default_kind() -> TranscriptionKind {
    TranscriptionKind::Dictation
}
```

Add to `TranscriptionEntry`:

```rust
#[serde(default = "default_kind")]
pub kind: TranscriptionKind,
#[serde(default)]
pub llm_provider: Option<String>,
#[serde(default)]
pub llm_model: Option<String>,
```

Update SQL constants:

```rust
CREATE TABLE IF NOT EXISTS transcriptions (
    id TEXT PRIMARY KEY NOT NULL,
    timestamp INTEGER NOT NULL,
    original_text TEXT NOT NULL,
    refined_text TEXT,
    language TEXT NOT NULL,
    audio_duration_ms INTEGER NOT NULL,
    provider TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',
    error_message TEXT,
    audio_path TEXT,
    kind TEXT NOT NULL DEFAULT 'dictation',
    llm_provider TEXT,
    llm_model TEXT
)
```

Add `kind`, `llm_provider`, and `llm_model` to `INSERT_SQL`, `QUERY_SQL`, `SEARCH_SQL`, and `GET_BY_ID_SQL`.

- [ ] **Step 4: Update SQLite adapter**

In `src-tauri/src/history.rs`, update insert params to include:

```rust
entry.kind.as_str(),
entry.llm_provider.as_deref(),
entry.llm_model.as_deref(),
```

Update row mapping to read the new column:

```rust
kind: voxpen_core::history::TranscriptionKind::from_db(row.get::<_, String>(10)?.as_str()),
llm_provider: row.get(11)?,
llm_model: row.get(12)?,
```

Add a migration in `HistoryDb::open()` after existing additive migrations:

```rust
let _ = conn.execute(
    "ALTER TABLE transcriptions ADD COLUMN kind TEXT NOT NULL DEFAULT 'dictation'",
    [],
);
let _ = conn.execute("ALTER TABLE transcriptions ADD COLUMN llm_provider TEXT", []);
let _ = conn.execute("ALTER TABLE transcriptions ADD COLUMN llm_model TEXT", []);
```

- [ ] **Step 5: Set kind at existing call sites**

Set existing normal dictation entries in `src-tauri/src/hotkey.rs`:

```rust
kind: voxpen_core::history::TranscriptionKind::Dictation,
llm_provider: None,
llm_model: None,
```

Set Voice Edit entries in `do_voice_edit_stop()`:

```rust
kind: voxpen_core::history::TranscriptionKind::VoiceEdit,
llm_provider: Some(refinement_provider.clone()),
llm_model: Some(refinement_model.clone()),
```

Set file transcription entries in `src-tauri/src/commands.rs`:

```rust
kind: voxpen_core::history::TranscriptionKind::Dictation,
llm_provider: None,
llm_model: None,
```

- [ ] **Step 6: Update TypeScript type**

Add to `src/types/history.ts`:

```ts
kind: "dictation" | "voice_edit" | "listen_command";
llm_provider: string | null;
llm_model: string | null;
```

- [ ] **Step 7: Verify**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml history -- --nocapture
pnpm build
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/history.rs src-tauri/src/history.rs src-tauri/src/hotkey.rs src-tauri/src/commands.rs src/types/history.ts
git commit -m "feat: tag transcription history entries by kind"
```

---

### Task 3: Add Command LLM Module

**Files:**

- Create: `src-tauri/crates/voxpen-core/src/pipeline/task_command.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`
- Modify: `src-tauri/crates/voxpen-core/src/error.rs`

- [ ] **Step 0: Add a command-specific error variant**

Add to `AppError` in `error.rs`:

```rust
#[error("Command failed: {0}")]
Command(String),
```

Add a display test:

```rust
#[test]
fn should_display_command_error() {
    let err = AppError::Command("empty output".to_string());
    assert_eq!(err.to_string(), "Command failed: empty output");
}
```

- [ ] **Step 1: Write the new module with tests first**

Create `task_command.rs` with this initial test module and public API skeleton:

```rust
use crate::api::groq::{self, ChatConfig};
use crate::error::AppError;

pub const LISTEN_COMMAND_SYSTEM_PROMPT: &str = "\
You are VoxPen's Listen to My Command engine.
The user speaks an instruction and expects a pasteable text artifact.

Core behavior:
1. Follow the user's requested writing or coding task.
2. Output only the deliverable text, code, checklist, email, spec, or note.
3. Do not add explanations unless the user asks for explanations.
4. Do not claim that you executed shell commands, edited files, sent messages, clicked buttons, or changed external systems.
5. If the user asks for direct system execution, output the safest pasteable instructions or draft instead.
6. Preserve the user's requested language. If unclear, use the language of the instruction.
7. For code, prefer complete, copy-pasteable snippets with imports when needed.";

pub fn user_message(command: &str, active_app: Option<&str>) -> Result<String, AppError> {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return Err(AppError::Command("no command to execute".to_string()));
    }

    let app_line = active_app
        .filter(|s| !s.trim().is_empty())
        .map(|s| format!("Active app: {s}\n"))
        .unwrap_or_default();

    Ok(format!("{app_line}Spoken instruction:\n{trimmed}"))
}

pub async fn generate(
    command: &str,
    active_app: Option<&str>,
    config: &ChatConfig,
    provider: &str,
    custom_base_url: &str,
) -> Result<String, AppError> {
    let user = user_message(command, active_app)?;
    let base_url = if provider == "custom" && !custom_base_url.is_empty() {
        custom_base_url
    } else {
        groq::base_url_for_provider(provider)
    };

    let output = groq::chat_completion_with_provider(
        config,
        LISTEN_COMMAND_SYSTEM_PROMPT,
        &user,
        provider,
        base_url,
    )
    .await?;

    let trimmed = output.trim();
    if trimmed.is_empty() {
        return Err(AppError::Command("empty command output from LLM".to_string()));
    }

    Ok(trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn chat_response(content: &str) -> serde_json::Value {
        serde_json::json!({
            "choices": [{
                "message": { "role": "assistant", "content": content }
            }]
        })
    }

    #[test]
    fn should_reject_empty_command() {
        let err = user_message("   ", None).unwrap_err();
        assert_eq!(err.to_string(), "Command failed: no command to execute");
    }

    #[test]
    fn should_include_active_app_when_present() {
        let msg = user_message("write code", Some("Code")).unwrap();
        assert!(msg.contains("Active app: Code"));
        assert!(msg.contains("write code"));
    }

    #[test]
    fn system_prompt_should_not_claim_direct_execution() {
        assert!(LISTEN_COMMAND_SYSTEM_PROMPT.contains("Do not claim"));
        assert!(LISTEN_COMMAND_SYSTEM_PROMPT.contains("edited files"));
        assert!(LISTEN_COMMAND_SYSTEM_PROMPT.contains("clicked buttons"));
    }

    #[tokio::test]
    async fn should_generate_command_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("```ts\nconst x = 1;\n```")))
            .expect(1)
            .mount(&server)
            .await;

        let config = ChatConfig::new("key".to_string());
        let output = generate("write TypeScript", None, &config, "groq", &format!("{}/", server.uri()))
            .await
            .unwrap();

        assert!(output.contains("const x = 1"));
    }

    #[tokio::test]
    async fn should_use_openai_compatible_custom_provider_path() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("custom provider output")))
            .expect(1)
            .mount(&server)
            .await;

        let config = ChatConfig::new("key".to_string());
        let output = generate("draft a reply", None, &config, "custom", &format!("{}/", server.uri()))
            .await
            .unwrap();

        assert_eq!(output, "custom provider output");
    }

    #[tokio::test]
    async fn should_reject_empty_llm_output() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/openai/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("   ")))
            .mount(&server)
            .await;

        let config = ChatConfig::new("key".to_string());
        let err = generate("write something", None, &config, "groq", &format!("{}/", server.uri()))
            .await
            .unwrap_err();

        assert_eq!(err.to_string(), "Command failed: empty command output from LLM");
    }
}
```

- [ ] **Step 2: Export the module**

Add to `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`:

```rust
pub mod task_command;
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml task_command -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/task_command.rs src-tauri/crates/voxpen-core/src/pipeline/mod.rs src-tauri/crates/voxpen-core/src/error.rs
git commit -m "feat: add listen command LLM prompt"
```

---

### Task 4: Register Listen Command Hotkey

**Files:**

- Modify: `src-tauri/src/hotkey.rs`
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Extend `HotkeyManager` fields**

In `HotkeyManager`, add:

```rust
registered_listen_command: Option<String>,
```

Initialize it in `HotkeyManager::new()`:

```rust
registered_listen_command: None,
```

Clear it in `unregister_all()`:

```rust
self.registered_listen_command = None;
```

- [ ] **Step 2: Extend `register_all()` signature**

Change:

```rust
pub fn register_all(
    &mut self,
    app: &AppHandle,
    ptt_shortcut: &str,
    toggle_shortcut: &str,
    edit_shortcut: &str,
) -> Result<(), String>
```

to:

```rust
pub fn register_all(
    &mut self,
    app: &AppHandle,
    ptt_shortcut: &str,
    toggle_shortcut: &str,
    edit_shortcut: &str,
    listen_command_enabled: bool,
    listen_command_shortcut: &str,
) -> Result<(), String>
```

Add registration after Voice Edit:

```rust
if listen_command_enabled && !listen_command_shortcut.is_empty() && is_combo_shortcut(listen_command_shortcut) {
    match self.register_listen_command_combo(app, listen_command_shortcut) {
        Ok(()) => self.registered_listen_command = Some(listen_command_shortcut.to_string()),
        Err(e) => errors.push(e),
    }
}
```

- [ ] **Step 3: Add combo registration function**

Add near `register_edit_combo()`:

```rust
fn register_listen_command_combo(&self, app: &AppHandle, shortcut: &str) -> Result<(), String> {
    let app_handle = app.clone();
    let processing = self.rdev_state.processing.clone();

    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _shortcut, event| {
            let app = app_handle.clone();
            let state: tauri::State<'_, AppState> = app.state();
            handle_listen_command_hotkey_event(&app, &state, event.state, &processing, true);
        })
        .map_err(|e| format!("Failed to register listen command shortcut '{}': {}", shortcut, e))?;

    Ok(())
}
```

- [ ] **Step 4: Update startup registration**

In `src-tauri/src/lib.rs`, update the `register_all()` call:

```rust
mgr.register_all(
    app.handle(),
    &saved_settings.hotkey_ptt,
    &saved_settings.hotkey_toggle,
    &saved_settings.hotkey_edit,
    saved_settings.listen_command_enabled,
    &saved_settings.hotkey_listen_command,
)
```

- [ ] **Step 5: Update runtime hotkey command**

In `src-tauri/src/commands.rs::set_hotkey`, add a `kind` branch:

```rust
"listen_command" => s.hotkey_listen_command = shortcut.clone(),
```

Update `register_all()` call:

```rust
mgr.register_all(
    &app,
    &settings_clone.hotkey_ptt,
    &settings_clone.hotkey_toggle,
    &settings_clone.hotkey_edit,
    settings_clone.listen_command_enabled,
    &settings_clone.hotkey_listen_command,
)?;
```

- [ ] **Step 6: Confirm `save_settings()` needs no hotkey re-registration**

Read `src-tauri/src/commands.rs::save_settings()`. It persists settings and updates the pipeline controller config, but does not call `register_all()`. Leave it that way in this task; runtime hotkey changes continue to go through `set_hotkey()`.

- [ ] **Step 7: Verify compile**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/hotkey.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: register listen command hotkey"
```

---

### Task 5: Implement Listen Command Hotkey Handler

**Files:**

- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1: Add handler skeleton**

Add after `handle_edit_hotkey_event()`:

```rust
fn handle_listen_command_hotkey_event(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    shortcut_state: ShortcutState,
    processing: &Arc<AtomicBool>,
    is_combo: bool,
) {
    let is_recording = state.recording_started.load(Ordering::SeqCst);
    let action = resolve_action(
        shortcut_state,
        &RecordingMode::HoldToRecord,
        is_recording,
        is_combo,
    );

    match action {
        HotkeyAction::Ignore => {}
        HotkeyAction::Start => start_listen_command_recording(app, state, processing),
        HotkeyAction::Stop => stop_listen_command_recording(app, state, processing),
    }
}
```

- [ ] **Step 2: Add `start_listen_command_recording()`**

Add:

```rust
fn start_listen_command_recording(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    processing: &Arc<AtomicBool>,
) {
    if processing
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let controller = state.controller.clone();
    let recorder = state.recorder.clone();
    let recording_started = state.recording_started.clone();
    let processing_flag = processing.clone();
    let app_for_err = app.clone();
    let audio_ducker = state.audio_ducker.clone();
    let settings = state.settings.clone();

    recording_started.store(false, Ordering::SeqCst);

    tauri::async_runtime::spawn(async move {
        let ctrl = controller.lock().await;
        if let Err(e) = ctrl.on_start_recording() {
            let _ = app_for_err.emit("pipeline-state", &PipelineState::Error { message: e.to_string() });
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
        drop(ctrl);

        match recorder.start() {
            Ok(()) => {
                recording_started.store(true, Ordering::SeqCst);
                let s = settings.lock().await;
                if s.audio_ducking_enabled {
                    if let Err(e) = audio_ducker.duck(s.audio_ducking_volume) {
                        eprintln!("listen command: audio ducking failed (non-fatal): {e}");
                    }
                }
            }
            Err(e) => {
                let msg = format_audio_error(&e);
                let _ = app_for_err.emit("pipeline-state", &PipelineState::Error { message: msg });
                processing_flag.store(false, Ordering::SeqCst);
            }
        }
    });
}
```

- [ ] **Step 3: Add `stop_listen_command_recording()`**

Add:

```rust
fn stop_listen_command_recording(
    app: &AppHandle,
    state: &tauri::State<'_, AppState>,
    processing: &Arc<AtomicBool>,
) {
    let controller = state.controller.clone();
    let recorder = state.recorder.clone();
    let clipboard = state.clipboard.clone();
    let keyboard = state.keyboard.clone();
    let settings = state.settings.clone();
    let history = state.history.clone();
    let dictionary = state.dictionary.clone();
    let license_mgr = state.license_manager.clone();
    let app_handle = app.clone();
    let recording_started = state.recording_started.clone();
    let processing_flag = processing.clone();
    let timeout_handle = state.recording_timeout_handle.clone();
    #[cfg(target_os = "linux")]
    let focused_window_id = crate::active_window::get_focused_window_id();
    #[cfg(not(target_os = "linux"))]
    let focused_window_id: Option<String> = None;
    let active_app = crate::active_window::get_active_app_name();
    let audio_ducker = state.audio_ducker.clone();

    tauri::async_runtime::spawn(async move {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(3);
        while !recording_started.load(Ordering::SeqCst) {
            if tokio::time::Instant::now() >= deadline {
                eprintln!("listen command: recording never started, aborting");
                let ctrl = controller.lock().await;
                ctrl.reset();
                drop(ctrl);
                processing_flag.store(false, Ordering::SeqCst);
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        recording_started.store(false, Ordering::SeqCst);

        if let Err(e) = audio_ducker.restore() {
            eprintln!("listen command: audio restore failed (non-fatal): {e}");
        }

        if let Some(h) = timeout_handle.lock().await.take() {
            h.abort();
        }

        let pcm_data = match recorder.stop() {
            Ok(data) => data,
            Err(e) => {
                eprintln!("listen command: audio stop error: {e}");
                let ctrl = controller.lock().await;
                ctrl.reset();
                drop(ctrl);
                processing_flag.store(false, Ordering::SeqCst);
                return;
            }
        };

        do_listen_command_stop(
            app_handle,
            controller,
            clipboard,
            keyboard,
            settings,
            history,
            dictionary,
            license_mgr,
            pcm_data,
            processing_flag,
            focused_window_id,
            active_app,
        )
        .await;
    });
}
```

- [ ] **Step 4: Verify compile fails at missing `do_listen_command_stop()`**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: FAIL with missing `do_listen_command_stop`.

- [ ] **Step 5: Commit is not allowed yet**

Do not commit this partial state. Continue to Task 6.

---

### Task 6: Implement `do_listen_command_stop()`

**Files:**

- Modify: `src-tauri/src/hotkey.rs`

- [ ] **Step 1: Add the stop implementation**

Add after `do_voice_edit_stop()`:

```rust
#[allow(clippy::too_many_arguments, unused_variables)]
async fn do_listen_command_stop(
    app: tauri::AppHandle,
    controller: Arc<
        tokio::sync::Mutex<
            voxpen_core::pipeline::controller::PipelineController<
                crate::state::GroqSttProvider,
                crate::state::GroqLlmProvider,
            >,
        >,
    >,
    clipboard: Arc<dyn voxpen_core::input::clipboard::ClipboardManager>,
    keyboard: Arc<dyn voxpen_core::input::paste::KeySimulator>,
    settings: Arc<tokio::sync::Mutex<voxpen_core::pipeline::settings::Settings>>,
    history: Arc<crate::history::HistoryDb>,
    dictionary: Arc<crate::dictionary::DictionaryDb>,
    license_mgr: Arc<
        voxpen_core::licensing::LicenseManager<
            voxpen_core::licensing::DirectLemonSqueezy,
            crate::licensing::TauriLicenseStore,
            crate::licensing::SqliteUsageDb,
        >,
    >,
    pcm_data: Vec<i16>,
    processing_flag: Arc<std::sync::atomic::AtomicBool>,
    focused_window_id: Option<String>,
    active_app: Option<String>,
) {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;
    #[cfg(not(target_os = "linux"))]
    use voxpen_core::input::paste::paste_text;

    let pcm_len = pcm_data.len();

    if pcm_len < 4000 {
        let ctrl = controller.lock().await;
        ctrl.emit_error("Command recording was too short. Speak for at least 0.25 seconds.".to_string());
        drop(ctrl);
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    if voxpen_core::audio::is_silent(&pcm_data) {
        let ctrl = controller.lock().await;
        ctrl.emit_error("No speech detected. Please try again.".to_string());
        drop(ctrl);
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    let vocab_words = dictionary.get_words(500).unwrap_or_default();
    let (stt_lang, command_provider, command_model, command_custom_base_url, auto_paste) = {
        let s = settings.lock().await;
        (
            s.stt_language.clone(),
            s.listen_command_provider.clone(),
            s.listen_command_model.clone(),
            s.listen_command_custom_base_url.clone(),
            s.auto_paste,
        )
    };
    let vocabulary_hint = voxpen_core::pipeline::vocabulary::build_stt_hint(&vocab_words, &stt_lang);

    let ctrl = controller.lock().await;
    let spoken_command = match ctrl.on_stop_recording_stt_only(pcm_data, vocabulary_hint).await {
        Ok(text) => text,
        Err(e) => {
            eprintln!("listen command: STT failed: {e}");
            ctrl.emit_error(e.to_string());
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
    };
    drop(ctrl);

    let voice_status = license_mgr.check_category(voxpen_core::licensing::UsageCategory::VoiceInput);
    if voice_status == voxpen_core::licensing::UsageStatus::Exhausted {
        let ctrl = controller.lock().await;
        ctrl.emit_error("Daily voice limit reached. Upgrade to Pro for unlimited access.".to_string());
        drop(ctrl);
        let _ = app.emit("usage-exhausted", ());
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    // v1 metering reuses the existing AI quota category to avoid license-plan migration.
    // User-facing copy must say command/AI action, not refinement.
    let command_status = license_mgr.check_category(voxpen_core::licensing::UsageCategory::Refinement);
    if command_status == voxpen_core::licensing::UsageStatus::Exhausted {
        let ctrl = controller.lock().await;
        ctrl.emit_error("Daily AI command limit reached. Upgrade to Pro for unlimited access.".to_string());
        drop(ctrl);
        let _ = app.emit("usage-exhausted", ());
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }

    let llm_key = match crate::state::get_api_key_from_handle(&app, &command_provider) {
        Ok(k) => k,
        Err(e) => {
            let ctrl = controller.lock().await;
            ctrl.emit_error(format!("Command LLM API key not configured: {e}"));
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
    };

    let chat_config = voxpen_core::api::groq::ChatConfig {
        api_key: llm_key,
        model: command_model.clone(),
        temperature: voxpen_core::api::groq::LLM_TEMPERATURE,
        max_tokens: 4096,
    };

    let generated = match tokio::time::timeout(
        std::time::Duration::from_secs(45),
        voxpen_core::pipeline::task_command::generate(
            &spoken_command,
            active_app.as_deref(),
            &chat_config,
            &command_provider,
            &command_custom_base_url,
        ),
    )
    .await
    {
        Ok(Ok(text)) => text,
        Ok(Err(e)) => {
            let ctrl = controller.lock().await;
            ctrl.emit_error(format!("Listen to My Command failed: {e}"));
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
        Err(_) => {
            let ctrl = controller.lock().await;
            ctrl.emit_error("Listen to My Command timed out after 45s".to_string());
            drop(ctrl);
            processing_flag.store(false, Ordering::SeqCst);
            return;
        }
    };

    {
        let ctrl = controller.lock().await;
        ctrl.emit_refined(spoken_command.clone(), generated.clone());
        drop(ctrl);
    }

    let entry = {
        let s = settings.lock().await;
        voxpen_core::history::TranscriptionEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            original_text: spoken_command,
            refined_text: Some(generated.clone()),
            language: s.stt_language.clone(),
            audio_duration_ms: (pcm_len as u64 * 1000) / 16000,
            provider: s.stt_provider.clone(),
            status: voxpen_core::history::TranscriptionStatus::Completed,
            error_message: None,
            audio_path: None,
            kind: voxpen_core::history::TranscriptionKind::ListenCommand,
            llm_provider: Some(command_provider.clone()),
            llm_model: Some(command_model.clone()),
        }
    };

    if let Err(e) = history.insert(&entry) {
        eprintln!("listen command history insert error: {e}");
    }

    let _ = license_mgr.record_usage(voxpen_core::licensing::UsageCategory::VoiceInput);
    // Same quota bucket as refinement in v1, but separate provider/model and prompt.
    let _ = license_mgr.record_usage(voxpen_core::licensing::UsageCategory::Refinement);
    let _ = app.emit("usage-updated", ());

    if auto_paste {
        {
            let ctrl = controller.lock().await;
            ctrl.reset();
            drop(ctrl);
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        #[cfg(target_os = "linux")]
        {
            let original = clipboard.get_text().unwrap_or(None);
            let script_path = crate::paste_script_path(&app);
            let mut cmd = std::process::Command::new("setsid");
            cmd.arg(&script_path).arg(&generated);
            if let Some(ref orig) = original {
                cmd.arg(orig);
            }
            if let Some(ref wid) = focused_window_id {
                cmd.env("VOXPEN_WINDOW_ID", wid);
            }
            if let Some(ref app_name) = active_app {
                cmd.env("VOXPEN_ACTIVE_APP", app_name);
            }
            if let Err(e) = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
                eprintln!("listen command: paste script spawn failed: {e}");
            }
        }

        #[cfg(not(target_os = "linux"))]
        {
            let text = generated.clone();
            let cb = clipboard.clone();
            let kb = keyboard.clone();
            match tokio::task::spawn_blocking(move || paste_text(cb.as_ref(), kb.as_ref(), &text)).await {
                Ok(Err(e)) => eprintln!("listen command: paste failed: {e}"),
                Err(e) => eprintln!("listen command: paste task panicked: {e}"),
                Ok(Ok(())) => {}
            }
        }
    }

    processing_flag.store(false, Ordering::SeqCst);
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    let ctrl = controller.lock().await;
    match ctrl.current_state() {
        PipelineState::Refined { .. } | PipelineState::Error { .. } => ctrl.reset(),
        _ => {}
    }
    drop(ctrl);
}
```

- [ ] **Step 2: Run compile**

Run:

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 3: Run focused Rust tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml task_command settings history -- --nocapture
```

Expected: PASS.

- [ ] **Step 4: Commit Tasks 5 and 6 together**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat: add listen command hotkey pipeline"
```

---

### Task 7: Add Settings UI and Command Model Picker

**Files:**

- Modify: `src/components/Settings/GeneralSection.tsx`
- Create: `src/components/Settings/ListenCommandSection.tsx`
- Modify: `src/components/Settings/SettingsWindow.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

- [ ] **Step 1: Add locale strings**

Add to `src/locales/en.json` near Voice Edit strings:

```json
"listenCommandEnabled": "Listen to My Command",
"listenCommandEnabledHint": "Speak an instruction and paste an AI-generated result. It does not run commands or edit files.",
"hotkeyListenCommand": "Listen Command Hotkey",
"hotkeyListenCommandHint": "Hold this combo, speak a task, and VoxPen pastes the generated result.",
"listenCommandTab": "Command",
"listenCommandProvider": "Command Provider",
"listenCommandProviderHint": "Used only for Listen to My Command. Your transcription provider can be different.",
"listenCommandModel": "Command Model",
"listenCommandModelHint": "Choose a preset or type a custom model ID.",
"listenCommandCustomModel": "Custom model ID",
"listenCommandCustomModelPlaceholder": "e.g. gpt-5.2, openai/gpt-oss-120b, anthropic/claude-haiku-4.5",
"listenCommandCustomBaseUrl": "Custom Base URL",
"listenCommandCustomBaseUrlHint": "For OpenAI-compatible providers such as a local server or a custom proxy. OpenRouter can use the built-in OpenRouter provider.",
"listenCommandApiKey": "Command API Key",
"listenCommandSafetyHint": "This mode asks the selected model to generate pasteable output. It does not execute shell commands, click apps, or modify files."
```

Add to `src/locales/zh-TW.json`:

```json
"listenCommandEnabled": "聽我指令",
"listenCommandEnabledHint": "說出任務指令並貼上 AI 產出的結果。此功能不會執行系統指令或直接修改檔案。",
"hotkeyListenCommand": "聽我指令快捷鍵",
"hotkeyListenCommandHint": "按住此組合鍵，說出任務，VoxPen 會貼上產出的結果。",
"listenCommandTab": "指令",
"listenCommandProvider": "指令模型供應商",
"listenCommandProviderHint": "只用於聽我指令。語音轉文字的供應商可以不同。",
"listenCommandModel": "指令模型",
"listenCommandModelHint": "選擇內建模型，或自行輸入模型 ID。",
"listenCommandCustomModel": "自訂模型 ID",
"listenCommandCustomModelPlaceholder": "例如 gpt-5.2、openai/gpt-oss-120b、anthropic/claude-haiku-4.5",
"listenCommandCustomBaseUrl": "自訂 Base URL",
"listenCommandCustomBaseUrlHint": "用於 OpenAI-compatible 的自訂供應商，例如本機服務或 proxy。OpenRouter 可直接選內建 OpenRouter。",
"listenCommandApiKey": "指令 API 金鑰",
"listenCommandSafetyHint": "此模式會請選定模型產生可貼上的結果，不會執行 shell、操作其他 App 或直接修改檔案。"
```

- [ ] **Step 2: Add UI controls**

In `GeneralSection.tsx`, insert after Voice Edit Hotkey:

```tsx
{/* Listen to My Command */}
<div className="flex items-center justify-between">
  <div>
    <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
      {t("listenCommandEnabled")}
    </label>
    <p className="text-xs text-gray-400 dark:text-gray-500">
      {t("listenCommandEnabledHint")}
    </p>
  </div>
  <ToggleSwitch
    id="listen-command-enabled"
    checked={settings.listen_command_enabled}
    onChange={(v) => onUpdate("listen_command_enabled", v)}
  />
</div>

<HotkeyPicker
  label={t("hotkeyListenCommand")}
  hint={t("hotkeyListenCommandHint")}
  currentValue={settings.hotkey_listen_command}
  kind="listen_command"
  onSaved={(s) => onUpdate("hotkey_listen_command", s)}
  t={t}
/>
```

- [ ] **Step 3: Add dedicated command LLM section**

Create `ListenCommandSection.tsx` using the provider/model UI pattern from `RefinementSection.tsx`, but bind to the command-only settings:

```tsx
const COMMAND_PROVIDERS = [
  { value: "openai", label: "OpenAI" },
  { value: "groq", label: "Groq" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "custom", label: "Custom OpenAI-compatible" },
] as const;

const COMMAND_MODEL_OPTIONS = {
  openai: [
    { value: "gpt-5.2", label: "GPT-5.2", tag: "recommended" },
    { value: "gpt-5.1", label: "GPT-5.1" },
    { value: "gpt-5", label: "GPT-5" },
    { value: "gpt-5-mini", label: "GPT-5 mini", tag: "budget" },
    { value: "gpt-5-nano", label: "GPT-5 nano", tag: "budget" },
  ],
  groq: [
    { value: "openai/gpt-oss-120b", label: "GPT-OSS 120B", tag: "recommended" },
    { value: "openai/gpt-oss-20b", label: "GPT-OSS 20B", tag: "budget" },
    { value: "qwen/qwen3-32b", label: "Qwen3 32B", tag: "multilingual" },
  ],
  openrouter: [
    { value: "openai/gpt-5.2", label: "OpenAI GPT-5.2", tag: "recommended" },
    { value: "anthropic/claude-haiku-4.5", label: "Claude Haiku 4.5" },
    { value: "google/gemini-3-flash", label: "Gemini 3 Flash", tag: "budget" },
  ],
};
```

Rules:

- Built-in OpenAI presets should include only model IDs verified against official OpenAI docs at implementation time. The plan checked `platform.openai.com/docs/models` on 2026-05-07 and did not find `gpt-5.5`.
- Always show a custom model ID input. This lets a user type `gpt-5.5` or any future/provider-specific model ID without a VoxPen release.
- `listen_command_provider === "custom"` must show `listen_command_custom_base_url`.
- OpenRouter should be a first-class provider, not forced through Custom, because the backend already has an OpenRouter base URL and provider-specific headers.
- API key status/save should call `getApiKeyStatus(settings.listen_command_provider)` and `saveApiKey(settings.listen_command_provider, key)`.
- The copy must say command provider/model, not refinement provider/model.

- [ ] **Step 4: Add SettingsWindow tab/section**

Add `ListenCommandSection` to `SettingsWindow.tsx`:

```tsx
import ListenCommandSection from "./ListenCommandSection";

type Tab = "license" | "general" | "speech" | "refinement" | "listenCommand" | "dictionary" | "fileTranscription" | "appearance" | "history";
```

Insert `"listenCommand"` after `"refinement"` in `TAB_IDS`. Add a `TabIcon` branch using the same compact SVG style as existing tabs. Add content rendering:

```tsx
{activeTab === "listenCommand" && (
  <ListenCommandSection settings={settings} onUpdate={updateSetting} />
)}
```

Use `t("listenCommandTab")` for the sidebar label.

- [ ] **Step 5: Verify frontend build**

Run:

```bash
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/Settings/GeneralSection.tsx src/components/Settings/ListenCommandSection.tsx src/components/Settings/SettingsWindow.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add listen command settings UI"
```

---

### Task 8: Add History Labels for Command Entries

**Files:**

- Modify: `src/components/History/HistoryEntry.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

- [ ] **Step 1: Add locale keys**

Add to `src/locales/en.json`:

```json
"instruction": "Instruction",
"result": "Result",
"edited": "Edited",
"sttProvider": "STT",
"aiProvider": "AI"
```

Add to `src/locales/zh-TW.json`:

```json
"instruction": "指令",
"result": "結果",
"edited": "編輯後",
"sttProvider": "語音",
"aiProvider": "AI"
```

- [ ] **Step 2: Add label helper**

In `HistoryEntry.tsx`, add near the top of the component:

```tsx
const originalLabel =
  entry.kind === "listen_command" ? t("instruction") :
  entry.kind === "voice_edit" ? t("original") :
  t("original");

const refinedLabel =
  entry.kind === "listen_command" ? t("result") :
  entry.kind === "voice_edit" ? t("edited") :
  t("refined");
```

- [ ] **Step 3: Replace hard-coded original/refined labels**

Use `originalLabel` for `entry.original_text` and `refinedLabel` for `entry.refined_text`.

- [ ] **Step 4: Display command LLM provider/model separately**

Keep the existing provider badge as the STT provider. For entries with `llm_provider`, add a second compact badge:

```tsx
{entry.llm_provider && (
  <span className="rounded bg-blue-50 px-1.5 py-0.5 text-[10px] text-blue-600 dark:bg-blue-900/30 dark:text-blue-300">
    {t("aiProvider")}: {entry.llm_provider}
    {entry.llm_model ? ` / ${entry.llm_model}` : ""}
  </span>
)}
```

Change the existing provider badge label to make the split clear:

```tsx
{t("sttProvider")}: {entry.provider}
```

- [ ] **Step 5: Verify build**

Run:

```bash
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/History/HistoryEntry.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: label listen command history entries"
```

---

### Task 9: Update Docs

**Files:**

- Modify: `docs/voice-commands.md`
- Modify: `docs/ROADMAP.md`

- [ ] **Step 1: Clarify naming in voice command docs**

Add near the top of `docs/voice-commands.md`:

```markdown
> Note: Voice Commands are punctuation and formatting substitutions such as "comma" and "new line." The separate **Listen to My Command** feature is for spoken tasks that produce AI-generated output.
```

- [ ] **Step 2: Add roadmap entry**

Add to the relevant feature table in `docs/ROADMAP.md`:

```markdown
| ✅ Listen to My Command | Speak a task -> generate a pasteable artifact such as code, specs, email, or tasks |
```

Add a short section:

```markdown
### Listen to My Command

**Status:** ✅ Shipped — Plan: `docs/superpowers/plans/2026-05-07-listen-command.md`

**What:** User holds a dedicated hotkey, speaks an instruction, and VoxPen pastes an LLM-generated result into the active app.

**Safety boundary:** v1 only generates pasteable text. It does not run shell commands, edit files directly, or automate apps.
```

- [ ] **Step 3: Commit**

```bash
git add docs/voice-commands.md docs/ROADMAP.md
git commit -m "docs: document listen command mode"
```

---

### Task 10: Full Verification

**Files:** no new files.

- [ ] **Step 1: Run Rust format**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
```

Expected: no output or formatted files only.

- [ ] **Step 2: Run core tests**

Run:

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 3: Run full Tauri tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 4: Run clippy**

Run:

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: PASS.

- [ ] **Step 5: Run frontend build**

Run:

```bash
pnpm build
```

Expected: PASS.

- [ ] **Step 6: Manual smoke test on desktop**

Run:

```bash
pnpm tauri:dev
```

Expected manual behavior:

1. Open Settings -> General.
2. Enable `Listen to My Command`.
3. Confirm hotkey displays `CommandOrControl+Shift+L`.
4. Open Settings -> Command.
5. Set Transcription provider to Groq or OpenAI in Speech settings, then set Command provider to a different provider such as OpenAI or OpenRouter.
6. Pick a command model from the list, then type a custom model ID and confirm it persists.
7. For Custom provider, set an OpenAI-compatible base URL and confirm command generation uses `/v1/chat/completions`.
8. Put cursor in a text editor.
9. Hold the command hotkey and say: "Write a JavaScript debounce function."
10. Release the hotkey.
11. Overlay shows Recording -> Processing -> Done.
12. Text editor receives a pasteable code snippet.
13. History shows kind `listen_command` with labels Instruction and Result.
14. Ask for a shell action: "delete all temp files." Expected pasted output is safe explanatory text, not a claim that deletion happened.

- [ ] **Step 7: Final formatting commit if verification changed files**

```bash
git status --short
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs \
  src-tauri/crates/voxpen-core/src/history.rs \
  src-tauri/crates/voxpen-core/src/pipeline/task_command.rs \
  src-tauri/crates/voxpen-core/src/pipeline/mod.rs \
  src-tauri/src/history.rs \
  src-tauri/src/hotkey.rs \
  src-tauri/src/commands.rs \
  src-tauri/src/lib.rs
git commit -m "chore: format listen command changes"
```

Skip this commit if `git status --short` is clean.

---

## Failure Modes Registry

| Codepath | Failure mode | Rescued? | Test? | User sees? | Logged? |
|---|---|---|---|---|---|
| settings deserialize | old settings missing new fields | Yes | Yes | no visible change | no |
| hotkey register | shortcut conflict | Yes | compile/manual | settings error | yes via returned error |
| recorder start | mic unavailable | Yes | existing pattern | overlay error | no |
| STT command | provider timeout | Yes | existing STT tests | overlay error | yes |
| task LLM | missing API key | Yes | manual/unit via key path | overlay error | no secret logged |
| task LLM | empty command | Yes | Yes | overlay error | no |
| task LLM | empty response | Yes | Yes | overlay error | yes |
| paste | paste simulation fails | Partially | existing paste tests | clipboard fallback/log | yes |
| history insert | SQLite write failure | Partially | existing history tests | output still pasted | yes |

Critical gaps after this plan: none if all tests are implemented.

## NOT in Scope

- Direct filesystem edits: unsafe without app-specific review UI and rollback.
- Shell execution: unsafe for a global speech hotkey.
- IDE context capture: requires per-app permissions and provider-specific context rules.
- Prompt preset library: useful, but v1 prompt can infer format from speech.
- Streaming generated output: more UI and state-machine work than needed for v1.
- Command result retry from history: later feature after command entries exist.

## What Already Exists

- Global hotkey registration in `src-tauri/src/hotkey.rs`.
- STT-only recording path in `PipelineController::on_stop_recording_stt_only()`.
- Purpose-built Voice Edit pipeline in `do_voice_edit_stop()`.
- LLM provider routing in `GroqLlmProvider` and `api::groq::chat_completion_with_provider()`.
- History schema and migration pattern in `voxpen-core/src/history.rs` and `src-tauri/src/history.rs`.
- Settings UI pattern in `GeneralSection.tsx` using `ToggleSwitch` and `HotkeyPicker`.
- Overlay states via existing `PipelineState::Recording`, `Processing`, `Refined`, and `Error`.

## Worktree Parallelization Strategy

Sequential implementation is recommended for Tasks 1-6 because `settings.rs`, `history.rs`, and `hotkey.rs` are shared integration surfaces.

Parallel lanes after Task 6:

| Step | Modules touched | Depends on |
|---|---|---|
| Settings UI | `src/components/Settings`, `src/types`, `src/locales` | Tasks 1, 4 |
| History labels | `src/components/History`, `src/types`, `src/locales` | Task 2 |
| Docs | `docs` | product decision only |

Execution order:

```text
Lane A: Tasks 1 -> 2 -> 3 -> 4 -> 5 -> 6 (sequential Rust foundation)
Lane B: Task 7 (parallel after Task 6 compile passes)
Lane C: Task 8 (parallel after Task 2 schema is merged)
Lane D: Task 9 (parallel any time after plan acceptance)
Final: Task 10 verification after all lanes merge
```

Conflict flags:

- Lane B and Lane C both touch `src/locales/en.json` and `src/locales/zh-TW.json`; coordinate or do sequential locale edits.
- Tasks 4-6 all touch `src-tauri/src/hotkey.rs`; keep them in one lane.

## GSTACK REVIEW REPORT

| Review | Trigger | Why | Runs | Status | Findings |
|--------|---------|-----|------|--------|----------|
| CEO Review | `/plan-ceo-review` | Scope & strategy | 1 | clear | Picked bounded pasteable artifact mode over unsafe full agent execution |
| Codex Review | `/codex review` | Independent 2nd opinion | 0 | not run | Not run during plan creation |
| Eng Review | `/plan-eng-review` | Architecture & tests (required) | 1 | clear | Reuse hotkey/STT/LLM/history/paste; add purpose-built command module and full tests |
| Design Review | `/plan-design-review` | UI/UX gaps | 1 | clear | Settings-only UI; safety boundary copy; history labels for command entries |
| DX Review | `/plan-devex-review` | Developer experience gaps | 0 | not run | Not needed for this user-facing feature plan |

- **UNRESOLVED:** 0
- **VERDICT:** CEO + DESIGN + ENG plan reviews complete. Ready to implement in `feat/listen-command` worktree after user approval.
