# Voice Commands for Formatting Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users say spoken keywords ("comma", "new line", "逗號", etc.) while dictating, and have them automatically replaced with the corresponding punctuation/formatting character in the final text.

**Architecture:** New `voice_commands` module in `voxpen-core`. Applied as a post-processing step inside `PipelineController::on_stop_recording()` — after STT returns raw text, before optional LLM refinement. Zero new API calls. Works with or without refinement enabled. A `voice_commands_enabled: bool` setting (default off) guards the feature.

**Tech Stack:** Pure Rust string processing (no regex dependency), React + TypeScript, i18next

---

## Supported Commands (v1)

| Spoken keyword | Output | Languages |
|---|---|---|
| "new paragraph" / "新段落" / "새 단락" / "新しい段落" | `\n\n` | EN / ZH / KO / JA |
| "new line" / "新行" / "새 줄" / "改行" | `\n` | EN / ZH / KO / JA |
| "question mark" / "問號" / "물음표" / "疑問符" | `?` | EN / ZH / KO / JA |
| "exclamation mark" / "exclamation point" / "驚嘆號" / "느낌표" | `!` | EN / ZH / KO / JA |
| "full stop" / "period" / "句號" / "마침표" | `.` | EN / ZH / KO / JA |
| "comma" / "逗號" / "쉼표" | `,` | EN / ZH / KO |

**Not in v1 (too complex):** "delete" / "刪除" word deletion — deferred.

---

## Key files

| File | Role |
|------|------|
| `src-tauri/crates/voxpen-core/src/pipeline/voice_commands.rs` | New module — pattern matching & replacement |
| `src-tauri/crates/voxpen-core/src/pipeline/mod.rs` | Export new module |
| `src-tauri/crates/voxpen-core/src/pipeline/settings.rs` | Add `voice_commands_enabled` field |
| `src-tauri/crates/voxpen-core/src/pipeline/controller.rs` | Add `voice_commands_enabled` to `PipelineConfig`; apply in `on_stop_recording()` |
| `src-tauri/src/commands.rs` | `config_from_settings()` — include new field |
| `src/types/settings.ts` | TS interface + default |
| `src/components/Settings/GeneralSection.tsx` | Toggle switch |
| `src/locales/en.json`, `zh-TW.json` | i18n strings |

---

### Task 1: Create `voice_commands` module (TDD)

**Files:**
- Create: `src-tauri/crates/voxpen-core/src/pipeline/voice_commands.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`

**Step 1: Export in mod.rs first**

Add to `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`:

```rust
pub mod voice_commands;
```

**Step 2: Create the module with tests only (RED)**

Create `src-tauri/crates/voxpen-core/src/pipeline/voice_commands.rs`:

```rust
use crate::pipeline::state::Language;

/// Apply voice command substitutions to raw STT output.
///
/// Replaces spoken formatting keywords with punctuation/newline characters.
/// Patterns are checked longest-first to avoid "new line" matching before
/// "new paragraph". Case-insensitive for ASCII patterns.
pub fn apply(text: &str, _lang: &Language) -> String {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::state::Language;

    #[test]
    fn should_return_unchanged_text_with_no_commands() {
        assert_eq!(apply("hello world", &Language::English), "hello world");
    }

    #[test]
    fn should_replace_english_comma() {
        assert_eq!(apply("hello comma world", &Language::English), "hello, world");
    }

    #[test]
    fn should_replace_english_period() {
        assert_eq!(apply("done period", &Language::English), "done.");
    }

    #[test]
    fn should_replace_english_question_mark() {
        assert_eq!(apply("are you sure question mark", &Language::English), "are you sure?");
    }

    #[test]
    fn should_replace_english_new_line() {
        assert_eq!(apply("first new line second", &Language::English), "first\nsecond");
    }

    #[test]
    fn should_replace_english_new_paragraph() {
        assert_eq!(apply("intro new paragraph body", &Language::English), "intro\n\nbody");
    }

    #[test]
    fn should_replace_chinese_comma() {
        assert_eq!(apply("你好逗號世界", &Language::Chinese), "你好,世界");
    }

    #[test]
    fn should_replace_chinese_new_line() {
        assert_eq!(apply("第一行新行第二行", &Language::Chinese), "第一行\n第二行");
    }

    #[test]
    fn should_replace_chinese_new_paragraph() {
        assert_eq!(apply("介紹新段落正文", &Language::Chinese), "介紹\n\n正文");
    }

    #[test]
    fn should_be_case_insensitive_for_english() {
        assert_eq!(apply("Hello Comma World", &Language::English), "Hello, World");
        assert_eq!(apply("COMMA", &Language::English), ",");
    }

    #[test]
    fn should_replace_new_paragraph_before_new_line() {
        // "new paragraph" must not be consumed as "new line" + "paragraph"
        assert_eq!(apply("intro new paragraph body", &Language::English), "intro\n\nbody");
    }

    #[test]
    fn should_handle_multiple_commands_in_sequence() {
        let result = apply("first comma second new line third", &Language::English);
        assert_eq!(result, "first, second\nthird");
    }

    #[test]
    fn should_normalize_extra_spaces_after_replacement() {
        // "done period " → "done."
        let result = apply("done period ", &Language::English);
        assert!(!result.contains("  "), "double space found: {:?}", result);
    }
}
```

**Step 3: Run to confirm FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- voice_commands 2>&1 | tail -15
```
Expected: compile error (todo!() panic) or test failures.

**Step 4: Implement `apply()`**

Replace `todo!()` with the full implementation:

```rust
use crate::pipeline::state::Language;

/// Apply voice command substitutions to raw STT output.
pub fn apply(text: &str, _lang: &Language) -> String {
    // Ordered longest-pattern-first to avoid "new line" consuming "new paragraph".
    const COMMANDS: &[(&str, &str)] = &[
        // 2-word English — must come before 1-word
        ("new paragraph", "\n\n"),
        ("question mark", "?"),
        ("exclamation mark", "!"),
        ("exclamation point", "!"),
        ("full stop", "."),
        ("new line", "\n"),
        // 1-word English
        ("comma", ","),
        ("period", "."),
        // Traditional Chinese (multi-char before single)
        ("新段落", "\n\n"),
        ("疑問符", "?"),
        ("驚嘆號", "!"),
        ("新行", "\n"),
        ("逗號", ","),
        ("句號", "."),
        // Japanese (multi-char before single)
        ("新しい段落", "\n\n"),
        ("改行", "\n"),
        // Korean (multi-word before single)
        ("새 단락", "\n\n"),
        ("새 줄", "\n"),
        ("물음표", "?"),
        ("느낌표", "!"),
        ("마침표", "."),
        ("쉼표", ","),
    ];

    let mut out = text.to_string();
    for (pat, rep) in COMMANDS {
        out = replace_ci(&out, pat, rep);
    }
    normalize_spaces(&out)
}

/// Case-insensitive global string replacement.
/// For ASCII patterns (English), lowercases both sides before matching.
/// For non-ASCII patterns (CJK), exact match (Whisper output is consistent).
fn replace_ci(text: &str, pat: &str, rep: &str) -> String {
    if pat.is_ascii() {
        // Case-insensitive for English keywords
        let lower_text = text.to_lowercase();
        let lower_pat = pat.to_lowercase();
        let mut result = String::with_capacity(text.len());
        let mut last = 0;
        let mut search = 0;
        while let Some(i) = lower_text[search..].find(lower_pat.as_str()) {
            let i = search + i;
            result.push_str(&text[last..i]);
            result.push_str(rep);
            last = i + lower_pat.len();
            search = last;
        }
        result.push_str(&text[last..]);
        result
    } else {
        // Exact match for CJK
        text.replace(pat, rep)
    }
}

/// Collapse runs of spaces (but preserve newlines).
fn normalize_spaces(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    // Trim leading/trailing spaces (but not newlines)
    out.trim_matches(' ').to_string()
}
```

**Step 5: Run to confirm PASS**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- voice_commands 2>&1 | tail -15
```
Expected: all tests pass.

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/voice_commands.rs \
        src-tauri/crates/voxpen-core/src/pipeline/mod.rs
git commit -m "feat: add voice_commands module with regex-free pattern replacement"
```

---

### Task 2: Add `voice_commands_enabled` to Settings and PipelineConfig (TDD)

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/controller.rs`
- Modify: `src-tauri/src/commands.rs`

**Step 1: Write failing tests for Settings**

Add to `settings.rs` tests:

```rust
#[test]
fn should_default_voice_commands_enabled_to_false() {
    let s = Settings::default();
    assert!(!s.voice_commands_enabled);
}

#[test]
fn should_deserialize_old_settings_without_voice_commands() {
    let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert!(!s.voice_commands_enabled);
}
```

**Step 2: Run to confirm FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- settings::tests 2>&1 | tail -5
```

**Step 3: Add field to Settings**

After `translation_target`, add:

```rust
/// Whether to replace spoken formatting keywords with punctuation characters.
/// E.g. "comma" → "," and "new line" → "\n". Applied before LLM refinement.
/// Default: false.
#[serde(default)]
pub voice_commands_enabled: bool,
```

Add to `impl Default for Settings`:

```rust
voice_commands_enabled: false,
```

**Step 4: Add field to PipelineConfig in controller.rs**

In `controller.rs`, `PipelineConfig` struct, add after `llm_model`:

```rust
pub voice_commands_enabled: bool,
```

In `PipelineConfig::new()`, add:

```rust
voice_commands_enabled: false,
```

**Step 5: Update `config_from_settings()` in commands.rs**

```rust
fn config_from_settings(settings: &Settings) -> PipelineConfig {
    PipelineConfig {
        groq_api_key: None,
        language: settings.stt_language.clone(),
        stt_model: settings.stt_model.clone(),
        refinement_enabled: settings.refinement_enabled,
        llm_api_key: None,
        llm_model: settings.refinement_model.clone(),
        voice_commands_enabled: settings.voice_commands_enabled,  // NEW
    }
}
```

**Step 6: Build and test**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- settings 2>&1 | tail -10
```

**Step 7: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs \
        src-tauri/crates/voxpen-core/src/pipeline/controller.rs \
        src-tauri/src/commands.rs
git commit -m "feat: add voice_commands_enabled to Settings and PipelineConfig"
```

---

### Task 3: Wire voice commands into `PipelineController::on_stop_recording()`

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/controller.rs`

**Step 1: Write failing test**

Add to `controller.rs` tests:

```rust
#[tokio::test]
async fn should_apply_voice_commands_when_enabled() {
    let mut cfg = config_with_key();
    cfg.voice_commands_enabled = true;
    // STT returns text with a spoken command
    let controller = PipelineController::new(
        cfg,
        mock_stt_success("hello comma world"),
        mock_llm_unused(),
    );
    controller.on_start_recording().unwrap();
    let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;
    // Voice command "comma" → "," should be applied
    assert_eq!(result.unwrap(), "hello, world");
}

#[tokio::test]
async fn should_not_apply_voice_commands_when_disabled() {
    let cfg = config_with_key(); // voice_commands_enabled = false by default
    let controller = PipelineController::new(
        cfg,
        mock_stt_success("hello comma world"),
        mock_llm_unused(),
    );
    controller.on_start_recording().unwrap();
    let result = controller.on_stop_recording(vec![100, 200], None, vec![]).await;
    // Text passes through unchanged
    assert_eq!(result.unwrap(), "hello comma world");
}
```

**Step 2: Run to confirm FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- controller 2>&1 | tail -15
```

**Step 3: Apply voice commands in `on_stop_recording()`**

In `controller.rs`, in `on_stop_recording()`, add the voice command step right after `raw_text` is obtained from STT and before the refinement check:

```rust
let raw_text = match self.stt.transcribe(pcm_data, vocabulary_hint).await {
    Ok(text) => text,
    Err(e) => { /* existing error handling */ }
};

// Apply voice command substitutions (e.g. "comma" → ",") if enabled.
let raw_text = if self.config.voice_commands_enabled {
    crate::pipeline::voice_commands::apply(&raw_text, &self.config.language)
} else {
    raw_text
};

// If refinement is disabled... (existing code continues)
if !self.config.refinement_enabled {
```

**Step 4: Run to confirm PASS**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- controller 2>&1 | tail -15
```

**Step 5: Full test suite**

```bash
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -5
```
Expected: all tests pass.

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/controller.rs
git commit -m "feat: apply voice commands in pipeline after STT, before LLM refinement"
```

---

### Task 4: TypeScript types

**Files:**
- Modify: `src/types/settings.ts`

**Step 1: Add field to `Settings` interface**

After `translation_target`, add:

```typescript
voice_commands_enabled: boolean;
```

**Step 2: Add to `defaultSettings`**

```typescript
voice_commands_enabled: false,
```

**Step 3: Verify**

```bash
pnpm tsc --noEmit 2>&1 | tail -10
```

**Step 4: Commit**

```bash
git add src/types/settings.ts
git commit -m "feat: add voice_commands_enabled to TypeScript settings types"
```

---

### Task 5: Toggle UI in GeneralSection + i18n

**Files:**
- Modify: `src/components/Settings/GeneralSection.tsx`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Read `GeneralSection.tsx` to find the right place**

Look for the `max_recording_secs` number input — add the voice commands toggle immediately after it.

**Step 2: Add toggle switch component**

`GeneralSection.tsx` may not have a `ToggleSwitch` — check. If not, add a local one (same pattern as `RefinementSection.tsx`):

```tsx
function ToggleSwitch({
  checked,
  onChange,
  id,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  id: string;
}) {
  return (
    <label htmlFor={id} className="relative inline-flex cursor-pointer items-center">
      <input
        id={id}
        type="checkbox"
        className="peer sr-only"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
      />
      <div
        className={
          "h-6 w-11 rounded-full bg-gray-300 transition-colors " +
          "after:absolute after:left-[2px] after:top-[2px] after:h-5 after:w-5 " +
          "after:rounded-full after:bg-white after:transition-transform " +
          "peer-checked:bg-blue-500 peer-checked:after:translate-x-5 " +
          "dark:bg-gray-600 dark:peer-checked:bg-blue-500"
        }
      />
    </label>
  );
}
```

**Step 3: Add the voice commands toggle block**

After the max recording duration input block, add:

```tsx
{/* Voice Commands */}
<div className="flex items-center justify-between">
  <div>
    <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
      {t("voiceCommandsEnabled")}
    </label>
    <p className="text-xs text-gray-400 dark:text-gray-500">
      {t("voiceCommandsEnabledHint")}
    </p>
  </div>
  <ToggleSwitch
    id="voice-commands-enabled"
    checked={settings.voice_commands_enabled}
    onChange={(v) => onUpdate("voice_commands_enabled", v)}
  />
</div>
```

**Step 4: Add i18n strings to `en.json`** (before closing `}`)

```json
"voiceCommandsEnabled": "Voice Commands",
"voiceCommandsEnabledHint": "Say 'comma', 'period', 'new line', 'new paragraph' to insert formatting while dictating."
```

**Step 5: Add i18n strings to `zh-TW.json`** (before closing `}`)

```json
"voiceCommandsEnabled": "語音指令",
"voiceCommandsEnabledHint": "說出「逗號」、「句號」、「新行」、「新段落」等關鍵字來插入標點符號。"
```

**Step 6: Build**

```bash
pnpm build 2>&1 | tail -10
```

**Step 7: Commit**

```bash
git add src/components/Settings/GeneralSection.tsx \
        src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add voice commands toggle to settings UI"
```

---

### Task 6: Update ROADMAP

**Files:**
- Modify: `docs/ROADMAP.md`

**Step 1: Mark Voice Commands as shipped**

Find the `### Voice Commands for Formatting` section and update:
- Status line: `**Status:** ✅ Shipped`

Add to Shipped table:
```
| ✅ Voice Commands for Formatting | "comma" → , · "new line" → \n · supports EN/ZH/JA/KO |
```

Update Typeless comparison table:
```
| Voice commands (punctuation, formatting) | ✅ | ✅ Shipped |
```

**Step 2: Commit**

```bash
git add docs/ROADMAP.md
git commit -m "docs: mark voice commands as shipped in roadmap"
```

---

## Testing end-to-end

1. Settings → General → enable "Voice Commands"
2. Press hotkey, say: "hello comma world new line how are you question mark"
3. Expected paste: `hello, world\nhow are you?`
4. With LLM refinement enabled: verify LLM further polishes the punctuation-inserted text
5. Disable voice commands → verify text passes through unchanged
