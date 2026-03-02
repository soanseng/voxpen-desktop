# Promo Code License Expiration — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add `expires_at` support to the licensing system so promo code licenses expire after 1 month, with a one-time overlay notification encouraging Pro purchase.

**Architecture:** Parse `expires_at` from LemonSqueezy API response, store in `LicenseInfo`, check locally on every `current_tier()` call for instant expiration. Emit a `"promo-expired"` event on first detection so the Overlay shows a one-time upgrade prompt.

**Tech Stack:** Rust (chrono for ISO 8601 parsing), React/TypeScript, Tauri events, i18next

---

### Task 1: Add `expires_at` to `LsLicenseKey` response type

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/licensing/lemonsqueezy.rs:161-168`

**Step 1: Write the failing test**

Add to the existing `mod tests` block in `lemonsqueezy.rs` (after the last test at ~line 389):

```rust
#[test]
fn should_deserialize_expires_at_from_license_key() {
    let json = r#"{"activated":true,"error":null,"license_key":{"id":1,"status":"active","key":"K","activation_limit":3,"activation_usage":1,"expires_at":"2026-04-02T00:00:00.000000Z"},"instance":{"id":"i","name":"n"},"meta":{"store_id":1,"product_id":2,"variant_id":3}}"#;
    let resp: LsLicenseResponse = serde_json::from_str(json).unwrap();
    let key = resp.license_key.unwrap();
    assert_eq!(key.expires_at.as_deref(), Some("2026-04-02T00:00:00.000000Z"));
}

#[test]
fn should_deserialize_null_expires_at_as_none() {
    let json = r#"{"activated":true,"error":null,"license_key":{"id":1,"status":"active","key":"K","activation_limit":3,"activation_usage":1,"expires_at":null},"instance":{"id":"i","name":"n"},"meta":{"store_id":1,"product_id":2,"variant_id":3}}"#;
    let resp: LsLicenseResponse = serde_json::from_str(json).unwrap();
    let key = resp.license_key.unwrap();
    assert!(key.expires_at.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing::lemonsqueezy::tests::should_deserialize_expires_at`
Expected: FAIL — `LsLicenseKey` has no `expires_at` field

**Step 3: Write minimal implementation**

In `lemonsqueezy.rs`, add `expires_at` to `LsLicenseKey` (line 162-168):

```rust
/// License key details from LemonSqueezy.
#[derive(Debug, Clone, Deserialize)]
pub struct LsLicenseKey {
    pub id: Option<u64>,
    pub status: Option<String>,
    pub key: Option<String>,
    pub activation_limit: Option<u32>,
    pub activation_usage: Option<u32>,
    /// ISO 8601 expiration timestamp. `None` means perpetual.
    pub expires_at: Option<String>,
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing::lemonsqueezy::tests`
Expected: ALL PASS (existing tests still pass because `expires_at` is `Option` and serde skips missing fields)

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/licensing/lemonsqueezy.rs
git commit -m "feat(licensing): add expires_at field to LsLicenseKey"
```

---

### Task 2: Add `expires_at` to `LicenseInfo` type

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/licensing/types.rs:31-45`

**Step 1: Write the failing test**

Add to `mod tests` in `types.rs`:

```rust
#[test]
fn should_roundtrip_license_info_with_expires_at() {
    let info = LicenseInfo {
        tier: LicenseTier::Pro,
        license_key: "key".to_string(),
        instance_id: "inst".to_string(),
        licensed_version: 1,
        activated_at: 1700000000,
        last_verified_at: 1700100000,
        verification_grace_until: None,
        expires_at: Some(1712016000), // 2026-04-02
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: LicenseInfo = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.expires_at, Some(1712016000));
}

#[test]
fn should_roundtrip_license_info_without_expires_at() {
    let info = LicenseInfo {
        tier: LicenseTier::Pro,
        license_key: "key".to_string(),
        instance_id: "inst".to_string(),
        licensed_version: 1,
        activated_at: 1700000000,
        last_verified_at: 1700100000,
        verification_grace_until: None,
        expires_at: None,
    };
    let json = serde_json::to_string(&info).unwrap();
    let deserialized: LicenseInfo = serde_json::from_str(&json).unwrap();
    assert!(deserialized.expires_at.is_none());
}

#[test]
fn should_deserialize_old_license_info_without_expires_at_field() {
    // Simulates loading a LicenseInfo saved before expires_at was added
    let json = r#"{"tier":"Pro","license_key":"k","instance_id":"i","licensed_version":1,"activated_at":1700000000,"last_verified_at":1700100000,"verification_grace_until":null}"#;
    let info: LicenseInfo = serde_json::from_str(json).unwrap();
    assert!(info.expires_at.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing::types::tests::should_roundtrip_license_info_with_expires`
Expected: FAIL — `LicenseInfo` has no `expires_at` field

**Step 3: Write minimal implementation**

In `types.rs`, add `expires_at` to `LicenseInfo` (lines 31-45). Use `#[serde(default)]` for backward compatibility with stored data that lacks the field:

```rust
/// Persisted license information for an activated Pro license.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub tier: LicenseTier,
    pub license_key: String,
    pub instance_id: String,
    /// Major version this license was activated for (version-bound licensing).
    pub licensed_version: u32,
    /// Unix timestamp (seconds) when the license was activated.
    pub activated_at: i64,
    /// Unix timestamp (seconds) of the last successful verification.
    pub last_verified_at: i64,
    /// If set, the grace period deadline (Unix seconds) after a failed verification.
    /// License stays Pro until this deadline passes.
    pub verification_grace_until: Option<i64>,
    /// Unix timestamp (seconds) when this license expires. `None` = perpetual.
    #[serde(default)]
    pub expires_at: Option<i64>,
}
```

**Step 4: Fix all existing test code that constructs `LicenseInfo`**

Every existing `LicenseInfo { ... }` literal in `types.rs` tests and `manager.rs` tests must add `expires_at: None`. Search for `LicenseInfo {` and add the field:

In `types.rs` tests — update `should_roundtrip_license_info` and `should_roundtrip_license_info_with_grace`:
```rust
// Add to each LicenseInfo literal:
expires_at: None,
```

In `manager.rs` tests — update helper functions `pro_license()` and `pro_license_with_grace()`:
```rust
fn pro_license(last_verified_at: i64) -> LicenseInfo {
    LicenseInfo {
        tier: LicenseTier::Pro,
        license_key: "KEY-123".to_string(),
        instance_id: "inst-001".to_string(),
        licensed_version: CURRENT_MAJOR_VERSION,
        activated_at: 1700000000,
        last_verified_at,
        verification_grace_until: None,
        expires_at: None,
    }
}
```

In `manager.rs` `activate()` method — add `expires_at` to the `LicenseInfo` construction (line 139-147):
```rust
let info = LicenseInfo {
    tier: LicenseTier::Pro,
    license_key: key.to_string(),
    instance_id,
    licensed_version: CURRENT_MAJOR_VERSION,
    activated_at: now,
    last_verified_at: now,
    verification_grace_until: None,
    expires_at: None, // Will be filled in Task 4
};
```

**Step 5: Run all tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing`
Expected: ALL PASS

**Step 6: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/licensing/types.rs src-tauri/crates/voxpen-core/src/licensing/manager.rs
git commit -m "feat(licensing): add expires_at field to LicenseInfo"
```

---

### Task 3: Add expiration check to `current_tier()`

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/licensing/manager.rs:58-63`

**Step 1: Write the failing tests**

Add to `mod tests` in `manager.rs`:

```rust
#[test]
fn current_tier_should_be_free_when_license_expired() {
    let now = chrono::Utc::now().timestamp();
    let mut license = pro_license(now);
    license.expires_at = Some(now - 3600); // expired 1 hour ago
    let mgr = LicenseManager::new(
        MockVerifier::new(vec![]),
        MockStore::new(Some(license)),
        MockUsageDb::uniform(0),
    );
    assert_eq!(mgr.current_tier(), LicenseTier::Free);
}

#[test]
fn current_tier_should_be_pro_when_license_not_expired() {
    let now = chrono::Utc::now().timestamp();
    let mut license = pro_license(now);
    license.expires_at = Some(now + 86400 * 30); // expires in 30 days
    let mgr = LicenseManager::new(
        MockVerifier::new(vec![]),
        MockStore::new(Some(license)),
        MockUsageDb::uniform(0),
    );
    assert_eq!(mgr.current_tier(), LicenseTier::Pro);
}

#[test]
fn current_tier_should_be_pro_when_perpetual_license() {
    let now = chrono::Utc::now().timestamp();
    let mut license = pro_license(now);
    license.expires_at = None; // perpetual
    let mgr = LicenseManager::new(
        MockVerifier::new(vec![]),
        MockStore::new(Some(license)),
        MockUsageDb::uniform(0),
    );
    assert_eq!(mgr.current_tier(), LicenseTier::Pro);
}
```

**Step 2: Run test to verify the first one fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing::manager::tests::current_tier_should_be_free_when_license_expired`
Expected: FAIL — returns `Pro` instead of `Free`

**Step 3: Write minimal implementation**

Replace `current_tier()` in `manager.rs` (lines 58-63):

```rust
pub fn current_tier(&self) -> LicenseTier {
    match self.store.load() {
        Some(info) if info.licensed_version == CURRENT_MAJOR_VERSION => {
            if let Some(exp) = info.expires_at {
                if chrono::Utc::now().timestamp() >= exp {
                    return LicenseTier::Free;
                }
            }
            info.tier
        }
        _ => LicenseTier::Free,
    }
}
```

**Step 4: Run all licensing tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/licensing/manager.rs
git commit -m "feat(licensing): check expires_at in current_tier()"
```

---

### Task 4: Parse `expires_at` in `activate()` and add helper

**Files:**
- Modify: `src-tauri/crates/voxpen-core/src/licensing/manager.rs:127-151`

**Step 1: Write the failing test**

Add to `mod tests` in `manager.rs`. We need to make `MockVerifier` return an `expires_at` value. First update `ok_license_response` to accept an optional `expires_at`:

```rust
fn ok_license_response_with_expiry(instance_id: &str, expires_at: Option<&str>) -> LsLicenseResponse {
    use crate::licensing::lemonsqueezy::{LsInstance, LsLicenseKey};
    LsLicenseResponse {
        valid: true,
        error: None,
        license_key: Some(LsLicenseKey {
            id: Some(1),
            status: Some("active".to_string()),
            key: Some("KEY".to_string()),
            activation_limit: Some(3),
            activation_usage: Some(1),
            expires_at: expires_at.map(|s| s.to_string()),
        }),
        instance: Some(LsInstance {
            id: Some(instance_id.to_string()),
            name: Some("Test".to_string()),
        }),
        meta: None,
    }
}
```

Add a new `VerifyBehavior` variant and update `MockVerifier`:

```rust
// In VerifyBehavior enum, add:
ActivateOkWithExpiry(String), // ISO 8601 expires_at
```

Update `MockVerifier::activate` match:
```rust
VerifyBehavior::ActivateOkWithExpiry(exp) => {
    Ok(ok_license_response_with_expiry("inst-new", Some(&exp)))
}
```

Now add the test:

```rust
#[tokio::test]
async fn activate_should_parse_and_store_expires_at() {
    let mgr = LicenseManager::new(
        MockVerifier::new(vec![VerifyBehavior::ActivateOkWithExpiry(
            "2026-04-02T00:00:00.000000Z".to_string(),
        )]),
        MockStore::new(None),
        MockUsageDb::uniform(0),
    );

    let info = mgr.activate("KEY-PROMO").await.unwrap();
    assert!(info.expires_at.is_some());
    // 2026-04-02T00:00:00Z = 1775088000 (approx)
    let exp = info.expires_at.unwrap();
    assert!(exp > 1700000000, "expires_at should be a valid timestamp");
}

#[tokio::test]
async fn activate_should_store_none_for_perpetual_license() {
    let mgr = LicenseManager::new(
        MockVerifier::new(vec![VerifyBehavior::ActivateOk]),
        MockStore::new(None),
        MockUsageDb::uniform(0),
    );

    let info = mgr.activate("KEY-PERP").await.unwrap();
    assert!(info.expires_at.is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing::manager::tests::activate_should_parse_and_store_expires_at`
Expected: FAIL — `expires_at` is always `None`

**Step 3: Write minimal implementation**

Add a helper function in `manager.rs` (before `hostname_or_default`):

```rust
/// Parse an ISO 8601 timestamp string to a Unix timestamp (seconds).
fn parse_iso8601_to_unix(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp())
}
```

Update `activate()` to parse and store `expires_at`:

```rust
pub async fn activate(&self, key: &str) -> Result<LicenseInfo, AppError> {
    let hostname = hostname_or_default();
    let response = self.verifier.activate(key, &hostname).await?;

    let instance_id = response
        .instance
        .as_ref()
        .and_then(|i| i.id.clone())
        .ok_or_else(|| AppError::License("no instance ID in response".to_string()))?;

    let expires_at = response
        .license_key
        .as_ref()
        .and_then(|lk| lk.expires_at.as_deref())
        .and_then(parse_iso8601_to_unix);

    let now = chrono::Utc::now().timestamp();
    let info = LicenseInfo {
        tier: LicenseTier::Pro,
        license_key: key.to_string(),
        instance_id,
        licensed_version: CURRENT_MAJOR_VERSION,
        activated_at: now,
        last_verified_at: now,
        verification_grace_until: None,
        expires_at,
    };

    self.store.save(&info)?;
    Ok(info)
}
```

**Step 4: Run all licensing tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml -p voxpen-core -- licensing`
Expected: ALL PASS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxpen-core/src/licensing/manager.rs
git commit -m "feat(licensing): parse expires_at from API in activate()"
```

---

### Task 5: Add `expires_at` to frontend TypeScript type

**Files:**
- Modify: `src/types/settings.ts:34-42`

**Step 1: Update `LicenseInfo` interface**

```typescript
export interface LicenseInfo {
  tier: LicenseTier;
  license_key: string;
  instance_id: string;
  licensed_version: number;
  activated_at: number;
  last_verified_at: number;
  verification_grace_until: number | null;
  expires_at: number | null;
}
```

**Step 2: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: PASS (no component uses `expires_at` yet, so no breakage)

**Step 3: Commit**

```bash
git add src/types/settings.ts
git commit -m "feat(licensing): add expires_at to frontend LicenseInfo type"
```

---

### Task 6: Add i18n strings for promo expiration

**Files:**
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add keys to `en.json`**

In the `"license"` object, after `"freeFooter"`, add:

```json
"promoExpiredTitle": "Trial ended",
"promoExpiredMessage": "Upgrade to Pro for unlimited voice input",
"promoExpiredUpgrade": "Upgrade to Pro",
"promoExpiredDismiss": "Later",
"expiresAt": "Expires: {{date}}"
```

**Step 2: Add keys to `zh-TW.json`**

In the `"license"` object, after `"freeFooter"`, add:

```json
"promoExpiredTitle": "試用期已結束",
"promoExpiredMessage": "升級 Pro 享受無限語音輸入",
"promoExpiredUpgrade": "升級 Pro",
"promoExpiredDismiss": "稍後",
"expiresAt": "到期日：{{date}}"
```

**Step 3: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: PASS

**Step 4: Commit**

```bash
git add src/locales/en.json src/locales/zh-TW.json
git commit -m "feat(licensing): add promo expiration i18n strings"
```

---

### Task 7: Show expiry date in `LicenseSection.tsx`

**Files:**
- Modify: `src/components/Settings/LicenseSection.tsx:216-225`

**Step 1: Add expiry date display**

After the "Last verified" line (line 225), add:

```tsx
{license.expires_at && (
  <div className="text-xs text-gray-500 dark:text-gray-400">
    {t("license.expiresAt", {
      date: new Date(license.expires_at * 1000).toLocaleDateString(),
    })}
  </div>
)}
```

**Step 2: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: PASS

**Step 3: Commit**

```bash
git add src/components/Settings/LicenseSection.tsx
git commit -m "feat(licensing): show license expiry date in Settings"
```

---

### Task 8: Add `"promo-expired"` event emission in hotkey handler

**Files:**
- Modify: `src-tauri/src/hotkey.rs`

**Step 1: Add an `AtomicBool` for one-time emission**

At the top of `hotkey.rs`, in the `RdevState` struct (around line 40-49), add a field:

```rust
/// Whether the promo-expired event has already been emitted this session.
promo_expired_emitted: AtomicBool,
```

Initialize it as `false` wherever `RdevState` is constructed.

**Step 2: Add the check in the recording start flow**

In the PTT handler (around line 560-574), after the license usage gate check, add:

```rust
// Promo expiration notification — check if license has expired
if let Some(info) = license_mgr.license_info() {
    if let Some(exp) = info.expires_at {
        if chrono::Utc::now().timestamp() >= exp {
            // One-time notification per session
            // Use a static AtomicBool since we can't easily pass RdevState here
            use std::sync::atomic::AtomicBool;
            static PROMO_EXPIRED_EMITTED: AtomicBool = AtomicBool::new(false);
            if !PROMO_EXPIRED_EMITTED.swap(true, Ordering::SeqCst) {
                let _ = app_for_err.emit("promo-expired", ());
            }
        }
    }
}
```

Place this right after the `match &voice_status` block and before the microphone setup code (before line 577). This way:
- It runs on every hotkey press
- The `static AtomicBool` ensures it only fires once per app session
- Recording still proceeds normally (Free tier with daily limits)

**Step 3: Build and verify**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: PASS

**Step 4: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat(licensing): emit promo-expired event on first hotkey after expiry"
```

---

### Task 9: Add promo-expired overlay in `Overlay.tsx`

**Files:**
- Modify: `src/components/Overlay.tsx`

**Step 1: Add state and event listener**

Add a `promoExpired` state alongside the existing `usageExhausted` state (around line 78):

```tsx
const [promoExpired, setPromoExpired] = useState(false);
```

In the `useEffect` (around line 136), add a listener:

```tsx
const unlistenPromoExpired = listen("promo-expired", () => {
  setPromoExpired(true);
  getCurrentWindow().setIgnoreCursorEvents(false).catch(() => {});
});
```

In the cleanup return, add:

```tsx
unlistenPromoExpired.then((fn) => fn());
```

**Step 2: Add the promo-expired overlay render**

After the `usageExhausted` render block (after line 189), before the normal state rendering, add:

```tsx
// Promo expired: amber/gold overlay with upgrade + dismiss buttons
if (promoExpired) {
  return (
    <div className="flex h-screen w-screen items-end justify-center pb-0">
      <div className="flex flex-col items-center gap-2 rounded-2xl bg-amber-900/90 px-6 py-3 shadow-lg backdrop-blur-md">
        <div className="flex items-center gap-2">
          <svg
            className="h-4 w-4 text-amber-400"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              d="M12 6v6h4.5m4.5 0a9 9 0 11-18 0 9 9 0 0118 0z"
            />
          </svg>
          <span className="text-xs font-semibold text-amber-200">
            {t("license.promoExpiredTitle")}
          </span>
        </div>
        <span className="text-[11px] text-amber-300/80">
          {t("license.promoExpiredMessage")}
        </span>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => {
              void openUrl(PURCHASE_URL);
              setPromoExpired(false);
              getCurrentWindow().setIgnoreCursorEvents(true).catch(() => {});
            }}
            className="rounded-full bg-amber-500 px-3 py-1 text-xs font-medium text-white hover:bg-amber-400"
          >
            {t("license.promoExpiredUpgrade")}
          </button>
          <button
            type="button"
            onClick={() => {
              setPromoExpired(false);
              getCurrentWindow().setIgnoreCursorEvents(true).catch(() => {});
            }}
            className="rounded-full border border-amber-500/50 px-3 py-1 text-xs font-medium text-amber-300 hover:bg-amber-800/50"
          >
            {t("license.promoExpiredDismiss")}
          </button>
        </div>
      </div>
    </div>
  );
}
```

**Step 3: Update the Idle check**

Update the early return (line 152) to also account for `promoExpired`:

```tsx
if (state.type === "Idle" && !usageExhausted && !promoExpired) {
  return null;
}
```

**Step 4: Verify frontend builds**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: PASS

**Step 5: Commit**

```bash
git add src/components/Overlay.tsx
git commit -m "feat(licensing): add promo-expired overlay with upgrade prompt"
```

---

### Task 10: Full build verification

**Files:** None (verification only)

**Step 1: Run all Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: ALL PASS

**Step 2: Run Rust lints**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: No warnings

**Step 3: Run frontend build**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm build`
Expected: PASS

**Step 4: Run full Tauri build (dev mode)**

Run: `cd /home/scipio/projects/voxpen-desktop && pnpm tauri build --debug 2>&1 | tail -5`
Expected: Build succeeds

**Step 5: Commit (if any clippy/format fixes needed)**

```bash
git add -A && git commit -m "chore: fix lint/format issues"
```
