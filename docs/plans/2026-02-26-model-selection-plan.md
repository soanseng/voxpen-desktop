# Model Selection & Multi-Provider Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Update VoxPen's refinement model lists, add OpenRouter as a new provider, implement proper multi-provider API routing, add model tags in the UI, and add a custom model name input field.

**Architecture:** Replace the Groq-only `chat_completion()` call with a generic OpenAI-compatible client that accepts configurable base URLs. Each provider (Groq, OpenAI, OpenRouter) has its own base URL but shares the same `/v1/chat/completions` endpoint format. Anthropic is removed as a direct provider (users access Claude via OpenRouter instead). A "Custom" provider supports user-defined base URL + model for Ollama/self-hosted setups.

**Tech Stack:** Rust (voxpen-core crate), React + TypeScript (frontend), Tauri v2

**Design doc:** See user's model research in the conversation above.

---

### Task 1: Add provider base URLs and update API module

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/api/mod.rs`
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs`

**Step 1: Write test**

Add to `groq.rs` tests:

```rust
#[test]
fn should_resolve_base_url_for_known_providers() {
    assert_eq!(base_url_for_provider("groq"), "https://api.groq.com/");
    assert_eq!(base_url_for_provider("openai"), "https://api.openai.com/");
    assert_eq!(base_url_for_provider("openrouter"), "https://openrouter.ai/api/");
}
```

**Step 2: Run test, verify fail**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml -- base_url`

**Step 3: Implement**

Add to `api/mod.rs`:

```rust
pub const GROQ_BASE_URL: &str = "https://api.groq.com/";
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/";
pub const OPENROUTER_BASE_URL: &str = "https://openrouter.ai/api/";
```

Move `GROQ_BASE_URL` from existing location, add the two new constants.

Add to `groq.rs` (or a shared location):

```rust
/// Resolve the API base URL for a given provider name.
/// Returns the known base URL for built-in providers, or the provider string
/// itself if it looks like a URL (for custom/Ollama setups).
pub fn base_url_for_provider(provider: &str) -> &str {
    match provider {
        "groq" => GROQ_BASE_URL,
        "openai" => OPENAI_BASE_URL,
        "openrouter" => OPENROUTER_BASE_URL,
        _ => provider, // custom: treat provider string as base URL
    }
}
```

Update `chat_completion()` to accept a `provider` parameter (or make `chat_completion_with_base_url` public).

Also update the error message in the 401 handler — currently hardcoded to `"groq"`:
```rust
if status == reqwest::StatusCode::UNAUTHORIZED {
    return Err(AppError::ApiKeyMissing(provider.to_string()));
}
```

**Step 4: Run tests, verify pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/api/
git commit -m "feat: add multi-provider base URL resolution (Groq, OpenAI, OpenRouter)"
```

---

### Task 2: Update chat_completion to support multi-provider routing

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs`

**Step 1: Write tests**

```rust
#[tokio::test]
async fn should_add_openrouter_headers_when_provider_is_openrouter() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("HTTP-Referer", "https://voxpen.app"))
        .and(header("X-Title", "VoxPen"))
        .respond_with(ResponseTemplate::new(200).set_body_json(chat_response("refined")))
        .expect(1)
        .mount(&server)
        .await;

    let config = ChatConfig::new("test-key".to_string());
    let result = chat_completion_with_provider(
        &config, "prompt", "text", "openrouter",
        &format!("{}/", server.uri()),
    ).await;
    assert!(result.is_ok());
}
```

**Step 2: Run test, verify fail**

**Step 3: Implement**

Rename `chat_completion_with_base_url` to `chat_completion_with_provider` and add a `provider: &str` parameter. Key changes:

1. URL path: For OpenAI and OpenRouter, use `/v1/chat/completions` (no `openai/` prefix). For Groq, keep `openai/v1/chat/completions`.

```rust
let path = if provider == "groq" {
    "openai/v1/chat/completions"
} else {
    "v1/chat/completions"
};
let url = format!("{base_url}{path}");
```

2. For OpenRouter, add required headers:
```rust
let mut req = client.post(&url).bearer_auth(&config.api_key).json(&body);
if provider == "openrouter" {
    req = req.header("HTTP-Referer", "https://voxpen.app")
             .header("X-Title", "VoxPen");
}
```

3. Update error to include provider name:
```rust
return Err(AppError::ApiKeyMissing(provider.to_string()));
```

Update the public `chat_completion()` function and `refine.rs` to use the new signature.

**Step 4: Run tests, verify pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/api/groq.rs
git commit -m "feat: chat_completion supports multi-provider routing with OpenRouter headers"
```

---

### Task 3: Update refine.rs to pass provider through

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/refine.rs`

**Step 1: Update refine() signature**

Add `provider: &str` parameter to `refine()`:

```rust
pub async fn refine(
    text: &str,
    config: &ChatConfig,
    language: &Language,
    vocab_words: &[String],
    custom_prompt: &str,
    tone_preset: &TonePreset,
    provider: &str,
) -> Result<String, AppError> {
```

Replace `groq::chat_completion(config, &system_prompt, text)` with:
```rust
let base_url = base_url_for_provider(provider);
groq::chat_completion_with_provider(config, &system_prompt, text, provider, base_url).await
```

**Step 2: Update existing tests**

Add `"groq"` as provider argument to all existing test calls.

**Step 3: Run tests, verify pass**

Run: `cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml`

**Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/refine.rs
git commit -m "feat: refine() routes through provider-specific API endpoints"
```

---

### Task 4: Update GroqLlmProvider to pass provider

**Files:**
- Modify: `src-tauri/src/state.rs`

**Step 1: Update GroqLlmProvider.refine()**

Read `s.refinement_provider` and pass it to `refine::refine()`:

```rust
let provider = s.refinement_provider.clone();
// ...
refine::refine(&text, &config, &language, &vocabulary, &custom_prompt, &tone_preset, &provider).await
```

**Step 2: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`

**Step 3: Commit**

```bash
git add src-tauri/src/state.rs
git commit -m "feat: LlmProvider passes refinement_provider for multi-provider routing"
```

---

### Task 5: Add custom_base_url to Settings

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/pipeline/settings.rs`
- Modify: `src/types/settings.ts`

For the "custom" provider, users need to specify their own base URL (e.g., `http://localhost:11434/` for Ollama).

**Step 1: Write test**

```rust
#[test]
fn should_default_custom_base_url_to_empty() {
    let settings = Settings::default();
    assert_eq!(settings.custom_base_url, "");
}

#[test]
fn should_deserialize_old_settings_without_custom_base_url() {
    let json = r#"{"hotkey":"F5","auto_paste":true,...}"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    assert_eq!(settings.custom_base_url, "");
}
```

**Step 2: Add field**

Rust Settings:
```rust
/// Custom API base URL for "custom" provider (e.g., Ollama at http://localhost:11434/)
#[serde(default)]
pub custom_base_url: String,
```

TypeScript Settings:
```typescript
custom_base_url: string;
```

Default: `""` (empty string)

**Step 3: Run tests, verify pass**

**Step 4: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/pipeline/settings.rs src/types/settings.ts
git commit -m "feat: add custom_base_url to Settings for custom/Ollama provider"
```

---

### Task 6: Update base_url_for_provider to handle custom URL

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs` (or wherever `base_url_for_provider` lives)
- Modify: `src-tauri/src/state.rs`

**Step 1: Update GroqLlmProvider to pass custom_base_url**

When provider is "custom", use `settings.custom_base_url` as the base URL instead of the provider name:

```rust
let provider = s.refinement_provider.clone();
let base_url_override = if provider == "custom" {
    Some(s.custom_base_url.clone())
} else {
    None
};
```

Pass this through to `refine::refine()` or resolve it before calling.

Actually, simpler approach: update `refine()` to accept an optional `base_url_override: Option<&str>`. When provided, it takes precedence over `base_url_for_provider()`.

**Step 2: Run tests, verify pass**

**Step 3: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/api/ src-tauri/crates/voxpen-core/src/pipeline/refine.rs src-tauri/src/state.rs
git commit -m "feat: custom provider uses user-defined base URL"
```

---

### Task 7: Update model lists and replace Anthropic with OpenRouter

**Files:**
- Modify: `src/components/Settings/RefinementSection.tsx`
- Modify: `src-tauri/crates/voxpen-core/src/api/groq.rs` (update DEFAULT constants)

**Step 1: Update provider list**

Replace Anthropic with OpenRouter:

```typescript
const REFINEMENT_PROVIDERS = [
  { value: "groq", label: "Groq" },
  { value: "openai", label: "OpenAI" },
  { value: "openrouter", label: "OpenRouter" },
  { value: "custom", label: "Custom / Ollama" },
];
```

**Step 2: Update model lists with tags**

```typescript
interface ModelOption {
  value: string;
  label: string;
  tag?: string;  // "recommended" | "budget" | "multilingual" | "quality"
}

function getModelsForProvider(provider: string): ModelOption[] {
  switch (provider) {
    case "groq":
      return [
        { value: "openai/gpt-oss-120b", label: "GPT-OSS 120B", tag: "recommended" },
        { value: "openai/gpt-oss-20b", label: "GPT-OSS 20B", tag: "budget" },
        { value: "qwen/qwen3-32b", label: "Qwen3 32B", tag: "multilingual" },
      ];
    case "openai":
      return [
        { value: "gpt-5-nano", label: "GPT-5 Nano", tag: "recommended" },
        { value: "gpt-5-mini", label: "GPT-5 Mini", tag: "quality" },
        { value: "gpt-4.1-mini", label: "GPT-4.1 Mini" },
      ];
    case "openrouter":
      return [
        { value: "google/gemini-3-flash", label: "Gemini 3 Flash", tag: "recommended" },
        { value: "anthropic/claude-haiku-4.5", label: "Claude Haiku 4.5", tag: "multilingual" },
        { value: "deepseek/deepseek-chat", label: "DeepSeek Chat", tag: "budget" },
      ];
    default:
      return [];
  }
}
```

**Step 3: Update default model constants in Rust**

In `groq.rs`:
```rust
pub const DEFAULT_LLM_MODEL: &str = "openai/gpt-oss-120b";  // unchanged
pub const ALT_LLM_MODEL: &str = "openai/gpt-oss-20b";       // unchanged
```

In `settings.rs` default: keep `"openai/gpt-oss-120b"` as default refinement_model.

In `settings.ts` defaultSettings: keep `"openai/gpt-oss-120b"`.

**Step 4: Build frontend**

Run: `npx tsc --noEmit && npx vite build`

**Step 5: Commit**

```bash
git add src/components/Settings/RefinementSection.tsx
git commit -m "feat: update model lists — add OpenRouter, update Groq/OpenAI, remove Anthropic"
```

---

### Task 8: Add model tags to dropdown UI

**Files:**
- Modify: `src/components/Settings/RefinementSection.tsx`

**Step 1: Add tag rendering in model dropdown**

Update the model `<select>` to show tags:

```tsx
{models.map((m) => (
  <option key={m.value} value={m.value}>
    {m.label}{m.tag ? ` — ${t(`modelTag.${m.tag}`)}` : ""}
  </option>
))}
```

**Step 2: Add i18n keys for tags**

en.json:
```json
"modelTag": {
  "recommended": "Recommended",
  "budget": "Budget",
  "quality": "Best Quality",
  "multilingual": "Multilingual"
}
```

zh-TW.json:
```json
"modelTag": {
  "recommended": "推薦",
  "budget": "省錢",
  "quality": "品質最佳",
  "multilingual": "多語言"
}
```

**Step 3: Build frontend**

Run: `npx tsc --noEmit`

**Step 4: Commit**

```bash
git add src/components/Settings/RefinementSection.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add model tags (recommended, budget, quality, multilingual) to dropdown"
```

---

### Task 9: Add custom model name input field

**Files:**
- Modify: `src/components/Settings/RefinementSection.tsx`

**Step 1: Add custom model input**

After the model dropdown, show a text input for custom model name that overrides the dropdown when non-empty. This allows users to type any model ID (covering new models not in the preset list).

Design:
- When preset models exist (Groq/OpenAI/OpenRouter): show dropdown + "or type a model name" text field below
- When no preset models (Custom provider): show only the text field
- The text field value binds to `settings.refinement_model` directly
- If the user types in the text field, it overrides the dropdown selection
- If the user selects from dropdown, it clears the text field override

Implementation approach — simpler: just add a small text input below the dropdown with placeholder "Custom model name...". When the user types there, it updates `refinement_model`. When the user picks from the dropdown, the text field is cleared.

For the "Custom / Ollama" provider, also show a base URL input field:
```
Base URL: [http://localhost:11434/          ]
Model:    [llama3.1:8b                      ]
```

**Step 2: Build frontend**

Run: `npx tsc --noEmit && npx vite build`

**Step 3: Commit**

```bash
git add src/components/Settings/RefinementSection.tsx src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add custom model name input and base URL field for Custom provider"
```

---

### Task 10: Update model default on provider change

**Files:**
- Modify: `src/components/Settings/RefinementSection.tsx`

When user switches provider, auto-select the recommended (first) model for that provider.

**Step 1: Update handleProviderChange**

```typescript
function handleProviderChange(provider: string) {
  onUpdate("refinement_provider", provider);
  const newModels = getModelsForProvider(provider);
  if (newModels.length > 0) {
    onUpdate("refinement_model", newModels[0].value);
  } else {
    onUpdate("refinement_model", "");  // Custom provider: empty, user fills in
  }
}
```

This logic already exists but ensure it handles the new providers correctly.

**Step 2: Build and verify**

Run: `npx tsc --noEmit`

**Step 3: Commit (if changes needed)**

---

### Task 11: Full integration test

**Step 1: Run all Rust tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
```

Expected: All tests pass.

**Step 2: Run clippy**

```bash
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

Expected: No warnings.

**Step 3: Build frontend**

```bash
npx tsc --noEmit && npx vite build
```

Expected: Clean build.

**Step 4: Verify model auto-selection**

Manually verify in the plan that provider switching sets the correct default model:
- Groq → `openai/gpt-oss-120b`
- OpenAI → `gpt-5-nano`
- OpenRouter → `google/gemini-3-flash`
- Custom → empty (user fills in)

**Step 5: Commit any fixes**
