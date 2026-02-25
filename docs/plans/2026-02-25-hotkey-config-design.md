# Hotkey Configuration + i18n Design

**Date**: 2026-02-25
**Version target**: v0.3.0

## Problem

1. Global hotkey is hardcoded to `CommandOrControl+Shift+V` — users cannot change it
2. Typeless-style single-key activation (Right Alt / fn) is the gold standard UX for voice-to-text
3. i18n infrastructure exists but hotkey config UI strings are placeholder

## Design

### Hotkey Configuration

**Approach**: Hybrid hotkey registration

- **Combo keys** (Ctrl+Shift+V, Cmd+Shift+Space, etc.): Use existing `tauri-plugin-global-shortcut`
- **Single keys** (Right Alt, F13, etc.): Use `rdev` crate for low-level OS keyboard hooks

#### Backend Changes

1. **`hotkey.rs`** — Refactor to support dynamic re-registration:
   - `register_hotkey(app, shortcut_str)` — parse shortcut string, decide rdev vs global-shortcut
   - `unregister_hotkey(app)` — clean up current registration
   - For rdev single-key mode: spawn background thread with `rdev::listen()`, detect press/release
   - For combo mode: use existing `global_shortcut().on_shortcut()` pattern

2. **`commands.rs`** — New `set_hotkey` command:
   - Validates shortcut string format
   - Calls unregister + register
   - Persists to settings via store
   - Returns error if registration fails (key already taken by OS, etc.)

3. **Cargo.toml** — Add `rdev` dependency

#### Frontend Changes

1. **`GeneralSection.tsx`** — Hotkey recorder widget:
   - Click "Change" button → enters recording mode
   - Press desired key/combo → displays it
   - "Save" confirms, "Cancel" reverts
   - Preset buttons: "Right Alt (Recommended)", "Ctrl+Shift+V"
   - Visual feedback: pulsing border during recording

2. **`settings.ts`** — No type changes needed (hotkey is already `string`)

#### Hotkey String Format

- Combo keys: `"CommandOrControl+Shift+V"` (Tauri format)
- Single keys: `"RAlt"`, `"F13"`, `"RControl"` (rdev Key enum names, prefixed for disambiguation)
- Detection: if string contains `+`, use global-shortcut; otherwise use rdev

### i18n Updates

Add new translation keys for hotkey configuration UI to both en.json and zh-TW.json:
- `hotkeyChange`, `hotkeyRecording`, `hotkeySave`, `hotkeyCancel`
- `hotkeyPresetAlt`, `hotkeyPresetCombo`
- `hotkeyError`, `hotkeyConflict`
- Updated `hotkeyHint` with actual usage instructions

## Testing

- Unit tests for hotkey string parsing (combo vs single key detection)
- Integration test for register/unregister cycle
- Frontend: manual testing of recorder widget

## Risks

- `rdev` on Linux requires X11 (no Wayland) — acceptable since current builds are Windows-only
- Some antivirus may flag low-level keyboard hooks — acceptable tradeoff for UX
