# Voice Commands

Voice Commands let you insert punctuation and formatting symbols by speaking keyword phrases while you dictate. Instead of pausing to type a comma or pressing enter, just say the word.

---

## Enabling Voice Commands

1. Open **Settings** (tray icon → Settings, or the hotkey)
2. Go to the **General** tab
3. Toggle **Voice Commands** on

That's it. The feature takes effect immediately — no restart needed.

---

## Supported Keywords

Speak any of these keywords and they will be replaced with the corresponding character in the pasted output.

### English

| Say this | You get |
|----------|---------|
| `comma` | `,` |
| `period` / `full stop` | `.` |
| `question mark` | `?` |
| `exclamation mark` / `exclamation point` | `!` |
| `new line` | line break |
| `new paragraph` | blank line (double line break) |

Keywords are **case-insensitive** — "Comma", "COMMA", and "comma" all work.

### 繁體中文

| 說這個 | 輸出 |
|--------|------|
| `逗號` | `,` |
| `句號` | `.` |
| `問號` | `?` |
| `驚嘆號` | `!` |
| `新行` | 換行 |
| `新段落` | 空行（雙換行） |

### 日本語

| これを言う | 出力 |
|-----------|------|
| `改行` | 改行 |
| `新しい段落` | 空行（二重改行） |

### 한국어

| 말하기 | 출력 |
|--------|------|
| `쉼표` | `,` |
| `마침표` | `.` |
| `물음표` | `?` |
| `느낌표` | `!` |
| `새 줄` | 줄 바꿈 |
| `새 단락` | 빈 줄 (이중 줄 바꿈) |

---

## Usage Examples

**Dictate:** "Please send me the report comma the one from last week period"
**Output:** `Please send me the report, the one from last week.`

---

**Dictate:** "Heading new paragraph First item comma second item period new paragraph Footer"
**Output:**
```
Heading

First item, second item.

Footer
```

---

**繁體中文示範：**
**口述：** 「你好逗號我是小明句號」
**輸出：** `你好，我是小明。`

---

## How It Works

Voice commands run **after** the speech-to-text (STT) step but **before** LLM refinement. This means:

- The STT engine transcribes your speech literally (e.g. "hello comma world")
- Voice Commands replaces the keywords (e.g. `hello, world`)
- If LLM refinement is on, the refined text gets the punctuation already in place

You do not need LLM refinement enabled for voice commands to work. They are a standalone post-processing step.

---

## Tips

- **"new paragraph" takes priority over "new line"** — saying "new paragraph" will never be misread as "new line" + the word "paragraph".
- **Spaces around keywords are absorbed** — "hello comma world" becomes `hello, world` (not `hello , world`).
- **Mixed-language dictation works** — You can say "first item 逗號 second item" and get `first item, second item`.
- **Disable when not needed** — If you are dictating prose and don't want accidental substitutions (e.g. if you actually want to say the word "comma"), turn the toggle off.
