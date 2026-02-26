# Windows Robustness Fixes — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix four proactively identified Windows-specific issues that cause thread leaks, mutex panics, and paste failures.

**Architecture:** Surgical fixes to existing modules — no new modules or architectural changes. Each task is independently committable and testable.

**Tech Stack:** Rust (cpal, rdev, arboard, enigo, std::sync::Mutex)

---

## Issue Summary

| # | Issue | Severity | File(s) |
|---|-------|----------|---------|
| 1 | rdev listener thread leaks on hotkey re-registration | HIGH | `src-tauri/src/hotkey.rs` |
| 2 | `Mutex::lock().unwrap()` panics on poison (17 call sites) | MEDIUM | `audio.rs`, `clipboard.rs`, `keyboard.rs`, `history.rs`, `dictionary.rs` |
| 3 | 100ms paste delay insufficient on slow Windows machines | LOW-MED | `voxpen-core/src/input/paste.rs` |
| 4 | No code signing → SmartScreen blocks install | UX | `release.yml` (no fix — requires purchased certificate) |

Issues 1-3 are fixable now. Issue 4 is documented as a known limitation.

---

## Task 1: Fix rdev Thread Leak on Hotkey Re-Registration

### Problem

`rdev::listen()` blocks forever — it runs an OS event loop (`SetWindowsHookEx` on Windows).
Each call to `register_single_key()` spawns a new thread. The `rdev_stop` flag prevents
*processing* events but does NOT terminate the thread. Every hotkey change leaks a thread
with an active Windows hook.

After 10 hotkey changes → 10 orphaned `SetWindowsHookEx` hooks → input lag.

### Solution: Persistent Listener Thread with Shared Target Key

Instead of spawning a new thread per registration, maintain **one permanent rdev thread**
that reads the target key from a shared `Arc`. On re-registration, just swap the key value.

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

**Step 1: Add shared key state to HotkeyManager**

Replace the current `rdev_stop: Arc<AtomicBool>` with a richer shared state:

```rust
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;

/// Shared state between HotkeyManager and the persistent rdev listener thread.
struct RdevSharedState {
    /// The target rdev::Key encoded as u8 index. 0 = disabled.
    target_key_index: AtomicU8,
    /// Whether the listener thread has been spawned.
    thread_spawned: AtomicBool,
}
```

Maintain a lookup table `RDEV_KEY_TABLE: &[(& str, rdev::Key)]` and store the
index into this table in `target_key_index`. When `target_key_index` is 0,
the listener ignores all events (effectively "unregistered").

**Step 2: Modify `register_single_key` to reuse existing thread**

```rust
fn register_single_key(&self, app: &AppHandle, key_name: &str) -> Result<(), String> {
    let index = rdev_key_index(key_name)
        .ok_or_else(|| format!("Unknown key: {}", key_name))?;

    // Update the target key atomically — existing thread picks it up immediately
    self.rdev_shared.target_key_index.store(index, Ordering::SeqCst);

    // Only spawn thread if not already running
    if !self.rdev_shared.thread_spawned.swap(true, Ordering::SeqCst) {
        let shared = Arc::clone(&self.rdev_shared);
        let app_handle = app.clone();
        let processing = Arc::new(AtomicBool::new(false));

        thread::spawn(move || {
            let key_down = Arc::new(AtomicBool::new(false));
            let _ = rdev::listen(move |event| {
                let idx = shared.target_key_index.load(Ordering::SeqCst);
                if idx == 0 { return; } // disabled
                let Some(target) = rdev_key_by_index(idx) else { return; };

                match event.event_type {
                    rdev::EventType::KeyPress(k) if k == target => {
                        if !key_down.swap(true, Ordering::SeqCst) {
                            // ... handle press (same as current code)
                        }
                    }
                    rdev::EventType::KeyRelease(k) if k == target => {
                        if key_down.swap(false, Ordering::SeqCst) {
                            // ... handle release (same as current code)
                        }
                    }
                    _ => {}
                }
            });
        });
    }

    Ok(())
}
```

**Step 3: Modify `unregister` to zero out the key index instead of spawning**

```rust
pub fn unregister(&mut self, app: &AppHandle) {
    if let Some(ref shortcut) = self.current_shortcut {
        if is_combo_shortcut(shortcut) {
            let _ = app.global_shortcut().unregister_all();
        }
        // Zero = disabled. The rdev thread stays alive but does nothing.
        self.rdev_shared.target_key_index.store(0, Ordering::SeqCst);
    }
    self.current_shortcut = None;
}
```

**Step 4: Run build and clippy**

```bash
cargo build --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

**Step 5: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "fix: prevent rdev thread leak on hotkey re-registration

Use a persistent listener thread with shared atomic key index.
Re-registration swaps the target key without spawning a new thread.
Prevents accumulating SetWindowsHookEx hooks on Windows."
```

---

## Task 2: Handle Mutex Poison Instead of Panicking

### Problem

17 call sites use `.lock().unwrap()` on `std::sync::Mutex`. If a thread panics
while holding a lock (e.g., cpal audio callback crashes on WASAPI error), the
Mutex becomes poisoned and **every subsequent `.unwrap()` panics**, crashing the app.

### Solution: Replace `.unwrap()` with poison recovery

Use `.lock().unwrap_or_else(|poisoned| poisoned.into_inner())` to recover the
inner value from a poisoned mutex. This is safe for our use cases:
- **Audio buffer**: worst case, partial samples (recording restarts anyway)
- **Clipboard/Keyboard**: operations return `Result`, errors handled upstream
- **SQLite connections**: rusqlite handles interrupted state gracefully
- **cpal Stream**: dropping a partially-initialized stream is safe

**Files:**
- Modify: `src-tauri/src/audio.rs` (5 sites)
- Modify: `src-tauri/src/clipboard.rs` (2 sites)
- Modify: `src-tauri/src/keyboard.rs` (1 site)
- Modify: `src-tauri/src/history.rs` (4 sites)
- Modify: `src-tauri/src/dictionary.rs` (5 sites, production code only — tests keep `.unwrap()`)

**Step 1: Create a helper extension trait (optional, for DRY)**

Add to `src-tauri/src/lib.rs` or a new `src-tauri/src/util.rs`:

```rust
/// Extension trait to recover from poisoned mutexes.
/// Preferred over `.unwrap()` because a panic in one thread
/// (e.g., audio callback) should not crash the entire app.
trait MutexExt<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T>;
}

impl<T> MutexExt<T> for std::sync::Mutex<T> {
    fn lock_or_recover(&self) -> std::sync::MutexGuard<'_, T> {
        self.lock().unwrap_or_else(|poisoned| {
            eprintln!("recovered from poisoned mutex");
            poisoned.into_inner()
        })
    }
}
```

**Alternative (simpler):** Just do inline `.unwrap_or_else(|e| e.into_inner())` at each site.
No new trait needed. More verbose but zero abstraction cost.

**Decision for implementer:** Choose one approach. The trait is DRY-er; inline is simpler.

**Step 2: Replace all production `.lock().unwrap()` calls**

Example for `audio.rs`:
```rust
// Before:
buffer.lock().unwrap().clear();
// After:
buffer.lock().unwrap_or_else(|e| e.into_inner()).clear();
```

**Step 3: Run tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
```

**Step 4: Commit**

```bash
git add src-tauri/src/audio.rs src-tauri/src/clipboard.rs src-tauri/src/keyboard.rs \
        src-tauri/src/history.rs src-tauri/src/dictionary.rs
git commit -m "fix: recover from poisoned mutexes instead of panicking

Replace .lock().unwrap() with poison recovery at 17 call sites.
Prevents cascading app crashes when a thread panics while holding
a lock (e.g., WASAPI audio callback failure on Windows)."
```

---

## Task 3: Platform-Aware Paste Delay

### Problem

`PASTE_DELAY = 100ms` (hardcoded). After simulating Ctrl+V, the code waits 100ms
then restores the original clipboard. On Windows with active antivirus scanning
the clipboard, 100ms may not be enough — the paste hasn't completed before
the clipboard is restored, causing the **original** clipboard content to be pasted.

### Solution: Increase delay with platform-specific value

```rust
/// Delay between clipboard write and paste simulation.
/// Windows needs more time due to clipboard chain processing and
/// potential antivirus scanning. macOS/Linux are faster.
#[cfg(target_os = "windows")]
const PASTE_DELAY: Duration = Duration::from_millis(200);

#[cfg(not(target_os = "windows"))]
const PASTE_DELAY: Duration = Duration::from_millis(100);
```

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/input/paste.rs:17`

**Step 1: Replace the constant**

Replace the single `PASTE_DELAY` with the platform-conditional version above.

**Step 2: Run tests**

```bash
cargo test -p voxpen-core --manifest-path src-tauri/Cargo.toml
```

**Step 3: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/input/paste.rs
git commit -m "fix: increase paste delay on Windows for clipboard reliability

Windows clipboard chain processing and antivirus scanning can delay
paste completion. 100ms is insufficient; increase to 200ms on Windows."
```

---

## Known Limitations (No Code Fix)

### Windows SmartScreen Warning (Issue 4)

**Problem:** No code signing certificate → SmartScreen shows "Windows protected your PC"
on first install for every user.

**Impact:** Major UX friction for non-technical users.

**Fix required:** Purchase an EV code signing certificate (~$200-400/year) and configure
in `release.yml`:
```yaml
env:
  TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
```

**Status:** Deferred until user base justifies cost.

### arboard Clipboard Lifetime (Issue from analysis)

**Problem:** Single `Clipboard` instance lives for app lifetime. On Windows, long-lived
clipboard handles can miss clipboard chain updates from other apps.

**Mitigation:** Current implementation is acceptable. If users report clipboard issues,
consider creating a new `Clipboard` instance per paste operation (tradeoff: ~1ms overhead
per operation).

---

## Execution Order

Tasks are independent and can be executed in any order.
Recommended: Task 1 (highest impact) → Task 2 → Task 3.

Total estimated diff: ~60 lines changed across 8 files.
