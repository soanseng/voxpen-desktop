# Tone Presets Design

Date: 2026-02-26
Status: Approved

## Summary

Add tone/style presets to VoxPen Desktop's LLM refinement system. Instead of a single per-language prompt, users select from 6 tones (Casual, Professional, Email, Note, Social, Custom) that switch the LLM system prompt. Each tone has per-language variants. The existing custom prompt field serves as the "Custom" tone.

## Tones

| Tone | Purpose | Example Output |
|------|---------|----------------|
| Casual | Natural spoken → clean casual text | 今天天氣真好，我們去公園吧。 |
| Professional | Spoken → formal business language | 請問您方便安排會議時間嗎？ |
| Email | Spoken → email-structured text | 您好，\n\n感謝您的回覆。\n\n此致敬禮 |
| Note | Spoken → bullet-point notes | • 天氣：晴\n• 計畫：公園散步 |
| Social | Spoken → social media friendly | 今天天氣超好！出門走走～ |
| Custom | User's own system prompt | (whatever `refinement_prompt` contains) |

## Data Model

### Rust

```rust
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

Added to `Settings`:
```rust
#[serde(default)]
pub tone_preset: TonePreset,
```

Default is `Casual` — preserves existing behavior since current prompts are casual-tone.

### TypeScript

```typescript
type TonePreset = "Casual" | "Professional" | "Email" | "Note" | "Social" | "Custom";

interface Settings {
  // ... existing fields ...
  tone_preset: TonePreset;
}
```

## Prompt Architecture

### New function: `for_language_and_tone()`

```rust
pub fn for_language_and_tone(lang: &Language, tone: &TonePreset) -> &'static str {
    match tone {
        TonePreset::Casual => for_language(lang),  // existing prompts = casual
        TonePreset::Professional => professional_for_language(lang),
        TonePreset::Email => email_for_language(lang),
        TonePreset::Note => note_for_language(lang),
        TonePreset::Social => social_for_language(lang),
        TonePreset::Custom => "",  // caller uses Settings.refinement_prompt
    }
}
```

### Prompt Resolution (in `refine.rs`)

```rust
let system_prompt = match tone_preset {
    TonePreset::Custom if !custom_prompt.is_empty() => custom_prompt.to_string(),
    TonePreset::Custom => prompts::for_language(language).to_string(),  // fallback
    _ => prompts::for_language_and_tone(language, &tone_preset).to_string(),
};
// + vocabulary suffix (unchanged)
```

### Prompt Design Principles

Each tone prompt follows the same structure as existing prompts:
1. Role statement
2. Tone-specific instructions (the key differentiator)
3. Language-specific filler word removal
4. Common rules (no added content, output only)

For languages where tone distinction is meaningful (Japanese keigo, Korean 존댓말), Professional/Email prompts specify the formal register explicitly.

## Prompt Matrix

5 tones × 11 languages = 55 prompt constants. Casual reuses existing 11 prompts. New: 44 constants.

Grouped by tone in separate const blocks within `prompts.rs`. No new files — keeps all prompts co-located.

## Integration Points

### `refine.rs` — Prompt resolution

Current:
```rust
pub async fn refine(text, config, language, vocab_words, custom_prompt)
```

New signature:
```rust
pub async fn refine(text, config, language, vocab_words, custom_prompt, tone_preset)
```

### `state.rs` — GroqLlmProvider

`GroqLlmProvider.refine()` reads `Settings.tone_preset` alongside `refinement_prompt` and passes it to `refine::refine()`.

### `controller.rs` — LlmProvider trait

**No change**. The tone is resolved inside `GroqLlmProvider.refine()` which already reads Settings. The trait stays generic.

### `commands.rs` — get_default_refinement_prompt

Updated to accept optional tone parameter:
```rust
pub async fn get_default_refinement_prompt(language: Language, tone: TonePreset) -> String
```

## Tray Menu

New "Tone" submenu between Language and Microphone:

```
Language      >
Tone          >  ✓ Casual
Microphone    >     Professional
                    Email
                    Note
                    Social
                    Custom
```

Handler: update `Settings.tone_preset`, sync to store and controller.

## Frontend UI

### RefinementSection changes

Add tone dropdown between "Enable Refinement" toggle and Provider selector:

```
[Enable Refinement toggle]
Tone: [Casual ▼]         ← NEW
Provider: [Groq ▼]
Model: [...]
System Prompt: [textarea]  ← only editable when tone = Custom
```

When tone != Custom:
- Textarea shows the tone's built-in prompt as read-only placeholder
- "Reset" button hidden
- "Using [tone name] preset" hint shown

When tone == Custom:
- Textarea editable (existing behavior)
- "Reset" button visible

## i18n

~8 new keys per locale:

```json
"tone": "Tone",
"toneHint": "Choose the style of refined text.",
"toneCasual": "Casual",
"toneProfessional": "Professional",
"toneEmail": "Email",
"toneNote": "Note",
"toneSocial": "Social",
"toneCustom": "Custom"
```

## Licensing Gate

Available to all users (Free + Pro). Tone switching is a prompt-level change with zero extra compute cost. Power users who benefit from tones will naturally hit the 15/day free limit and upgrade.

## File Changes

### Modified (Rust — voxpen-core)

| File | Change |
|------|--------|
| `pipeline/settings.rs` | Add `tone_preset: TonePreset` field |
| `pipeline/state.rs` | Add `TonePreset` enum |
| `pipeline/prompts.rs` | Add `for_language_and_tone()` + 44 new prompt constants |
| `pipeline/refine.rs` | Accept `tone_preset` param, use in prompt resolution |
| `pipeline/mod.rs` | Re-export `TonePreset` |

### Modified (Tauri app)

| File | Change |
|------|--------|
| `src-tauri/src/state.rs` | `GroqLlmProvider.refine()` reads & passes tone_preset |
| `src-tauri/src/lib.rs` | Add Tone submenu to tray menu |
| `src-tauri/src/commands.rs` | Update `get_default_refinement_prompt` signature |

### Modified (Frontend)

| File | Change |
|------|--------|
| `src/types/settings.ts` | Add `TonePreset` type, `tone_preset` field to Settings |
| `src/components/Settings/RefinementSection.tsx` | Add tone dropdown |
| `src/locales/en.json` | Add ~8 tone keys |
| `src/locales/zh-TW.json` | Add ~8 tone keys |

## Testing (~15 new tests)

| Test | Verifies |
|------|----------|
| `for_language_and_tone_casual_matches_existing` | Casual returns same as `for_language()` |
| `for_language_and_tone_professional_zh` | Professional Chinese uses formal register |
| `for_language_and_tone_professional_ja` | Professional Japanese uses keigo |
| `for_language_and_tone_email_structure` | Email prompts mention greeting/sign-off |
| `for_language_and_tone_note_bullets` | Note prompts mention bullet points |
| `for_language_and_tone_social_casual` | Social prompts mention social media |
| `for_language_and_tone_custom_returns_empty` | Custom returns empty string |
| `all_tone_language_combinations_non_empty` | 5×11 = 55 non-empty prompts (except Custom) |
| `refine_uses_tone_preset_prompt` | Refine function selects correct prompt for tone |
| `refine_custom_tone_uses_custom_prompt` | Custom tone uses Settings.refinement_prompt |
| `refine_custom_tone_empty_fallback` | Custom tone + empty prompt falls back to Casual |
| `settings_roundtrip_with_tone` | Settings serialize/deserialize with tone_preset |
| `settings_default_tone_is_casual` | Default tone preset is Casual |
| `settings_backwards_compat_no_tone` | Old settings JSON without tone deserializes (default) |

## Roadmap (not in v1)

- **v2**: Auto-detection based on active window (e.g., Outlook → Email, Slack → Social)
- **v2**: Per-app tone memory
- **v2**: User-created custom preset library (save/name/share presets)
