# LemonSqueezy Licensing Design

Date: 2026-02-26
Status: Approved

## Summary

VoxInk Desktop transitions from free open-source to freemium with LemonSqueezy license key validation. BYOK model stays for both tiers. Architecture: pure local (App direct to LemonSqueezy API), with future migration path to proxy model.

## Business Model

| Item | Decision |
|------|----------|
| Pricing | One-time purchase + major version upgrade (Sublime/Sketch model) |
| Devices | 1 key = 3 activations |
| API Keys | BYOK both tiers (user pays own API costs) |
| Source | Closed source (repo goes private) |

## Free vs Pro

| Feature | Free | Pro |
|---------|------|-----|
| Voice input (hotkey record/transcribe/paste) | 15/day | Unlimited |
| LLM refinement | Shared in 15/day quota | Unlimited |
| Audio file transcription | No | Yes |
| Custom vocabulary | Yes | Yes |
| History | Yes | Yes |
| Custom prompts | Yes | Yes |
| All 11 languages | Yes | Yes |

- Count method: successful transcription (STT returns text) = 1 use
- Reset: local timezone midnight
- UX: countdown warning at 3 remaining, hard block at 0 (no recording)

## Architecture: Approach A (Pure Local)

```
User enters key --> App calls LemonSqueezy /v1/licenses/activate
                --> Success: encrypt & store locally
                --> Every 7 days: App calls /v1/licenses/validate
                --> Usage counting in local SQLite
```

### Why Approach A

1. One-time purchase + BYOK = minimal business logic
2. Zero ops cost (no server to maintain)
3. Refund abuse: max 14 days delay (7-day verify cycle + 7-day grace), acceptable for one-time purchase
4. Clean upgrade path to Approach C when needed

### Future: Approach C (Proxy + Webhook)

```
App --> Your CF Worker --> LemonSqueezy (verify)
LemonSqueezy --> Webhook --> CF Worker (revocation events)
```

Migration requires only a new `LicenseVerifier` impl. `LicenseManager` stays unchanged.

## Data Model

### Rust Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LicenseTier {
    Free,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub tier: LicenseTier,
    pub license_key: String,
    pub instance_id: String,
    pub licensed_version: u32,         // major version bound (e.g., 1)
    pub activated_at: i64,
    pub last_verified_at: i64,
    pub verification_grace_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub date: String,                  // "2026-02-26" (local timezone)
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UsageStatus {
    Available { remaining: u32 },
    Warning { remaining: u32 },        // <= 3 remaining
    Exhausted,
    Unlimited,                         // Pro user
}
```

### Storage

| Data | Store | Reason |
|------|-------|--------|
| `license_key` + `instance_id` | `secrets.json` (encrypted) | Sensitive, same pattern as API keys |
| `LicenseInfo` metadata | `secrets.json` (encrypted) | Prevent tampering of timestamps |
| `UsageRecord` | SQLite `daily_usage` table | Queryable, auto-cleanup |

### SQLite Schema

```sql
CREATE TABLE IF NOT EXISTS daily_usage (
    date TEXT PRIMARY KEY NOT NULL,
    count INTEGER NOT NULL DEFAULT 0
);
```

## LemonSqueezy API Integration

### Endpoints Used

| Action | Endpoint | When |
|--------|----------|------|
| Activate | `POST /v1/licenses/activate` | User enters key |
| Validate | `POST /v1/licenses/validate` | Silent re-verify every 7 days |
| Deactivate | `POST /v1/licenses/deactivate` | User logs out / switches device |

### Rust Module

```rust
pub struct LemonSqueezyClient {
    http: reqwest::Client,
}

impl LemonSqueezyClient {
    pub async fn activate(&self, key: &str, instance_name: &str)
        -> Result<ActivationResponse, AppError>;
    pub async fn validate(&self, key: &str, instance_id: &str)
        -> Result<ValidationResponse, AppError>;
    pub async fn deactivate(&self, key: &str, instance_id: &str)
        -> Result<(), AppError>;
}
```

`instance_name` = hostname or machine-id (same machine reinstall doesn't consume a slot).

## Verification State Machine

```
App startup
  |
  +-- No license stored --> Free mode
  |
  +-- License stored
       |
       +-- last_verified < 7 days ago --> Use local token, Pro mode
       |
       +-- last_verified >= 7 days ago --> Silent verify
            |
            +-- Online + valid --> Update last_verified_at, Pro mode
            |
            +-- Online + invalid (revoked/refunded)
            |    |
            |    +-- First failure --> Set grace_until = now + 7 days, notify, keep Pro
            |    +-- Past grace --> Downgrade to Free, clear license
            |
            +-- Offline
                 |
                 +-- last_verified < 30 days ago --> Keep Pro (offline grace)
                 +-- last_verified >= 30 days ago --> Downgrade to Free, prompt to reconnect
```

Verification triggers (no background timer):
1. App startup — check `last_verified_at`
2. Before each recording — `LicenseManager.check_access()` piggybacks verify check

## LicenseManager

```rust
pub struct LicenseManager<V: LicenseVerifier> {
    verifier: V,
    store: LicenseStore,
    db: UsageDb,
    app_major_version: u32,
}

impl<V: LicenseVerifier> LicenseManager<V> {
    pub async fn check_access(&self) -> UsageStatus;
    pub async fn record_usage(&self) -> Result<UsageStatus, AppError>;
    pub async fn activate(&self, key: &str) -> Result<LicenseInfo, AppError>;
    pub async fn deactivate(&self) -> Result<(), AppError>;
    pub async fn verify_if_needed(&self) -> Result<LicenseTier, AppError>;
    pub fn current_tier(&self) -> LicenseTier;
    pub fn license_info(&self) -> Option<&LicenseInfo>;
}
```

### LicenseVerifier Trait (future-proof)

```rust
#[cfg_attr(test, automock)]
pub trait LicenseVerifier: Send + Sync {
    async fn activate(&self, key: &str, instance: &str)
        -> Result<ActivationResponse, AppError>;
    async fn validate(&self, key: &str, instance: &str)
        -> Result<ValidationResponse, AppError>;
    async fn deactivate(&self, key: &str, instance: &str)
        -> Result<(), AppError>;
}

// v1: Direct LemonSqueezy
pub struct DirectLemonSqueezy { ... }
impl LicenseVerifier for DirectLemonSqueezy { ... }

// Future v2: Via proxy
// pub struct ProxyVerifier { ... }
// impl LicenseVerifier for ProxyVerifier { ... }
```

## Pipeline Integration

### Gate Points (2 changes in hotkey.rs)

**Before recording:**
```rust
async fn on_start_recording(state: &AppState) {
    let usage = state.license_manager.check_access().await;
    match usage {
        UsageStatus::Exhausted => {
            app.emit("usage-exhausted", ());
            return;  // Don't record
        }
        UsageStatus::Warning { remaining } => {
            app.emit("usage-warning", remaining);
            // Continue recording
        }
        _ => {}
    }
    // ... existing recording logic
}
```

**After successful transcription:**
```rust
async fn on_transcription_success(state: &AppState, text: &str) {
    if let Ok(status) = state.license_manager.record_usage().await {
        app.emit("usage-updated", status);
    }
    // ... existing paste logic
}
```

### AppState Extension

```rust
pub struct AppState {
    // ... existing fields ...
    pub license_manager: Arc<LicenseManager<DirectLemonSqueezy>>,
}
```

### New Tauri IPC Commands (5)

```rust
#[tauri::command] async fn activate_license(key: String) -> Result<LicenseInfo, String>;
#[tauri::command] async fn deactivate_license() -> Result<(), String>;
#[tauri::command] async fn get_license_info() -> Result<Option<LicenseInfo>, String>;
#[tauri::command] async fn get_usage_status() -> Result<UsageStatus, String>;
#[tauri::command] async fn get_license_tier() -> Result<LicenseTier, String>;
```

### Events (Rust to React)

```rust
app.emit("usage-exhausted", ());
app.emit("usage-warning", remaining: u32);
app.emit("usage-updated", UsageStatus);
app.emit("license-changed", LicenseTier);
app.emit("license-verification-failed", message);
```

## Frontend UI

### Settings Window: License Tab (first tab)

**Free state:** usage progress bar (N/15), key input field, activate button, purchase link.

**Pro state:** masked key display, version validity, device count, last verified date, deactivate button.

### Overlay Changes

| State | Trigger | Display | Auto-dismiss |
|-------|---------|---------|-------------|
| `UsageWarning` | `usage-warning` event | "N left today" text on Recording overlay | With Recording |
| `UsageExhausted` | `usage-exhausted` event | "Daily limit reached" + "Upgrade" button (clickable) | 5 seconds |

### System Tray Addition

```
───────────────
Free: 12/15 today    (or "Pro" for Pro users)
Upgrade to Pro...    (Free only, opens browser)
───────────────
```

### i18n: ~15 new keys per locale

Keys: `license_tab`, `license_free`, `license_pro`, `license_usage_today`, `license_enter_key`, `license_activate`, `license_deactivate`, `license_purchase`, `license_valid_for`, `license_devices`, `license_last_verified`, `license_exhausted`, `license_upgrade_prompt`, `license_warning_remaining`, `license_verification_failed`.

## File Changes

### New Files (Rust)

```
src-tauri/crates/voxink-core/src/licensing/
  mod.rs
  types.rs
  manager.rs
  lemonsqueezy.rs
  store.rs
  usage.rs
```

### New Files (React)

```
src/components/Settings/LicenseSection.tsx
```

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/crates/voxink-core/src/lib.rs` | Add `pub mod licensing` |
| `src-tauri/crates/voxink-core/src/error.rs` | Add `License`, `UsageLimitReached` variants |
| `src-tauri/src/state.rs` | Add `license_manager` to `AppState` |
| `src-tauri/src/hotkey.rs` | Add 2 gate points (before record, after transcription) |
| `src-tauri/src/commands.rs` | Add 5 IPC commands |
| `src-tauri/src/lib.rs` | Register commands, init LicenseManager, tray menu items |
| `src/components/Settings/SettingsWindow.tsx` | Add License tab |
| `src/components/Overlay.tsx` | Add UsageWarning / Exhausted states |
| `src/lib/tauri.ts` | Add 5 invoke wrappers |
| `src/types/settings.ts` | Add LicenseInfo, UsageStatus, LicenseTier types |
| `src/locales/en.json` | Add ~15 license keys |
| `src/locales/zh-TW.json` | Add ~15 license keys |

## Security

- License key stored encrypted (`secrets.json`), never in plaintext
- Key masked in Debug output (first 4 + `****` + last 4)
- Key never enters React webview — frontend receives masked version only
- LemonSqueezy API calls over HTTPS (reqwest + rustls)
- `daily_usage` SQLite theoretically tamperable, but no economic incentive under BYOK
- Stronger protection available via Approach C migration

## Testing

### Unit Tests (~16 cases)

| Test | Verifies |
|------|----------|
| `check_access_free_under_limit` | Available with correct remaining |
| `check_access_free_warning` | Warning at 13/15 used |
| `check_access_free_exhausted` | Exhausted at 15/15 |
| `check_access_pro` | Unlimited |
| `record_usage_increments` | Count +1 after call |
| `record_usage_crosses_midnight` | Count resets on new date |
| `verify_skips_if_recent` | No API call if < 7 days |
| `verify_triggers_if_stale` | API call if >= 7 days |
| `verify_failure_sets_grace` | grace_until = now + 7 days |
| `verify_failure_past_grace_downgrades` | Downgrade to Free |
| `offline_within_30_days` | Keep Pro |
| `offline_past_30_days` | Downgrade to Free |
| `activate_success` | LicenseInfo stored, tier = Pro |
| `activate_invalid_key` | Returns error |
| `activate_device_limit` | Returns error at 3 devices |
| `version_mismatch_downgrades` | v1 key + v2 app = Free |

### HTTP Mocking

`wiremock` for LemonSqueezy API responses. `LicenseVerifier` trait enables `MockLicenseVerifier` via `mockall`.

### LemonSqueezy Test Mode

Development and CI use LemonSqueezy test mode (sandbox keys, same API endpoints, no real charges).

## Roadmap

| Phase | Content | Trigger |
|-------|---------|---------|
| **v1.0** | Approach A — local licensing + direct LemonSqueezy | This design |
| **v1.x** | Audio file transcription UI (Pro only) | Pro user demand |
| **v2.0** | Migrate to Approach C — add CF Worker webhook | Refund abuse becomes a problem |
| **v2.0** | Major version upgrade pricing (v1 keys invalid for v2) | Feature mass justifies new version |
| **v2.x** | User dashboard — self-service device management | User growth |
