# Tone Presets Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add tone/style presets (Casual, Professional, Email, Note, Social, Custom) that switch the LLM refinement system prompt per language.

**Architecture:** `TonePreset` enum in `pipeline/state.rs`, 55-entry prompt matrix in `prompts.rs`, tone passed through `refine()` to resolve the system prompt. Frontend gets tone dropdown in RefinementSection + tray submenu.

**Tech Stack:** Rust (voxpen-core crate), React + TypeScript (frontend), Tauri v2 (glue)

**Design doc:** `docs/plans/2026-02-26-tone-presets-design.md`

---

### Task 1: Add TonePreset enum to state.rs

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/state.rs`

**Step 1: Write tests**

Add to the existing `tests` module in `state.rs`:

```rust
#[test]
fn should_have_casual_as_default_tone() {
    assert_eq!(TonePreset::default(), TonePreset::Casual);
}

#[test]
fn should_serialize_tone_preset() {
    let tone = TonePreset::Professional;
    let json = serde_json::to_string(&tone).unwrap();
    assert_eq!(json, "\"Professional\"");
}

#[test]
fn should_deserialize_tone_preset() {
    let tone: TonePreset = serde_json::from_str("\"Email\"").unwrap();
    assert_eq!(tone, TonePreset::Email);
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- tone`
Expected: FAIL — `TonePreset` not found

**Step 3: Implement TonePreset enum**

Add to `state.rs` after the `RecordingMode` enum:

```rust
/// Tone preset for LLM refinement — controls the style of the output text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub enum TonePreset {
    #[default]
    Casual,
    Professional,
    Email,
    Note,
    Social,
    Custom,
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- tone`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/state.rs
git commit -m "feat: add TonePreset enum for refinement tone selection"
```

---

### Task 2: Add tone_preset to Settings

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`

**Step 1: Write tests**

Add to `settings.rs` tests module:

```rust
#[test]
fn should_default_tone_to_casual() {
    let settings = Settings::default();
    assert_eq!(settings.tone_preset, TonePreset::Casual);
}

#[test]
fn should_roundtrip_tone_preset_in_settings() {
    let mut settings = Settings::default();
    settings.tone_preset = TonePreset::Professional;
    let json = serde_json::to_string(&settings).unwrap();
    let deserialized: Settings = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.tone_preset, TonePreset::Professional);
}

#[test]
fn should_deserialize_old_settings_without_tone() {
    // Old settings JSON without tone_preset field should get Casual default
    let json = r#"{"hotkey":"F5","auto_paste":true,"launch_at_login":false,"stt_provider":"groq","stt_language":"Auto","stt_model":"whisper-large-v3-turbo","refinement_enabled":false,"refinement_provider":"groq","refinement_model":"openai/gpt-oss-120b","theme":"system","ui_language":"en"}"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.tone_preset, TonePreset::Casual);
}
```

**Step 2: Run tests, verify fail**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- tone`

**Step 3: Add `tone_preset` field to Settings**

In `settings.rs`, add import `use crate::pipeline::state::TonePreset;` and add the field to the `Settings` struct:

```rust
/// Tone preset for refinement output style (Casual, Professional, Email, Note, Social, Custom)
#[serde(default)]
pub tone_preset: TonePreset,
```

And in `Default` impl: `tone_preset: TonePreset::default(),`

**Step 4: Run tests, verify pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs
git commit -m "feat: add tone_preset field to Settings"
```

---

### Task 3: Add tone-aware prompt functions to prompts.rs

This is the largest task — adds `for_language_and_tone()` and 44 new prompt constants (Professional, Email, Note, Social × 11 languages).

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/prompts.rs`

**Step 1: Write tests**

Add to `prompts.rs` tests module:

```rust
#[test]
fn should_return_casual_same_as_existing() {
    for lang in &[Language::Auto, Language::Chinese, Language::English, Language::Japanese,
                  Language::Korean, Language::French, Language::German, Language::Spanish,
                  Language::Vietnamese, Language::Indonesian, Language::Thai] {
        assert_eq!(
            for_language_and_tone(lang, &TonePreset::Casual),
            for_language(lang),
            "Casual tone should match existing prompt for {:?}", lang
        );
    }
}

#[test]
fn should_return_empty_for_custom_tone() {
    assert_eq!(for_language_and_tone(&Language::English, &TonePreset::Custom), "");
}

#[test]
fn should_return_professional_prompt_with_formal_register() {
    let prompt = for_language_and_tone(&Language::Japanese, &TonePreset::Professional);
    assert!(prompt.contains("敬語") || prompt.contains("ビジネス"),
            "Japanese Professional should reference keigo/business");
}

#[test]
fn should_return_email_prompt_with_structure() {
    let prompt = for_language_and_tone(&Language::English, &TonePreset::Email);
    assert!(prompt.contains("email") || prompt.contains("greeting"),
            "Email prompt should reference email structure");
}

#[test]
fn should_return_note_prompt_with_bullets() {
    let prompt = for_language_and_tone(&Language::English, &TonePreset::Note);
    assert!(prompt.contains("bullet") || prompt.contains("concise"),
            "Note prompt should reference bullet points");
}

#[test]
fn should_return_social_prompt_with_social_style() {
    let prompt = for_language_and_tone(&Language::Chinese, &TonePreset::Social);
    assert!(prompt.contains("社群") || prompt.contains("輕鬆"),
            "Social prompt should reference social media style");
}

#[test]
fn should_have_non_empty_prompts_for_all_tone_language_combinations() {
    let tones = [TonePreset::Casual, TonePreset::Professional,
                 TonePreset::Email, TonePreset::Note, TonePreset::Social];
    let langs = [Language::Auto, Language::Chinese, Language::English, Language::Japanese,
                 Language::Korean, Language::French, Language::German, Language::Spanish,
                 Language::Vietnamese, Language::Indonesian, Language::Thai];
    for tone in &tones {
        for lang in &langs {
            let prompt = for_language_and_tone(lang, tone);
            assert!(!prompt.is_empty(),
                    "Prompt should not be empty for {:?} x {:?}", tone, lang);
        }
    }
}
```

**Step 2: Run tests, verify fail**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- tone`

**Step 3: Implement prompt constants and `for_language_and_tone()`**

Add `use crate::pipeline::state::TonePreset;` import.

Add the public function:

```rust
pub fn for_language_and_tone(lang: &Language, tone: &TonePreset) -> &'static str {
    match tone {
        TonePreset::Casual => for_language(lang),
        TonePreset::Professional => professional_for_language(lang),
        TonePreset::Email => email_for_language(lang),
        TonePreset::Note => note_for_language(lang),
        TonePreset::Social => social_for_language(lang),
        TonePreset::Custom => "",
    }
}
```

Then add 4 private dispatch functions and 44 prompt constants grouped by tone. Each prompt follows the same per-language structure as the existing Casual prompts but with tone-specific instructions.

**Professional prompts** — formal register, keigo for Japanese, 존댓말 for Korean, vous-form for French/German:

Example (Japanese):
```
あなたはビジネス文書の編集アシスタントです。以下の口語内容をビジネスにふさわしい丁寧語・敬語で整理してください：
1. フィラー（えーと、あの、まあ、なんか、ちょっと）を除去
2. 言い直しがある場合は最終的な意味のみ残す
3. ビジネスにふさわしい敬語・丁寧語に変換
4. 適切に句読点を追加
5. 原文にない内容を追加しない
整理後のテキストのみ出力し、説明は不要です。
```

**Email prompts** — greeting + body + closing structure:

Example (Chinese):
```
你是一個語音轉文字的編輯助手。請將以下口語內容整理為適合電子郵件的格式：
1. 移除贅字（嗯、那個、就是、然後、對、呃）
2. 如果說話者中途改口，只保留最終的意思
3. 整理為郵件結構：開頭問候、正文段落、結尾敬語
4. 修正語法但保持原意
5. 適當加入標點符號
6. 保持繁體中文
只輸出整理後的文字，不要加任何解釋。
```

**Note prompts** — bullet points, concise:

Example (English):
```
You are a voice-to-text editor. Convert the following speech into concise bullet-point notes:
1. Remove filler words (um, uh, like, you know, I mean, basically, actually, so)
2. If the speaker corrected themselves mid-sentence, keep only the final version
3. Extract key points as bullet items (use • prefix)
4. Keep each point brief and factual
5. Do not add content that wasn't in the original speech
Output only the bullet-point notes, no explanations.
```

**Social prompts** — casual, emoji-friendly, social media style:

Example (Chinese):
```
你是一個語音轉文字的編輯助手。請將以下口語內容整理為適合社群媒體發文的風格：
1. 移除贅字（嗯、那個、就是、然後、對、呃）
2. 如果說話者中途改口，只保留最終的意思
3. 保持輕鬆活潑的語氣
4. 適當加入標點符號
5. 不要添加原文沒有的內容
6. 保持繁體中文
只輸出整理後的文字，不要加任何解釋。
```

**Step 4: Run tests, verify pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/prompts.rs
git commit -m "feat: add tone-aware prompt matrix (5 tones × 11 languages)"
```

---

### Task 4: Update refine.rs to accept tone_preset

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/refine.rs`

**Step 1: Write test**

Add to `refine.rs` tests:

```rust
#[tokio::test]
async fn should_use_custom_prompt_for_custom_tone() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("refined")))
        .expect(1)
        .mount(&server)
        .await;

    let config = test_config("test-key");
    let result = refine_with_base_url(
        "some text",
        &config,
        &Language::English,
        &format!("{}/", server.uri()),
        "my custom prompt",
        &TonePreset::Custom,
    ).await;
    assert_eq!(result.unwrap(), "refined");
}

#[tokio::test]
async fn should_fallback_to_casual_when_custom_tone_empty_prompt() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/openai/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("refined")))
        .expect(1)
        .mount(&server)
        .await;

    let config = test_config("test-key");
    let result = refine_with_base_url(
        "some text",
        &config,
        &Language::English,
        &format!("{}/", server.uri()),
        "",  // empty custom prompt
        &TonePreset::Custom,
    ).await;
    assert_eq!(result.unwrap(), "refined");
}
```

**Step 2: Run tests, verify fail (signature mismatch)**

**Step 3: Update `refine()` and `refine_with_base_url()` signatures**

Add `tone_preset: &TonePreset` parameter. Update prompt resolution:

```rust
use crate::pipeline::state::{Language, TonePreset};

pub async fn refine(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    vocab_words: &[String],
    custom_prompt: &str,
    tone_preset: &TonePreset,
) -> Result<String, AppError> {
    if text.is_empty() {
        return Err(AppError::Refinement("no text to refine".to_string()));
    }

    let mut system_prompt = match tone_preset {
        TonePreset::Custom if !custom_prompt.is_empty() => custom_prompt.to_string(),
        TonePreset::Custom => prompts::for_language(language).to_string(),
        _ => prompts::for_language_and_tone(language, tone_preset).to_string(),
    };
    if let Some(suffix) = vocabulary::build_llm_suffix(vocab_words, language) {
        system_prompt.push_str(&suffix);
    }
    groq::chat_completion(config, &system_prompt, text).await
}
```

Update existing tests to pass `&TonePreset::Casual` as the new parameter.

Update `refine_with_base_url` similarly.

**Step 4: Run tests, verify pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/refine.rs
git commit -m "feat: refine() accepts tone_preset for prompt selection"
```

---

### Task 5: Re-export TonePreset from pipeline module

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/mod.rs`

**Step 1: Add re-export**

Add `TonePreset` to the public API of the pipeline module (alongside existing re-exports of `Language`, `RecordingMode`, `PipelineState`).

**Step 2: Build to verify**

Run: `cargo build -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 3: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/mod.rs
git commit -m "feat: re-export TonePreset from pipeline module"
```

---

### Task 6: Update GroqLlmProvider to pass tone_preset

**Files:**
- Modify: `src-tauri/src/state.rs`

**Step 1: Update `GroqLlmProvider.refine()`**

In the `refine()` method of `GroqLlmProvider`, read `tone_preset` from settings alongside `custom_prompt`:

```rust
let custom_prompt = s.refinement_prompt.clone();
let tone_preset = s.tone_preset.clone();
drop(s);
refine::refine(&text, &config, &language, &vocabulary, &custom_prompt, &tone_preset).await
```

**Step 2: Build to verify**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`

**Step 3: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat: GroqLlmProvider passes tone_preset to refine()"
```

---

### Task 7: Update get_default_refinement_prompt command

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src/lib/tauri.ts`

**Step 1: Update Rust command**

```rust
#[tauri::command]
pub async fn get_default_refinement_prompt(language: Language, tone: TonePreset) -> String {
    prompts::for_language_and_tone(&language, &tone).to_string()
}
```

Add the necessary imports.

**Step 2: Update TypeScript wrapper**

```typescript
export async function getDefaultRefinementPrompt(language: string, tone: string): Promise<string> {
  return invoke<string>("get_default_refinement_prompt", { language, tone });
}
```

**Step 3: Update call sites in RefinementSection.tsx**

Pass the current tone when calling `getDefaultRefinementPrompt`.

**Step 4: Build both**

Run: `cargo build --manifest-path src-tauri/Cargo.toml && cd /home/scipio/projects/voxpen-desktop && npx tsc --noEmit`

**Step 5: Commit**

```bash
git add src-tauri/src/commands.rs src/lib/tauri.ts src/components/Settings/RefinementSection.tsx
git commit -m "feat: get_default_refinement_prompt accepts tone parameter"
```

---

### Task 8: Add Tone submenu to tray menu

**Files:**
- Modify: `src-tauri/src/lib.rs`

**Step 1: Add ALL_TONES constant**

```rust
const ALL_TONES: &[(&str, TonePreset)] = &[
    ("Casual", TonePreset::Casual),
    ("Professional", TonePreset::Professional),
    ("Email", TonePreset::Email),
    ("Note", TonePreset::Note),
    ("Social", TonePreset::Social),
    ("Custom", TonePreset::Custom),
];
```

**Step 2: Update `build_tray_menu` to accept current_tone and build Tone submenu**

Add `current_tone: &TonePreset` parameter. Build a submenu with CheckMenuItems prefixed `tone_`, between Language and Microphone.

**Step 3: Add tone menu event handler**

In the `on_menu_event` closure, handle `tone_*` events:
- Parse tone from menu id
- Update `Settings.tone_preset`
- Persist to store

**Step 4: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add Tone submenu to system tray menu"
```

---

### Task 9: Add tone dropdown to RefinementSection

**Files:**
- Modify: `src/components/Settings/RefinementSection.tsx`

**Step 1: Add TONE_PRESETS constant**

```typescript
const TONE_PRESETS = [
  { value: "Casual", labelKey: "toneCasual" },
  { value: "Professional", labelKey: "toneProfessional" },
  { value: "Email", labelKey: "toneEmail" },
  { value: "Note", labelKey: "toneNote" },
  { value: "Social", labelKey: "toneSocial" },
  { value: "Custom", labelKey: "toneCustom" },
];
```

**Step 2: Add tone dropdown UI**

Insert between the "Enable Refinement" toggle and the Provider selector. Uses same styling as other selects.

**Step 3: Conditionally disable/enable custom prompt textarea**

When `tone_preset !== "Custom"`:
- Show selected tone's built-in prompt as placeholder (fetch via `getDefaultRefinementPrompt`)
- Disable textarea editing
- Show hint: "Using [tone name] preset"

When `tone_preset === "Custom"`:
- Enable textarea (existing behavior)

**Step 4: Build frontend**

Run: `npx tsc --noEmit && npx vite build`

**Step 5: Commit**

```bash
git add src/components/Settings/RefinementSection.tsx
git commit -m "feat: add tone preset dropdown to refinement settings UI"
```

---

### Task 10: Add TypeScript types and i18n keys

**Files:**
- Modify: `src/types/settings.ts`
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add TonePreset type to settings.ts**

```typescript
export type TonePreset = "Casual" | "Professional" | "Email" | "Note" | "Social" | "Custom";
```

Add `tone_preset: TonePreset;` to Settings interface.

**Step 2: Add i18n keys**

en.json:
```json
"tone": "Tone",
"toneHint": "Choose the style of refined text.",
"toneCasual": "Casual",
"toneProfessional": "Professional",
"toneEmail": "Email",
"toneNote": "Note",
"toneSocial": "Social",
"toneCustom": "Custom",
"toneUsingPreset": "Using {{tone}} preset"
```

zh-TW.json:
```json
"tone": "語氣風格",
"toneHint": "選擇潤飾文字的風格。",
"toneCasual": "輕鬆自然",
"toneProfessional": "商務專業",
"toneEmail": "電子郵件",
"toneNote": "筆記摘要",
"toneSocial": "社群貼文",
"toneCustom": "自訂",
"toneUsingPreset": "使用「{{tone}}」預設風格"
```

**Step 3: Build frontend**

Run: `npx tsc --noEmit`

**Step 4: Commit**

```bash
git add src/types/settings.ts src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add tone preset types and i18n keys"
```

---

### Task 11: Full integration test

**Step 1: Run all Rust tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All tests pass (existing + new tone tests).

**Step 2: Run clippy**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: No warnings.

**Step 3: Build frontend**

```bash
cd /home/scipio/projects/voxpen-desktop && npx tsc --noEmit && npx vite build
```

Expected: Clean build.

**Step 4: Verify test count increased**

Previous test count: ~197 tests. Expected new: ~210+ tests.

**Step 5: Commit any fixes if needed**
