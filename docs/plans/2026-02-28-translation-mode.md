# Translation Mode Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Let users speak in one language and receive polished output in another, powered by the existing LLM refinement pipeline.

**Architecture:** Translation replaces the normal cleanup prompt when enabled. A new `for_translation(source, target) -> String` prompt function generates a dynamic instruction; `refine()` gains an optional `translation_target` param that overrides tone/custom prompt selection. No new API calls or pipeline stages — purely a different system prompt.

**Tech Stack:** Rust (`serde`, `thiserror`), React + TypeScript, i18next

---

## Key files

| File | Role |
|------|------|
| `src-tauri/crates/voxpen-core/src/pipeline/settings.rs` | Add `translation_enabled`, `translation_target` fields |
| `src-tauri/crates/voxpen-core/src/pipeline/prompts.rs` | Add `for_translation()` |
| `src-tauri/crates/voxpen-core/src/pipeline/refine.rs` | Add `translation_target` param |
| `src-tauri/src/state.rs` | Wire `GroqLlmProvider` to read translation settings |
| `src/types/settings.ts` | TS interface + defaultSettings |
| `src/components/Settings/RefinementSection.tsx` | Toggle + target language selector |
| `src/locales/en.json`, `zh-TW.json` | i18n strings |

---

### Task 1: Add translation fields to Settings (TDD)

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`

**Step 1: Write failing tests**

Add to the `#[cfg(test)] mod tests` block in `settings.rs`:

```rust
#[test]
fn should_default_translation_enabled_to_false() {
    let s = Settings::default();
    assert!(!s.translation_enabled);
}

#[test]
fn should_default_translation_target_to_english() {
    let s = Settings::default();
    assert_eq!(s.translation_target, Language::English);
}

#[test]
fn should_deserialize_old_settings_without_translation_fields() {
    let json = r#"{"hotkey_ptt":"RAlt","hotkey_toggle":"CommandOrControl+Shift+V","recording_mode":"HoldToRecord","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
    let s: Settings = serde_json::from_str(json).unwrap();
    assert!(!s.translation_enabled);
    assert_eq!(s.translation_target, Language::English);
}

#[test]
fn should_roundtrip_translation_fields() {
    let mut s = Settings::default();
    s.translation_enabled = true;
    s.translation_target = Language::Chinese;
    let json = serde_json::to_string(&s).unwrap();
    let s2: Settings = serde_json::from_str(&json).unwrap();
    assert!(s2.translation_enabled);
    assert_eq!(s2.translation_target, Language::Chinese);
}
```

**Step 2: Run to confirm FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- settings 2>&1 | tail -10
```
Expected: compile error — fields don't exist yet.

**Step 3: Add fields to Settings struct**

After `max_recording_secs`, add:

```rust
/// Whether to translate dictation to a different language instead of just cleaning up.
/// Requires refinement_enabled = true (uses the same LLM pipeline with a translate prompt).
#[serde(default)]
pub translation_enabled: bool,

/// Target language for translation mode.
/// Default: English (most common translation target for CJK users).
#[serde(default = "default_translation_target")]
pub translation_target: Language,
```

Add helper function after `default_max_recording_secs`:

```rust
fn default_translation_target() -> Language {
    Language::English
}
```

Add to `impl Default for Settings`:

```rust
translation_enabled: false,
translation_target: Language::English,
```

**Step 4: Run to confirm PASS**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- settings 2>&1 | tail -10
```
Expected: all settings tests pass.

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs
git commit -m "feat: add translation_enabled and translation_target settings"
```

---

### Task 2: Add `for_translation()` prompt (TDD)

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/prompts.rs`

**Step 1: Write failing tests**

Add to the test block in `prompts.rs`:

```rust
#[cfg(test)]
mod translation_tests {
    use super::*;

    #[test]
    fn should_return_nonempty_translation_prompt_for_all_target_languages() {
        let all = [
            Language::Chinese, Language::English, Language::Japanese,
            Language::Korean, Language::French, Language::German,
            Language::Spanish, Language::Vietnamese, Language::Indonesian,
            Language::Thai, Language::Auto,
        ];
        for target in &all {
            let prompt = for_translation(&Language::Chinese, target);
            assert!(!prompt.is_empty(), "empty prompt for target {:?}", target);
        }
    }

    #[test]
    fn should_include_target_language_name_in_prompt() {
        let prompt = for_translation(&Language::Chinese, &Language::English);
        assert!(prompt.contains("English"), "expected 'English' in prompt: {}", prompt);
    }

    #[test]
    fn should_include_different_target_for_japanese() {
        let prompt = for_translation(&Language::English, &Language::Japanese);
        assert!(prompt.contains("Japanese") || prompt.contains("日本語"));
    }
}
```

**Step 2: Run to confirm FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- translation_tests 2>&1 | tail -10
```

**Step 3: Implement `for_translation()`**

Add after the `for_language_and_tone` dispatch function in `prompts.rs`:

```rust
// ---------------------------------------------------------------------------
// Translation prompt
// ---------------------------------------------------------------------------

/// Returns a dynamic translation prompt for speaking into `_source` and
/// outputting in `target`. Returns an owned `String` because the content
/// varies with the target language.
pub fn for_translation(_source: &Language, target: &Language) -> String {
    let target_name = match target {
        Language::Chinese    => "Traditional Chinese (繁體中文)",
        Language::English    => "English",
        Language::Japanese   => "Japanese (日本語)",
        Language::Korean     => "Korean (한국어)",
        Language::French     => "French (Français)",
        Language::German     => "German (Deutsch)",
        Language::Spanish    => "Spanish (Español)",
        Language::Vietnamese => "Vietnamese (Tiếng Việt)",
        Language::Indonesian => "Indonesian (Bahasa Indonesia)",
        Language::Thai       => "Thai (ภาษาไทย)",
        Language::Auto       => "the most appropriate language",
    };

    format!(
        "You are a voice-to-text translator. Translate the following speech \
transcription into {target}:
1. Translate naturally and fluently — not word-for-word
2. Remove filler words and false starts from the original
3. If the speaker corrected themselves mid-sentence, translate only the final version
4. Add proper punctuation in the target language
5. Do not add content that was not in the original speech
Output only the translated text, no explanations.",
        target = target_name
    )
}
```

**Step 4: Run to confirm PASS**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- translation_tests 2>&1 | tail -10
```

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/prompts.rs
git commit -m "feat: add for_translation() prompt for translation mode"
```

---

### Task 3: Update `refine::refine()` to support translation (TDD)

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/refine.rs`

**Step 1: Write failing tests**

Add to the test block in `refine.rs`:

```rust
#[tokio::test]
async fn should_use_translation_prompt_when_target_is_some() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(chat_response("Hello world")),
        )
        .expect(1)
        .mount(&server)
        .await;

    let config = test_config("test-key");
    let result = refine_with_base_url_and_translation(
        "你好世界",
        &config,
        &Language::Chinese,
        "groq",
        &format!("{}/", server.uri()),
        "",
        &TonePreset::Casual,
        Some(&Language::English),
    )
    .await;

    assert_eq!(result.unwrap(), "Hello world");
}
```

Also add the helper to `refine.rs` (test-only):

```rust
#[cfg(test)]
async fn refine_with_base_url_and_translation(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    provider: &str,
    base_url: &str,
    custom_prompt: &str,
    tone_preset: &TonePreset,
    translation_target: Option<&Language>,
) -> Result<String, AppError> {
    if text.is_empty() {
        return Err(AppError::Refinement("no text to refine".to_string()));
    }
    let system_prompt = if let Some(target) = translation_target {
        prompts::for_translation(language, target)
    } else {
        match tone_preset {
            TonePreset::Custom if !custom_prompt.is_empty() => custom_prompt.to_string(),
            TonePreset::Custom => prompts::for_language(language).to_string(),
            _ => prompts::for_language_and_tone(language, tone_preset).to_string(),
        }
    };
    groq::chat_completion_with_provider(config, &system_prompt, text, provider, base_url).await
}
```

**Step 2: Run to confirm FAIL**

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- refine 2>&1 | tail -15
```

**Step 3: Add `translation_target` parameter to public `refine()` function**

Update the function signature (add one param at the end):

```rust
pub async fn refine(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    vocab_words: &[String],
    custom_prompt: &str,
    tone_preset: &TonePreset,
    provider: &str,
    custom_base_url: &str,
    translation_target: Option<&Language>,  // NEW
) -> Result<String, AppError> {
```

Update the `system_prompt` selection logic:

```rust
let mut system_prompt: String = if let Some(target) = translation_target {
    prompts::for_translation(language, target)
} else {
    match tone_preset {
        TonePreset::Custom if !custom_prompt.is_empty() => custom_prompt.to_string(),
        TonePreset::Custom => prompts::for_language(language).to_string(),
        _ => prompts::for_language_and_tone(language, tone_preset).to_string(),
    }
};
```

**Step 4: Fix the callers** — `state.rs` calls `refine::refine(...)`. Add `None` as the last arg for now (will be wired in Task 4):

In `src-tauri/src/state.rs`, in `GroqLlmProvider::refine()`, find the `refine::refine(...)` call and add `None` at the end:

```rust
refine::refine(
    &text,
    &config,
    &language,
    &vocabulary,
    &custom_prompt,
    &tone_preset,
    &provider,
    &custom_base_url,
    None, // translation_target — wired in next task
)
.await
```

**Step 5: Build and test**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- refine 2>&1 | tail -15
```
Expected: all pass.

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/refine.rs src-tauri/src/state.rs
git commit -m "feat: add translation_target param to refine() function"
```

---

### Task 4: Wire translation through GroqLlmProvider

**Files:**
- Modify: `src-tauri/src/state.rs`

**Step 1: Read translation settings in `GroqLlmProvider::refine()`**

In `state.rs`, update the `GroqLlmProvider::refine()` `Box::pin(async move { ... })` block.

Find where settings are read (currently reads `refinement_provider`, `refinement_model`, etc.) and add:

```rust
let custom_prompt = s.refinement_prompt.clone();
let tone_preset = s.tone_preset.clone();
let provider = s.refinement_provider.clone();
let custom_base_url = s.custom_base_url.clone();
let translation_target = if s.translation_enabled {   // NEW
    Some(s.translation_target.clone())                 // NEW
} else {                                               // NEW
    None                                               // NEW
};                                                     // NEW
drop(s);
```

Then pass `translation_target.as_ref()` to `refine::refine()`:

```rust
refine::refine(
    &text,
    &config,
    &language,
    &vocabulary,
    &custom_prompt,
    &tone_preset,
    &provider,
    &custom_base_url,
    translation_target.as_ref(),  // was None, now wired
)
.await
```

**Step 2: Build**

```bash
cargo build --manifest-path src-tauri/Cargo.toml 2>&1 | tail -20
```
Expected: clean build.

**Step 3: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat: wire translation settings through GroqLlmProvider"
```

---

### Task 5: TypeScript types

**Files:**
- Modify: `src/types/settings.ts`

**Step 1: Add fields to `Settings` interface**

After `max_recording_secs: number;`, add:

```typescript
translation_enabled: boolean;
translation_target: Settings["stt_language"];
```

**Step 2: Add defaults**

In `defaultSettings`, add:

```typescript
translation_enabled: false,
translation_target: "English",
```

**Step 3: Verify no TS errors**

```bash
pnpm tsc --noEmit 2>&1 | tail -20
```
Expected: no errors.

**Step 4: Commit**

```bash
git add src/types/settings.ts
git commit -m "feat: add translation settings to TypeScript types"
```

---

### Task 6: Translation UI in RefinementSection

**Files:**
- Modify: `src/components/Settings/RefinementSection.tsx`

**Step 1: Add translation toggle + target language dropdown**

Import is already `import type { Settings } from "../../types/settings";` — no change needed.

Add `TRANSLATION_TARGETS` constant after `REFINEMENT_PROVIDERS`:

```typescript
const TRANSLATION_TARGETS: { value: Settings["stt_language"]; labelKey: string }[] = [
  { value: "Chinese", labelKey: "chinese" },
  { value: "English", labelKey: "english" },
  { value: "Japanese", labelKey: "japanese" },
  { value: "Korean", labelKey: "korean" },
  { value: "French", labelKey: "french" },
  { value: "German", labelKey: "german" },
  { value: "Spanish", labelKey: "spanish" },
  { value: "Vietnamese", labelKey: "vietnamese" },
  { value: "Indonesian", labelKey: "indonesian" },
  { value: "Thai", labelKey: "thai" },
];
```

**Step 2: Add UI block in JSX**

After the `{/* Enable toggle */}` block and before the `{/* Tone Preset */}` block, insert:

```tsx
{/* Translation Mode */}
<div className={`space-y-3 ${disabled ? "opacity-40" : ""}`}>
  <div className="flex items-center justify-between">
    <div>
      <label className="text-sm font-medium text-gray-700 dark:text-gray-300">
        {t("translationEnabled")}
      </label>
      <p className="text-xs text-gray-400 dark:text-gray-500">
        {t("translationEnabledHint")}
      </p>
    </div>
    <ToggleSwitch
      id="translation-enabled"
      checked={settings.translation_enabled}
      onChange={(v) => onUpdate("translation_enabled", v)}
    />
  </div>

  {settings.translation_enabled && (
    <div className="space-y-1">
      <label
        htmlFor="translation-target"
        className="block text-sm font-medium text-gray-700 dark:text-gray-300"
      >
        {t("translationTarget")}
      </label>
      <select
        id="translation-target"
        value={settings.translation_target}
        onChange={(e) =>
          onUpdate("translation_target", e.target.value as Settings["stt_language"])
        }
        disabled={disabled}
        className={
          "w-full max-w-xs rounded-lg border border-gray-300 bg-white " +
          "px-3 py-2 text-sm text-gray-900 " +
          "focus:border-blue-500 focus:outline-none focus:ring-1 focus:ring-blue-500 " +
          "disabled:cursor-not-allowed " +
          "dark:border-gray-600 dark:bg-gray-800 dark:text-gray-100"
        }
      >
        {TRANSLATION_TARGETS.map((lang) => (
          <option key={lang.value} value={lang.value}>
            {t(lang.labelKey)}
          </option>
        ))}
      </select>
    </div>
  )}
</div>
```

**Step 3: Build frontend**

```bash
pnpm build 2>&1 | tail -20
```
Expected: clean build.

**Step 4: Commit**

```bash
git add src/components/Settings/RefinementSection.tsx
git commit -m "feat: add translation mode toggle and target language selector to settings UI"
```

---

### Task 7: i18n strings

**Files:**
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add to `en.json`** (before the closing `}`)

```json
"translationEnabled": "Translation Mode",
"translationEnabledHint": "Translate your dictation to another language. Requires LLM Refinement to be enabled.",
"translationTarget": "Output Language"
```

**Step 2: Add to `zh-TW.json`** (before the closing `}`)

```json
"translationEnabled": "翻譯模式",
"translationEnabledHint": "將語音翻譯為另一種語言輸出。需啟用文字潤飾功能。",
"translationTarget": "輸出語言"
```

**Step 3: Build and verify**

```bash
pnpm build 2>&1 | tail -10
```

**Step 4: Commit**

```bash
git add src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add i18n strings for translation mode"
```

---

### Task 8: Update ROADMAP

**Files:**
- Modify: `docs/ROADMAP.md`

**Step 1: Mark Translation Mode as shipped**

Find the `### Translation Mode` section and update:
- Status line: `**Status:** ✅ Shipped`

Add to the Shipped table (after Auto-structured output row):
```
| ✅ Translation Mode | Speak in any language, output in any other via LLM translate prompt |
```

Update Typeless comparison table row:
```
| Translation | ✅ | ✅ Shipped |
```

**Step 2: Commit**

```bash
git add docs/ROADMAP.md
git commit -m "docs: mark translation mode as shipped in roadmap"
```

---

## Testing the feature end-to-end

1. Open Settings → Refinement
2. Enable "LLM Refinement"
3. Enable "Translation Mode" → select "English" as output
4. Set STT Language to "Chinese"
5. Press hotkey, say something in Chinese
6. Verify pasted text is in English
7. Disable Translation Mode → verify normal Chinese cleanup is restored
