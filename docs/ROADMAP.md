# VoxPen Desktop — Feature Roadmap

> Last updated: 2026-02-28
> Competitive reference: Typeless (primary benchmark), Wispr Flow

---

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Shipped |
| 🔨 | Has implementation plan |
| 📋 | Planned / on backlog |
| 💡 | Idea / exploratory |

---

## Shipped (v0.x)

| Feature | Notes |
|---------|-------|
| ✅ Global hotkey voice dictation (PTT + Toggle) | RAlt, modifier combos |
| ✅ Floating overlay (recording indicator) | States: Recording, Processing, Done, Error |
| ✅ Auto-paste at cursor | Clipboard save/restore + Ctrl/Cmd+V simulation |
| ✅ STT providers: Groq, OpenAI, OpenRouter, Custom | BYOK, provider-aware routing |
| ✅ LLM refinement: Groq, OpenAI, Anthropic, Custom | Filler word removal, self-correction |
| ✅ Language support: Auto / 中文 / English / 日本語 | Whisper prompt injection per language |
| ✅ Tone presets | Casual, Professional, Email, Note, Social, Custom |
| ✅ Custom vocabulary / personal dictionary | Injected into Whisper prompt |
| ✅ Transcription history | SQLite, search, copy |
| ✅ System tray | Status, quick language switch, menu |
| ✅ Settings UI | Hotkey, STT/LLM config, API keys, theme, i18n |
| ✅ Auto-update | Tauri updater + public releases repo |
| ✅ Freemium licensing | LemonSqueezy + per-category usage quotas |
| ✅ Audio file transcription | Drag-and-drop, WAV chunking |
| ✅ Auto-structured output | List/step detection in Casual prompts → bullet or numbered format |
| ✅ Translation Mode | Speak in one language, output in another. Toggle + target language in Settings |
| ✅ Voice Commands for Formatting | "comma" → `,` · "new line" → `\n` · "new paragraph" → `\n\n` · supports EN/ZH/JA/KO |

---

## P0 — Safety / Reliability

### Recording Time Limit
**Status:** 🔨 Plan: `docs/plans/2026-02-28-recording-time-limit.md`

**Problem:** No upper bound on recording duration. Buffer grows unbounded in memory. If user forgets to release PTT or toggle key, recording runs forever.

**Solution:** Configurable max recording duration (default: 5 minutes). Auto-stop with normal pipeline when limit is reached. Show timeout indicator in overlay.

---

## P1 — Core UX Improvements

### Voice Commands for Formatting
**Status:** ✅ Shipped — Plan: `docs/plans/2026-02-28-voice-commands.md` · Guide: `docs/voice-commands.md`

**What:** While dictating, spoken keywords insert formatting instead of text:
- "新行" / "new line" / "새 줄" / "改行" → `\n`
- "新段落" / "new paragraph" / "새 단락" / "新しい段落" → `\n\n`
- "逗號" / "comma" / "쉼표" → `,`
- "句號" / "period" / "full stop" / "마침표" → `.`
- "問號" / "question mark" / "물음표" / "疑問符" → `?`
- "驚嘆號" / "exclamation mark" / "느낌표" → `!`

**Approach:** Post-STT, regex-free string replacement in `pipeline/voice_commands.rs`. Configurable toggle in Settings → General. Zero new API calls. Works with or without LLM refinement.

---

### Expanded Language Support
**Status:** 📋

**What:** Add more Whisper-supported languages to the language picker:
- Korean (ko)
- Spanish (es)
- French (fr)
- German (de)
- Portuguese (pt)
- Arabic (ar)
- Vietnamese (vi)

**Approach:** Add variants to `Language` enum in `pipeline/state.rs`. Each needs a `code()` and `prompt()`. LLM refinement prompts need adding for each language (or fallback to English prompt).

**Scope:** Medium — enum changes, localization strings, no new APIs.

---

## P2 — Power Features

### Select Text → Voice Edit
**Status:** 📋

**What:** User selects text in any app → presses hotkey → speaks edit command → VoxPen replaces selection with edited version.

Examples:
- Select: "this is a bad sentence" → say "make it more professional" → replaced
- Select: a paragraph → say "summarize in one sentence" → replaced
- Select: English text → say "translate to Chinese" → replaced

**Approach:**
1. On hotkey press: read selected text via clipboard (`Ctrl+C`)
2. Record voice command
3. Pass (selected_text + voice_command) to LLM as a structured prompt
4. Replace clipboard with result + paste

**Complexity:** High — requires detecting if there's a selection, new LLM prompt design, UX for disambiguation (dictate vs edit mode).

**Typeless comparison:** Core feature of Typeless premium tier.

---

### Translation Mode
**Status:** ✅ Shipped — Plan: `docs/plans/2026-02-28-translation-mode.md`

**What:** Speak in one language, output in another. User selects source + target language in settings or via quick toggle.

Examples:
- Speak in Chinese → output in English
- Speak in Japanese → output in Traditional Chinese

**Approach:** Post-STT LLM step with translation prompt. Reuses refinement pipeline with a `for_translation()` prompt variant.

---

## P3 — Personalization

### Personalization Progress Tracking
**Status:** 💡

**What:** Show users how well the app has adapted to their writing style. Metrics: vocabulary size, most-used phrases, average session duration.

**Approach:** Derived from history SQLite database. Display in Settings or a dedicated "Your Profile" tab.

**Complexity:** Low on backend (just aggregate queries), Medium on UI.

---

### Context-Aware Tone by Application
**Status:** 💡

**What:** Automatically apply different tone presets based on the active application. E.g., "Professional" in Outlook, "Casual" in WhatsApp.

**Approach:** Detect active window title/app name via OS APIs. Map app → tone preset in settings.

**Complexity:** High — cross-platform active window detection, complex UX for mapping rules.

**Typeless comparison:** Part of Typeless's adaptive intelligence pitch.

---

## P4 — Offline / Privacy Mode

### On-Device Whisper (Local STT)
**Status:** 🔨 Plan: `docs/plans/2026-02-26-local-whisper-plan.md`

**What:** Transcription runs entirely on-device via whisper.cpp. No API key needed. Works offline.

**Approach:** `whisper-rs` crate (whisper.cpp bindings). Model download + management UI. Feature flag: `local-whisper`.

**Complexity:** High — binary size, model management, platform-specific build.

---

## Notes on Typeless Comparison

| Feature | Typeless | VoxPen |
|---------|---------|--------|
| Voice commands (punctuation, formatting) | ✅ | ✅ Shipped |
| 100+ languages | ✅ | 📋 P1 (expand from 4) |
| Select text + voice edit | ✅ | 📋 P2 |
| Translation | ✅ | ✅ Shipped |
| Context-aware tone by app | ✅ | 💡 P3 |
| Personal dictionary | ✅ | ✅ Shipped |
| Filler word removal | ✅ | ✅ Shipped (via LLM) |
| Auto-structured output (list/steps) | ✅ | ✅ Shipped |
| BYOK / no subscription required | ❌ | ✅ Core differentiator |
| Local/offline STT | ❌ | 🔨 P4 |
| Custom refinement prompts | ❌ | ✅ Shipped |
| Tone presets | ❌ | ✅ Shipped |
| Open API endpoint support | ❌ | ✅ Shipped |

**VoxPen's core differentiator vs Typeless:** BYOK model — no monthly subscription, use your own API keys, support any OpenAI-compatible endpoint (Ollama, local models, etc.).
