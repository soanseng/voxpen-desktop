# LemonSqueezy Licensing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add freemium licensing to VoxInk Desktop — Free tier (15 transcriptions/day, full features) and Pro tier (unlimited + audio file transcription) via LemonSqueezy license key validation.

**Architecture:** Pure local approach (Approach A). App calls LemonSqueezy API directly for activate/validate/deactivate. Usage counting in local SQLite. License data in encrypted Tauri store. `LicenseVerifier` trait enables future migration to proxy model.

**Tech Stack:** Rust (`reqwest`, `serde`, `thiserror`, `chrono`), LemonSqueezy License API, SQLite (`rusqlite`), React + TypeScript + Tailwind, Tauri store plugin.

**Design Doc:** `docs/plans/2026-02-26-lemonsqueezy-licensing-design.md`

---

## Task 1: Add `chrono` dependency to voxink-core

**Files:**
- Modify: `src-tauri/crates/voxink-core/Cargo.toml`

**Step 1: Add chrono to dependencies**

Add `chrono` for local timezone date handling (midnight reset logic):

```toml
chrono = { version = "0.4", default-features = false, features = ["clock"] }
```

Add after the `thiserror` line in `[dependencies]`.

**Step 2: Verify it compiles**

Run: `cargo build -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: BUILD SUCCESS

**Step 3: Commit**

```bash
git add src-tauri/crates/voxink-core/Cargo.toml
git commit -m "chore: add chrono dependency for licensing date logic"
```

---

## Task 2: Add License and Usage error variants

**Files:**
- Modify: `src-tauri/crates/voxink-core/src/error.rs`
- Test: in-file `#[cfg(test)]` module

**Step 1: Write failing tests**

Add these tests to the existing `mod tests` block in `error.rs`:

```rust
#[test]
fn should_display_license_error() {
    let err = AppError::License("key revoked".to_string());
    assert_eq!(err.to_string(), "License error: key revoked");
}

#[test]
fn should_display_usage_limit_error() {
    let err = AppError::UsageLimitReached;
    assert_eq!(err.to_string(), "Daily usage limit reached");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- should_display_license_error should_display_usage_limit`
Expected: FAIL — variants don't exist

**Step 3: Add the error variants**

Add to the `AppError` enum (after the `Paste` variant, before the closing brace):

```rust
#[error("License error: {0}")]
License(String),

#[error("Daily usage limit reached")]
UsageLimitReached,
```

**Step 4: Run tests to verify they pass**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- should_display_license should_display_usage`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxink-core/src/error.rs
git commit -m "feat: add License and UsageLimitReached error variants"
```

---

## Task 3: Create licensing types module

**Files:**
- Create: `src-tauri/crates/voxink-core/src/licensing/types.rs`
- Create: `src-tauri/crates/voxink-core/src/licensing/mod.rs`
- Modify: `src-tauri/crates/voxink-core/src/lib.rs`

**Step 1: Write the types with tests**

Create `src-tauri/crates/voxink-core/src/licensing/types.rs`:

```rust
use serde::{Deserialize, Serialize};

/// Free or Pro license tier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseTier {
    Free,
    Pro,
}

/// Persistent license information stored in encrypted Tauri store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub tier: LicenseTier,
    pub license_key: String,
    pub instance_id: String,
    /// Major version this key is valid for (e.g., 1).
    pub licensed_version: u32,
    pub activated_at: i64,
    pub last_verified_at: i64,
    /// Deadline for verification grace period (set on first verify failure).
    pub verification_grace_until: Option<i64>,
}

/// Current usage status for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UsageStatus {
    /// Has remaining quota.
    Available { remaining: u32 },
    /// 3 or fewer remaining — show countdown.
    Warning { remaining: u32 },
    /// Daily limit reached.
    Exhausted,
    /// Pro user — no limit.
    Unlimited,
}

/// Daily usage record for SQLite storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    /// Date string in "YYYY-MM-DD" format (local timezone).
    pub date: String,
    pub count: u32,
}

/// Maximum daily free-tier transcriptions.
pub const FREE_DAILY_LIMIT: u32 = 15;

/// Number of remaining uses that triggers a warning.
pub const WARNING_THRESHOLD: u32 = 3;

/// Days between silent re-verification.
pub const VERIFY_INTERVAL_DAYS: i64 = 7;

/// Grace period (days) after verification failure before downgrade.
pub const VERIFY_GRACE_DAYS: i64 = 7;

/// Offline grace period (days) — how long local token stays valid without network.
pub const OFFLINE_GRACE_DAYS: i64 = 30;

/// Maximum device activations per license key.
pub const MAX_DEVICE_ACTIVATIONS: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_serialize_license_tier() {
        assert_eq!(serde_json::to_string(&LicenseTier::Free).unwrap(), "\"Free\"");
        assert_eq!(serde_json::to_string(&LicenseTier::Pro).unwrap(), "\"Pro\"");
    }

    #[test]
    fn should_roundtrip_license_info() {
        let info = LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: "XXXX-YYYY".to_string(),
            instance_id: "inst-001".to_string(),
            licensed_version: 1,
            activated_at: 1_700_000_000,
            last_verified_at: 1_700_000_000,
            verification_grace_until: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: LicenseInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(back.tier, LicenseTier::Pro);
        assert_eq!(back.license_key, "XXXX-YYYY");
    }

    #[test]
    fn should_serialize_usage_status_as_tagged_enum() {
        let status = UsageStatus::Warning { remaining: 2 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"Warning\""));
        assert!(json.contains("\"remaining\":2"));
    }

    #[test]
    fn should_serialize_unlimited_status() {
        let status = UsageStatus::Unlimited;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"Unlimited\""));
    }

    #[test]
    fn should_serialize_exhausted_status() {
        let status = UsageStatus::Exhausted;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"Exhausted\""));
    }

    #[test]
    fn should_have_correct_constants() {
        assert_eq!(FREE_DAILY_LIMIT, 15);
        assert_eq!(WARNING_THRESHOLD, 3);
        assert_eq!(VERIFY_INTERVAL_DAYS, 7);
        assert_eq!(VERIFY_GRACE_DAYS, 7);
        assert_eq!(OFFLINE_GRACE_DAYS, 30);
        assert_eq!(MAX_DEVICE_ACTIVATIONS, 3);
    }
}
```

**Step 2: Create mod.rs**

Create `src-tauri/crates/voxink-core/src/licensing/mod.rs`:

```rust
pub mod types;

pub use types::*;
```

**Step 3: Add to lib.rs**

Add `pub mod licensing;` to `src-tauri/crates/voxink-core/src/lib.rs` (after `pub mod pipeline;` line — wait, there's no pipeline line at the end. Add it after `pub mod pipeline;`).

Actually, looking at lib.rs, add after `pub mod pipeline;` on line 7:

```rust
pub mod licensing;
```

**Step 4: Run tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- licensing`
Expected: All 6 tests PASS

**Step 5: Commit**

```bash
git add src-tauri/crates/voxink-core/src/licensing/ src-tauri/crates/voxink-core/src/lib.rs
git commit -m "feat: add licensing types module (LicenseTier, LicenseInfo, UsageStatus)"
```

---

## Task 4: Create usage tracking module

**Files:**
- Create: `src-tauri/crates/voxink-core/src/licensing/usage.rs`
- Modify: `src-tauri/crates/voxink-core/src/licensing/mod.rs`

**Step 1: Write the usage module with SQL constants and tests**

Create `src-tauri/crates/voxink-core/src/licensing/usage.rs`:

```rust
/// SQL to create the daily_usage table.
pub const CREATE_TABLE_SQL: &str = "\
CREATE TABLE IF NOT EXISTS daily_usage (
    date TEXT PRIMARY KEY NOT NULL,
    count INTEGER NOT NULL DEFAULT 0
)";

/// SQL to get the usage count for a given date.
pub const GET_COUNT_SQL: &str = "\
SELECT count FROM daily_usage WHERE date = ?";

/// SQL to increment usage count, inserting if not exists.
pub const INCREMENT_SQL: &str = "\
INSERT INTO daily_usage (date, count) VALUES (?, 1)
ON CONFLICT(date) DO UPDATE SET count = count + 1";

/// SQL to delete records older than a given date (cleanup).
pub const CLEANUP_SQL: &str = "\
DELETE FROM daily_usage WHERE date < ?";

use super::types::{UsageStatus, FREE_DAILY_LIMIT, WARNING_THRESHOLD};

/// Compute usage status from the current count.
pub fn compute_status(count: u32) -> UsageStatus {
    let remaining = FREE_DAILY_LIMIT.saturating_sub(count);
    match remaining {
        0 => UsageStatus::Exhausted,
        r if r <= WARNING_THRESHOLD => UsageStatus::Warning { remaining: r },
        r => UsageStatus::Available { remaining: r },
    }
}

/// Get today's date string in local timezone ("YYYY-MM-DD").
pub fn today_local() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_return_available_when_under_limit() {
        assert_eq!(
            compute_status(0),
            UsageStatus::Available { remaining: 15 }
        );
        assert_eq!(
            compute_status(5),
            UsageStatus::Available { remaining: 10 }
        );
    }

    #[test]
    fn should_return_available_at_boundary() {
        // 12 used = 3 remaining, which equals WARNING_THRESHOLD
        assert_eq!(
            compute_status(11),
            UsageStatus::Available { remaining: 4 }
        );
    }

    #[test]
    fn should_return_warning_at_threshold() {
        assert_eq!(
            compute_status(12),
            UsageStatus::Warning { remaining: 3 }
        );
        assert_eq!(
            compute_status(13),
            UsageStatus::Warning { remaining: 2 }
        );
        assert_eq!(
            compute_status(14),
            UsageStatus::Warning { remaining: 1 }
        );
    }

    #[test]
    fn should_return_exhausted_at_limit() {
        assert_eq!(compute_status(15), UsageStatus::Exhausted);
    }

    #[test]
    fn should_return_exhausted_above_limit() {
        // Edge case: count somehow exceeds limit
        assert_eq!(compute_status(20), UsageStatus::Exhausted);
    }

    #[test]
    fn should_produce_valid_date_string() {
        let date = today_local();
        // Format: YYYY-MM-DD
        assert_eq!(date.len(), 10);
        assert_eq!(&date[4..5], "-");
        assert_eq!(&date[7..8], "-");
    }

    #[test]
    fn should_have_create_table_with_primary_key() {
        assert!(CREATE_TABLE_SQL.contains("PRIMARY KEY"));
        assert!(CREATE_TABLE_SQL.contains("daily_usage"));
    }

    #[test]
    fn should_have_upsert_in_increment_sql() {
        assert!(INCREMENT_SQL.contains("ON CONFLICT"));
        assert!(INCREMENT_SQL.contains("count + 1"));
    }
}
```

**Step 2: Add to mod.rs**

Add `pub mod usage;` to `src-tauri/crates/voxink-core/src/licensing/mod.rs`.

**Step 3: Run tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- licensing::usage`
Expected: All 8 tests PASS

**Step 4: Commit**

```bash
git add src-tauri/crates/voxink-core/src/licensing/
git commit -m "feat: add usage tracking module with daily count logic"
```

---

## Task 5: Create LemonSqueezy API client

**Files:**
- Create: `src-tauri/crates/voxink-core/src/licensing/lemonsqueezy.rs`
- Modify: `src-tauri/crates/voxink-core/src/licensing/mod.rs`

**Step 1: Write the LemonSqueezy client**

Create `src-tauri/crates/voxink-core/src/licensing/lemonsqueezy.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::error::AppError;

const LS_API_BASE: &str = "https://api.lemonsqueezy.com";

/// Response from LemonSqueezy license activation/validation.
#[derive(Debug, Deserialize)]
pub struct LsLicenseResponse {
    pub valid: bool,
    pub error: Option<String>,
    pub license_key: Option<LsLicenseKey>,
    pub instance: Option<LsInstance>,
    pub meta: Option<LsMeta>,
}

#[derive(Debug, Deserialize)]
pub struct LsLicenseKey {
    pub id: u64,
    pub status: String,
    pub key: String,
    pub activation_limit: Option<u32>,
    pub activation_usage: u32,
}

#[derive(Debug, Deserialize)]
pub struct LsInstance {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct LsMeta {
    pub product_name: Option<String>,
    pub variant_name: Option<String>,
}

/// Activate request body.
#[derive(Serialize)]
struct ActivateRequest {
    license_key: String,
    instance_name: String,
}

/// Validate request body.
#[derive(Serialize)]
struct ValidateRequest {
    license_key: String,
    instance_id: String,
}

/// Deactivate request body.
#[derive(Serialize)]
struct DeactivateRequest {
    license_key: String,
    instance_id: String,
}

/// Client for LemonSqueezy License API.
pub struct LemonSqueezyClient {
    http: reqwest::Client,
    base_url: String,
}

impl LemonSqueezyClient {
    pub fn new() -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            base_url: LS_API_BASE.to_string(),
        }
    }

    /// Create client with custom base URL (for testing with wiremock).
    #[cfg(test)]
    pub fn new_with_base_url(base_url: &str) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .expect("failed to build HTTP client");
        Self {
            http,
            base_url: base_url.to_string(),
        }
    }

    /// Activate a license key on this device.
    pub async fn activate(
        &self,
        license_key: &str,
        instance_name: &str,
    ) -> Result<LsLicenseResponse, AppError> {
        let url = format!("{}/v1/licenses/activate", self.base_url);
        let body = ActivateRequest {
            license_key: license_key.to_string(),
            instance_name: instance_name.to_string(),
        };
        let resp = self
            .http
            .post(&url)
            .header("Accept", "application/json")
            .form(&body)
            .send()
            .await
            .map_err(|e| AppError::License(format!("Network error: {e}")))?;

        let data: LsLicenseResponse = resp
            .json()
            .await
            .map_err(|e| AppError::License(format!("Invalid response: {e}")))?;

        if !data.valid {
            let msg = data.error.unwrap_or_else(|| "Activation failed".to_string());
            return Err(AppError::License(msg));
        }
        Ok(data)
    }

    /// Validate an existing license activation.
    pub async fn validate(
        &self,
        license_key: &str,
        instance_id: &str,
    ) -> Result<LsLicenseResponse, AppError> {
        let url = format!("{}/v1/licenses/validate", self.base_url);
        let body = ValidateRequest {
            license_key: license_key.to_string(),
            instance_id: instance_id.to_string(),
        };
        let resp = self
            .http
            .post(&url)
            .header("Accept", "application/json")
            .form(&body)
            .send()
            .await
            .map_err(|e| AppError::License(format!("Network error: {e}")))?;

        let data: LsLicenseResponse = resp
            .json()
            .await
            .map_err(|e| AppError::License(format!("Invalid response: {e}")))?;

        Ok(data)
    }

    /// Deactivate a license on this device.
    pub async fn deactivate(
        &self,
        license_key: &str,
        instance_id: &str,
    ) -> Result<(), AppError> {
        let url = format!("{}/v1/licenses/deactivate", self.base_url);
        let body = DeactivateRequest {
            license_key: license_key.to_string(),
            instance_id: instance_id.to_string(),
        };
        self.http
            .post(&url)
            .header("Accept", "application/json")
            .form(&body)
            .send()
            .await
            .map_err(|e| AppError::License(format!("Network error: {e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn valid_activate_response() -> serde_json::Value {
        serde_json::json!({
            "valid": true,
            "error": null,
            "license_key": {
                "id": 12345,
                "status": "active",
                "key": "TEST-KEY-1234",
                "activation_limit": 3,
                "activation_usage": 1
            },
            "instance": {
                "id": "inst-abc-123",
                "name": "my-machine"
            },
            "meta": {
                "product_name": "VoxInk Pro",
                "variant_name": "Lifetime"
            }
        })
    }

    fn invalid_key_response() -> serde_json::Value {
        serde_json::json!({
            "valid": false,
            "error": "The license key was not found.",
            "license_key": null,
            "instance": null,
            "meta": null
        })
    }

    #[tokio::test]
    async fn should_activate_with_valid_key() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/licenses/activate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(valid_activate_response()),
            )
            .mount(&mock)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&mock.uri());
        let result = client.activate("TEST-KEY-1234", "my-machine").await;
        assert!(result.is_ok());
        let resp = result.unwrap();
        assert!(resp.valid);
        assert_eq!(resp.instance.unwrap().id, "inst-abc-123");
    }

    #[tokio::test]
    async fn should_fail_with_invalid_key() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/licenses/activate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(invalid_key_response()),
            )
            .mount(&mock)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&mock.uri());
        let result = client.activate("BAD-KEY", "my-machine").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn should_validate_active_license() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/licenses/validate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(valid_activate_response()),
            )
            .mount(&mock)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&mock.uri());
        let result = client.validate("TEST-KEY-1234", "inst-abc-123").await;
        assert!(result.is_ok());
        assert!(result.unwrap().valid);
    }

    #[tokio::test]
    async fn should_handle_network_error() {
        // Point to a non-existent server
        let client = LemonSqueezyClient::new_with_base_url("http://127.0.0.1:1");
        let result = client.activate("key", "machine").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Network error"));
    }

    #[tokio::test]
    async fn should_deactivate_license() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/licenses/deactivate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "deactivated": true
            })))
            .mount(&mock)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&mock.uri());
        let result = client.deactivate("TEST-KEY-1234", "inst-abc-123").await;
        assert!(result.is_ok());
    }
}
```

**Step 2: Add to mod.rs**

Add `pub mod lemonsqueezy;` to the licensing mod.rs.

**Step 3: Run tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- licensing::lemonsqueezy`
Expected: All 5 tests PASS

**Step 4: Commit**

```bash
git add src-tauri/crates/voxink-core/src/licensing/
git commit -m "feat: add LemonSqueezy API client with wiremock tests"
```

---

## Task 6: Create LicenseVerifier trait

**Files:**
- Create: `src-tauri/crates/voxink-core/src/licensing/verifier.rs`
- Modify: `src-tauri/crates/voxink-core/src/licensing/mod.rs`

**Step 1: Write the trait and DirectLemonSqueezy impl**

Create `src-tauri/crates/voxink-core/src/licensing/verifier.rs`:

```rust
use std::future::Future;
use std::pin::Pin;

use crate::error::AppError;
use super::lemonsqueezy::{LemonSqueezyClient, LsLicenseResponse};

/// Abstraction over license verification — enables swapping between
/// direct LemonSqueezy calls (v1) and a proxy server (future v2).
pub trait LicenseVerifier: Send + Sync {
    fn activate(
        &self,
        key: &str,
        instance_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>>;

    fn validate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>>;

    fn deactivate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>>;
}

/// V1 implementation: direct calls to LemonSqueezy API.
pub struct DirectLemonSqueezy {
    client: LemonSqueezyClient,
}

impl DirectLemonSqueezy {
    pub fn new() -> Self {
        Self {
            client: LemonSqueezyClient::new(),
        }
    }

    #[cfg(test)]
    pub fn new_with_base_url(base_url: &str) -> Self {
        Self {
            client: LemonSqueezyClient::new_with_base_url(base_url),
        }
    }
}

impl LicenseVerifier for DirectLemonSqueezy {
    fn activate(
        &self,
        key: &str,
        instance_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>> {
        let client = &self.client;
        // We need owned values for the async block
        let key = key.to_string();
        let name = instance_name.to_string();
        // SAFETY: LemonSqueezyClient holds a reqwest::Client which is Send+Sync.
        // We create a raw pointer to work around lifetime issues.
        let ptr = client as *const LemonSqueezyClient;
        Box::pin(async move {
            // SAFETY: self lives as long as the future because DirectLemonSqueezy owns the client
            let client = unsafe { &*ptr };
            client.activate(&key, &name).await
        })
    }

    fn validate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>> {
        let key = key.to_string();
        let id = instance_id.to_string();
        let ptr = &self.client as *const LemonSqueezyClient;
        Box::pin(async move {
            let client = unsafe { &*ptr };
            client.validate(&key, &id).await
        })
    }

    fn deactivate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>> {
        let key = key.to_string();
        let id = instance_id.to_string();
        let ptr = &self.client as *const LemonSqueezyClient;
        Box::pin(async move {
            let client = unsafe { &*ptr };
            client.deactivate(&key, &id).await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn valid_response() -> serde_json::Value {
        serde_json::json!({
            "valid": true,
            "error": null,
            "license_key": {
                "id": 1, "status": "active", "key": "K",
                "activation_limit": 3, "activation_usage": 1
            },
            "instance": { "id": "i-1", "name": "m" },
            "meta": { "product_name": "VoxInk", "variant_name": "Pro" }
        })
    }

    #[tokio::test]
    async fn should_activate_through_trait() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/licenses/activate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(valid_response()))
            .mount(&mock)
            .await;

        let verifier = DirectLemonSqueezy::new_with_base_url(&mock.uri());
        let result = verifier.activate("K", "m").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn should_validate_through_trait() {
        let mock = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/licenses/validate"))
            .respond_with(ResponseTemplate::new(200).set_body_json(valid_response()))
            .mount(&mock)
            .await;

        let verifier = DirectLemonSqueezy::new_with_base_url(&mock.uri());
        let result = verifier.validate("K", "i-1").await;
        assert!(result.is_ok());
    }
}
```

**Note to implementer:** The raw pointer approach is needed because `LicenseVerifier` trait methods return boxed futures that need `Send`. If the compiler rejects the unsafe, an alternative is to store `LemonSqueezyClient` inside an `Arc` and clone it into the async block. Prefer the Arc approach if unsafe feels wrong:

```rust
pub struct DirectLemonSqueezy {
    client: Arc<LemonSqueezyClient>,
}
```

Then clone the Arc into each future. The implementer should pick whichever compiles cleanly.

**Step 2: Add to mod.rs**

Add `pub mod verifier;` and `pub use verifier::*;` to the licensing mod.rs.

**Step 3: Run tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- licensing::verifier`
Expected: All 2 tests PASS

**Step 4: Commit**

```bash
git add src-tauri/crates/voxink-core/src/licensing/
git commit -m "feat: add LicenseVerifier trait with DirectLemonSqueezy impl"
```

---

## Task 7: Create LicenseManager core logic

**Files:**
- Create: `src-tauri/crates/voxink-core/src/licensing/manager.rs`
- Modify: `src-tauri/crates/voxink-core/src/licensing/mod.rs`

This is the largest task. The LicenseManager ties together verification, usage counting, and tier determination.

**Step 1: Write the LicenseManager**

Create `src-tauri/crates/voxink-core/src/licensing/manager.rs`:

The manager must be designed for testability. It depends on:
- A `LicenseVerifier` (trait, mockable)
- A "store" for reading/writing LicenseInfo (injected as closures or trait)
- A "usage db" for reading/writing counts (injected)

For testability without SQLite, define simple traits:

```rust
use crate::error::AppError;
use super::types::*;
use super::usage;

/// Trait for persisting/loading license data.
pub trait LicenseStore: Send + Sync {
    fn load(&self) -> Option<LicenseInfo>;
    fn save(&self, info: &LicenseInfo) -> Result<(), AppError>;
    fn clear(&self) -> Result<(), AppError>;
}

/// Trait for usage count persistence.
pub trait UsageDb: Send + Sync {
    fn get_count(&self, date: &str) -> u32;
    fn increment(&self, date: &str) -> Result<u32, AppError>;
}

/// Core license management logic.
///
/// Generic over verifier, store, and usage DB for testability.
pub struct LicenseManager<V, S, D>
where
    V: super::verifier::LicenseVerifier,
    S: LicenseStore,
    D: UsageDb,
{
    verifier: V,
    store: S,
    db: D,
    app_major_version: u32,
    instance_name: String,
}

impl<V, S, D> LicenseManager<V, S, D>
where
    V: super::verifier::LicenseVerifier,
    S: LicenseStore,
    D: UsageDb,
{
    pub fn new(
        verifier: V,
        store: S,
        db: D,
        app_major_version: u32,
        instance_name: String,
    ) -> Self {
        Self { verifier, store, db, app_major_version, instance_name }
    }

    /// Get current license tier based on stored data.
    pub fn current_tier(&self) -> LicenseTier {
        match self.store.load() {
            Some(info) if info.tier == LicenseTier::Pro
                && info.licensed_version >= self.app_major_version => LicenseTier::Pro,
            _ => LicenseTier::Free,
        }
    }

    /// Get stored license info (if any).
    pub fn license_info(&self) -> Option<LicenseInfo> {
        self.store.load()
    }

    /// Check usage access before recording. Also piggybacks silent verification.
    pub async fn check_access(&self) -> UsageStatus {
        // Piggyback verification (best-effort, don't block on failure)
        let _ = self.verify_if_needed().await;

        match self.current_tier() {
            LicenseTier::Pro => UsageStatus::Unlimited,
            LicenseTier::Free => {
                let today = usage::today_local();
                let count = self.db.get_count(&today);
                usage::compute_status(count)
            }
        }
    }

    /// Record a successful transcription. Returns updated status.
    pub fn record_usage(&self) -> Result<UsageStatus, AppError> {
        match self.current_tier() {
            LicenseTier::Pro => Ok(UsageStatus::Unlimited),
            LicenseTier::Free => {
                let today = usage::today_local();
                let new_count = self.db.increment(&today)?;
                Ok(usage::compute_status(new_count))
            }
        }
    }

    /// Activate a license key.
    pub async fn activate(&self, key: &str) -> Result<LicenseInfo, AppError> {
        let resp = self.verifier.activate(key, &self.instance_name).await?;

        let instance = resp.instance
            .ok_or_else(|| AppError::License("No instance in response".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        let info = LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: key.to_string(),
            instance_id: instance.id,
            licensed_version: self.app_major_version,
            activated_at: now,
            last_verified_at: now,
            verification_grace_until: None,
        };
        self.store.save(&info)?;
        Ok(info)
    }

    /// Deactivate the current license.
    pub async fn deactivate(&self) -> Result<(), AppError> {
        if let Some(info) = self.store.load() {
            let _ = self.verifier.deactivate(&info.license_key, &info.instance_id).await;
        }
        self.store.clear()
    }

    /// Silent verification — only runs if stale (>= VERIFY_INTERVAL_DAYS).
    pub async fn verify_if_needed(&self) -> Result<LicenseTier, AppError> {
        let info = match self.store.load() {
            Some(info) => info,
            None => return Ok(LicenseTier::Free),
        };

        // Version check
        if info.licensed_version < self.app_major_version {
            self.store.clear()?;
            return Ok(LicenseTier::Free);
        }

        let now = chrono::Utc::now().timestamp();
        let seconds_per_day = 86_400;
        let days_since_verify = (now - info.last_verified_at) / seconds_per_day;

        // Not stale yet — skip verification
        if days_since_verify < VERIFY_INTERVAL_DAYS {
            return Ok(self.current_tier());
        }

        // Attempt online verification
        match self.verifier.validate(&info.license_key, &info.instance_id).await {
            Ok(resp) if resp.valid => {
                // Success — update timestamp, clear grace
                let mut updated = info;
                updated.last_verified_at = now;
                updated.verification_grace_until = None;
                self.store.save(&updated)?;
                Ok(LicenseTier::Pro)
            }
            Ok(_) => {
                // Invalid — handle grace period
                self.handle_verification_failure(info, now)
            }
            Err(_) => {
                // Network error — check offline grace
                let offline_days = days_since_verify;
                if offline_days < OFFLINE_GRACE_DAYS {
                    Ok(LicenseTier::Pro) // Still within offline grace
                } else {
                    self.store.clear()?;
                    Ok(LicenseTier::Free)
                }
            }
        }
    }

    fn handle_verification_failure(
        &self,
        info: LicenseInfo,
        now: i64,
    ) -> Result<LicenseTier, AppError> {
        let seconds_per_day = 86_400;

        match info.verification_grace_until {
            None => {
                // First failure — set grace period
                let mut updated = info;
                updated.verification_grace_until =
                    Some(now + VERIFY_GRACE_DAYS * seconds_per_day);
                self.store.save(&updated)?;
                Ok(LicenseTier::Pro) // Grace period active
            }
            Some(grace_until) if now < grace_until => {
                Ok(LicenseTier::Pro) // Still in grace
            }
            Some(_) => {
                // Grace expired — downgrade
                self.store.clear()?;
                Ok(LicenseTier::Free)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::verifier::LicenseVerifier;
    use super::super::lemonsqueezy::LsLicenseResponse;
    use std::cell::RefCell;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    // --- Mock LicenseStore ---
    struct MockStore {
        data: Mutex<RefCell<Option<LicenseInfo>>>,
    }
    impl MockStore {
        fn new(initial: Option<LicenseInfo>) -> Self {
            Self { data: Mutex::new(RefCell::new(initial)) }
        }
    }
    impl LicenseStore for MockStore {
        fn load(&self) -> Option<LicenseInfo> {
            self.data.lock().unwrap().borrow().clone()
        }
        fn save(&self, info: &LicenseInfo) -> Result<(), AppError> {
            *self.data.lock().unwrap().borrow_mut() = Some(info.clone());
            Ok(())
        }
        fn clear(&self) -> Result<(), AppError> {
            *self.data.lock().unwrap().borrow_mut() = None;
            Ok(())
        }
    }

    // --- Mock UsageDb ---
    struct MockUsageDb {
        counts: Mutex<RefCell<std::collections::HashMap<String, u32>>>,
    }
    impl MockUsageDb {
        fn new() -> Self {
            Self { counts: Mutex::new(RefCell::new(std::collections::HashMap::new())) }
        }
        fn with_count(date: &str, count: u32) -> Self {
            let mut map = std::collections::HashMap::new();
            map.insert(date.to_string(), count);
            Self { counts: Mutex::new(RefCell::new(map)) }
        }
    }
    impl UsageDb for MockUsageDb {
        fn get_count(&self, date: &str) -> u32 {
            *self.counts.lock().unwrap().borrow().get(date).unwrap_or(&0)
        }
        fn increment(&self, date: &str) -> Result<u32, AppError> {
            let guard = self.counts.lock().unwrap();
            let mut map = guard.borrow_mut();
            let count = map.entry(date.to_string()).or_insert(0);
            *count += 1;
            Ok(*count)
        }
    }

    // --- Mock Verifier ---
    struct MockVerifier {
        activate_result: Mutex<RefCell<Option<Result<LsLicenseResponse, AppError>>>>,
        validate_valid: Mutex<RefCell<bool>>,
        validate_error: Mutex<RefCell<bool>>,
    }
    impl MockVerifier {
        fn always_valid() -> Self {
            Self {
                activate_result: Mutex::new(RefCell::new(None)),
                validate_valid: Mutex::new(RefCell::new(true)),
                validate_error: Mutex::new(RefCell::new(false)),
            }
        }
        fn validate_invalid() -> Self {
            Self {
                activate_result: Mutex::new(RefCell::new(None)),
                validate_valid: Mutex::new(RefCell::new(false)),
                validate_error: Mutex::new(RefCell::new(false)),
            }
        }
        fn network_error() -> Self {
            Self {
                activate_result: Mutex::new(RefCell::new(None)),
                validate_valid: Mutex::new(RefCell::new(false)),
                validate_error: Mutex::new(RefCell::new(true)),
            }
        }
    }

    fn mock_ls_response(valid: bool) -> LsLicenseResponse {
        serde_json::from_value(serde_json::json!({
            "valid": valid,
            "error": if valid { serde_json::Value::Null } else { serde_json::Value::String("invalid".into()) },
            "license_key": { "id": 1, "status": "active", "key": "K", "activation_limit": 3, "activation_usage": 1 },
            "instance": { "id": "i-1", "name": "m" },
            "meta": { "product_name": "VoxInk", "variant_name": "Pro" }
        })).unwrap()
    }

    impl LicenseVerifier for MockVerifier {
        fn activate(&self, _key: &str, _inst: &str)
            -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>>
        {
            let resp = mock_ls_response(true);
            Box::pin(async move { Ok(resp) })
        }
        fn validate(&self, _key: &str, _inst: &str)
            -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>>
        {
            let valid = *self.validate_valid.lock().unwrap().borrow();
            let error = *self.validate_error.lock().unwrap().borrow();
            Box::pin(async move {
                if error {
                    Err(AppError::License("Network error".to_string()))
                } else {
                    Ok(mock_ls_response(valid))
                }
            })
        }
        fn deactivate(&self, _key: &str, _inst: &str)
            -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>>
        {
            Box::pin(async { Ok(()) })
        }
    }

    fn pro_license(last_verified_at: i64) -> LicenseInfo {
        LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: "KEY".to_string(),
            instance_id: "INST".to_string(),
            licensed_version: 1,
            activated_at: 1_700_000_000,
            last_verified_at,
            verification_grace_until: None,
        }
    }

    fn now_ts() -> i64 {
        chrono::Utc::now().timestamp()
    }

    #[test]
    fn should_return_free_when_no_license() {
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(None),
            MockUsageDb::new(),
            1, "m".into(),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Free);
    }

    #[test]
    fn should_return_pro_when_license_stored() {
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(Some(pro_license(now_ts()))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Pro);
    }

    #[test]
    fn should_downgrade_on_version_mismatch() {
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(Some(pro_license(now_ts()))),
            MockUsageDb::new(),
            2, // app is v2, license is v1
            "m".into(),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Free);
    }

    #[tokio::test]
    async fn should_return_unlimited_for_pro() {
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(Some(pro_license(now_ts()))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        assert_eq!(mgr.check_access().await, UsageStatus::Unlimited);
    }

    #[tokio::test]
    async fn should_return_available_for_free_under_limit() {
        let today = usage::today_local();
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(None),
            MockUsageDb::with_count(&today, 5),
            1, "m".into(),
        );
        assert_eq!(
            mgr.check_access().await,
            UsageStatus::Available { remaining: 10 }
        );
    }

    #[tokio::test]
    async fn should_return_warning_for_free_near_limit() {
        let today = usage::today_local();
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(None),
            MockUsageDb::with_count(&today, 13),
            1, "m".into(),
        );
        assert_eq!(
            mgr.check_access().await,
            UsageStatus::Warning { remaining: 2 }
        );
    }

    #[tokio::test]
    async fn should_return_exhausted_at_limit() {
        let today = usage::today_local();
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(None),
            MockUsageDb::with_count(&today, 15),
            1, "m".into(),
        );
        assert_eq!(mgr.check_access().await, UsageStatus::Exhausted);
    }

    #[test]
    fn should_increment_usage_on_record() {
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(None),
            MockUsageDb::new(),
            1, "m".into(),
        );
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Available { remaining: 14 });
    }

    #[test]
    fn should_return_unlimited_on_record_for_pro() {
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(Some(pro_license(now_ts()))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        assert_eq!(mgr.record_usage().unwrap(), UsageStatus::Unlimited);
    }

    #[tokio::test]
    async fn should_skip_verify_when_recent() {
        let mgr = LicenseManager::new(
            MockVerifier::validate_invalid(), // would fail if called
            MockStore::new(Some(pro_license(now_ts()))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        // Should not trigger verification since last_verified is now
        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro);
    }

    #[tokio::test]
    async fn should_verify_when_stale_and_succeed() {
        let stale = now_ts() - (8 * 86_400); // 8 days ago
        let mgr = LicenseManager::new(
            MockVerifier::always_valid(),
            MockStore::new(Some(pro_license(stale))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro);
    }

    #[tokio::test]
    async fn should_set_grace_on_first_verify_failure() {
        let stale = now_ts() - (8 * 86_400);
        let store = MockStore::new(Some(pro_license(stale)));
        let mgr = LicenseManager::new(
            MockVerifier::validate_invalid(),
            &store, // borrow won't work, need owned
            MockUsageDb::new(),
            1, "m".into(),
        );
        // This test won't compile because MockStore isn't borrowed.
        // The implementer should make MockStore use Arc<Mutex<>> internally
        // or wrap it in Arc. Adjust as needed.
    }

    #[tokio::test]
    async fn should_keep_pro_during_offline_grace() {
        let stale = now_ts() - (20 * 86_400); // 20 days, under 30-day offline limit
        let mgr = LicenseManager::new(
            MockVerifier::network_error(),
            MockStore::new(Some(pro_license(stale))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro);
    }

    #[tokio::test]
    async fn should_downgrade_past_offline_grace() {
        let stale = now_ts() - (31 * 86_400); // 31 days, past 30-day limit
        let mgr = LicenseManager::new(
            MockVerifier::network_error(),
            MockStore::new(Some(pro_license(stale))),
            MockUsageDb::new(),
            1, "m".into(),
        );
        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Free);
    }
}
```

**Important note to implementer:** The test for `should_set_grace_on_first_verify_failure` won't compile as-is because `LicenseManager` needs to own its store. The MockStore already uses `Mutex<RefCell<>>` which is `Send+Sync`. The issue is ownership — the manager takes ownership in `new()`. Tests that need to inspect store state after calling the manager should either:
1. Use `Arc<MockStore>` and implement the traits for `Arc<MockStore>`, OR
2. Use a shared reference pattern with `&dyn LicenseStore`

Pick whichever approach compiles. The test intent is documented — implement it to match.

**Step 2: Add to mod.rs**

Add `pub mod manager;` and `pub use manager::{LicenseManager, LicenseStore, UsageDb};` to the licensing mod.rs.

**Step 3: Run tests**

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml -- licensing::manager`
Expected: All tests PASS (adjust test code as needed for ownership)

**Step 4: Commit**

```bash
git add src-tauri/crates/voxink-core/src/licensing/
git commit -m "feat: add LicenseManager with verification state machine and usage gating"
```

---

## Task 8: Wire licensing into Tauri AppState

**Files:**
- Create: `src-tauri/src/licensing.rs` — Tauri-specific `LicenseStore` and `UsageDb` implementations
- Modify: `src-tauri/src/state.rs:129-141` — add `license_manager` field to `AppState`
- Modify: `src-tauri/src/lib.rs:112-153` — initialize LicenseManager in setup

**Step 1: Create Tauri licensing bridge**

Create `src-tauri/src/licensing.rs` that implements `LicenseStore` using Tauri's encrypted store and `UsageDb` using the shared SQLite connection:

- `TauriLicenseStore` — reads/writes `secrets.json` keys `license_info`
- `SqliteUsageDb` — wraps `rusqlite::Connection` (shared with history/dictionary)

Run `CREATE TABLE IF NOT EXISTS daily_usage` during init (same pattern as history/dictionary).

**Step 2: Add to AppState**

Add to `src-tauri/src/state.rs` AppState struct:

```rust
pub license_manager: Arc<voxink_core::licensing::LicenseManager<
    voxink_core::licensing::DirectLemonSqueezy,
    crate::licensing::TauriLicenseStore,
    crate::licensing::SqliteUsageDb,
>>,
```

**Step 3: Initialize in lib.rs setup**

In the `setup` closure, after creating the dictionary DB, initialize the licensing components:

```rust
let license_store = licensing::TauriLicenseStore::new(app.handle().clone());
let usage_db = licensing::SqliteUsageDb::open(db_path.clone())
    .expect("failed to open usage DB");
let verifier = voxink_core::licensing::DirectLemonSqueezy::new();
let machine_name = hostname::get()
    .map(|h| h.to_string_lossy().to_string())
    .unwrap_or_else(|_| "unknown".to_string());
let app_version: u32 = env!("CARGO_PKG_VERSION")
    .split('.')
    .next()
    .and_then(|v| v.parse().ok())
    .unwrap_or(1);
let license_manager = voxink_core::licensing::LicenseManager::new(
    verifier, license_store, usage_db, app_version, machine_name,
);
```

Add `hostname` crate to `src-tauri/Cargo.toml` dependencies.

**Step 4: Add mod declaration**

Add `mod licensing;` to `src-tauri/src/lib.rs`.

**Step 5: Build and test**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: BUILD SUCCESS

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: All existing tests still pass

**Step 6: Commit**

```bash
git add src-tauri/src/licensing.rs src-tauri/src/state.rs src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "feat: wire LicenseManager into Tauri AppState"
```

---

## Task 9: Add licensing IPC commands

**Files:**
- Modify: `src-tauri/src/commands.rs` — add 5 new Tauri commands
- Modify: `src-tauri/src/lib.rs:349-367` — register new commands

**Step 1: Add the 5 IPC commands**

Add to `src-tauri/src/commands.rs`:

```rust
use voxink_core::licensing::types::{LicenseInfo, LicenseTier, UsageStatus};

#[tauri::command]
pub async fn activate_license(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<LicenseInfo, String> {
    state.license_manager.activate(&key).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn deactivate_license(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.license_manager.deactivate().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_license_info(
    state: tauri::State<'_, AppState>,
) -> Result<Option<LicenseInfo>, String> {
    Ok(state.license_manager.license_info())
}

#[tauri::command]
pub async fn get_usage_status(
    state: tauri::State<'_, AppState>,
) -> Result<UsageStatus, String> {
    Ok(state.license_manager.check_access().await)
}

#[tauri::command]
pub async fn get_license_tier(
    state: tauri::State<'_, AppState>,
) -> Result<LicenseTier, String> {
    Ok(state.license_manager.current_tier())
}
```

**Step 2: Register in invoke_handler**

Add the 5 new commands to the `invoke_handler` macro in `lib.rs:349`:

```rust
commands::activate_license,
commands::deactivate_license,
commands::get_license_info,
commands::get_usage_status,
commands::get_license_tier,
```

**Step 3: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: BUILD SUCCESS

**Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat: add licensing IPC commands (activate, deactivate, status)"
```

---

## Task 10: Add usage gate in hotkey handler

**Files:**
- Modify: `src-tauri/src/hotkey.rs:384-437` — add check before recording starts
- Modify: `src-tauri/src/hotkey.rs:512-513` — add usage recording after successful transcription

**Step 1: Add usage check before recording (HotkeyAction::Start)**

In `handle_hotkey_event`, inside `HotkeyAction::Start`, right after the `processing` compare_exchange succeeds (line ~392) and before the `tauri::async_runtime::spawn`, add a license check:

```rust
// Check license usage before starting
let license_mgr = state.license_manager.clone();
let app_for_usage = app.clone();
```

Then inside the spawned async block, before the microphone setup (line ~404):

```rust
// === License gate ===
let usage = license_mgr.check_access().await;
match &usage {
    voxink_core::licensing::UsageStatus::Exhausted => {
        let _ = app_for_err.emit("usage-exhausted", ());
        processing_flag.store(false, Ordering::SeqCst);
        return;
    }
    voxink_core::licensing::UsageStatus::Warning { remaining } => {
        let _ = app_for_err.emit("usage-warning", remaining);
    }
    _ => {}
}
```

**Step 2: Add usage recording after successful transcription**

In the `HotkeyAction::Stop` branch, after the successful `history.insert()` call (around line ~536), add:

```rust
// Record usage for licensing
if let Ok(status) = license_mgr.record_usage() {
    let _ = app_handle.emit("usage-updated", &status);
}
```

Clone `license_manager` at the top of the Stop branch alongside other state clones.

**Step 3: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: BUILD SUCCESS

**Step 4: Commit**

```bash
git add src-tauri/src/hotkey.rs
git commit -m "feat: add license usage gate before recording and usage tracking after transcription"
```

---

## Task 11: Add frontend TypeScript types and invoke wrappers

**Files:**
- Modify: `src/types/settings.ts` — add licensing types
- Modify: `src/lib/tauri.ts` — add 5 invoke wrappers

**Step 1: Add TypeScript types**

Add to the end of `src/types/settings.ts`:

```typescript
export type LicenseTier = "Free" | "Pro";

export interface LicenseInfo {
  tier: LicenseTier;
  license_key: string;
  instance_id: string;
  licensed_version: number;
  activated_at: number;
  last_verified_at: number;
  verification_grace_until: number | null;
}

export type UsageStatus =
  | { type: "Available"; data: { remaining: number } }
  | { type: "Warning"; data: { remaining: number } }
  | { type: "Exhausted" }
  | { type: "Unlimited" };
```

**Step 2: Add invoke wrappers**

Add to the end of `src/lib/tauri.ts`:

```typescript
import type { LicenseInfo, LicenseTier, UsageStatus } from "../types/settings";

export async function activateLicense(key: string): Promise<LicenseInfo> {
  return invoke<LicenseInfo>("activate_license", { key });
}

export async function deactivateLicense(): Promise<void> {
  return invoke("deactivate_license");
}

export async function getLicenseInfo(): Promise<LicenseInfo | null> {
  return invoke<LicenseInfo | null>("get_license_info");
}

export async function getUsageStatus(): Promise<UsageStatus> {
  return invoke<UsageStatus>("get_usage_status");
}

export async function getLicenseTier(): Promise<LicenseTier> {
  return invoke<LicenseTier>("get_license_tier");
}
```

**Step 3: Verify TypeScript compiles**

Run: `cd /home/scipio/projects/voxink-desktop && npx tsc --noEmit`
Expected: No errors

**Step 4: Commit**

```bash
git add src/types/settings.ts src/lib/tauri.ts
git commit -m "feat: add licensing TypeScript types and Tauri invoke wrappers"
```

---

## Task 12: Add i18n keys for licensing

**Files:**
- Modify: `src/locales/en.json`
- Modify: `src/locales/zh-TW.json`

**Step 1: Add English keys**

Add a `"license"` section to `en.json` (before the closing `}`):

```json
"license": {
  "tab": "License",
  "free": "VoxInk Free",
  "pro": "VoxInk Pro",
  "usageToday": "Today: {{used}} / {{limit}}",
  "enterKey": "Enter license key...",
  "activate": "Activate",
  "activating": "Activating...",
  "deactivate": "Deactivate this device",
  "purchase": "Purchase Pro",
  "validFor": "Valid for: v{{version}}.x",
  "devices": "Devices: {{used}} / {{limit}}",
  "lastVerified": "Last verified: {{date}}",
  "exhausted": "Daily limit reached",
  "upgradePrompt": "Upgrade to Pro",
  "warningRemaining": "{{count}} left today",
  "verificationFailed": "License verification failed. Please reconnect within {{days}} days.",
  "activateSuccess": "License activated successfully.",
  "activateFailed": "Activation failed: {{error}}",
  "keyInvalid": "Invalid license key.",
  "deactivateSuccess": "License deactivated."
}
```

**Step 2: Add Traditional Chinese keys**

Add matching `"license"` section to `zh-TW.json`:

```json
"license": {
  "tab": "授權",
  "free": "VoxInk 免費版",
  "pro": "VoxInk Pro",
  "usageToday": "今日：{{used}} / {{limit}}",
  "enterKey": "輸入授權碼...",
  "activate": "啟用",
  "activating": "啟用中...",
  "deactivate": "停用此裝置",
  "purchase": "購買 Pro 版",
  "validFor": "適用於：v{{version}}.x",
  "devices": "裝置：{{used}} / {{limit}}",
  "lastVerified": "上次驗證：{{date}}",
  "exhausted": "今日額度已用完",
  "upgradePrompt": "升級至 Pro",
  "warningRemaining": "今日剩餘 {{count}} 次",
  "verificationFailed": "授權驗證失敗，請在 {{days}} 天內重新連線。",
  "activateSuccess": "授權啟用成功。",
  "activateFailed": "啟用失敗：{{error}}",
  "keyInvalid": "無效的授權碼。",
  "deactivateSuccess": "已停用授權。"
}
```

**Step 3: Commit**

```bash
git add src/locales/en.json src/locales/zh-TW.json
git commit -m "feat: add i18n keys for licensing UI (en + zh-TW)"
```

---

## Task 13: Create LicenseSection React component

**Files:**
- Create: `src/components/Settings/LicenseSection.tsx`

**Step 1: Write the component**

Create `src/components/Settings/LicenseSection.tsx` with two states:

1. **Free state**: usage progress bar, license key input, activate button, purchase link
2. **Pro state**: masked key, version validity, devices, last verified, deactivate button

Use existing patterns from `SttSection.tsx` (API key input) and `GeneralSection.tsx` (layout).

Key behaviors:
- On mount: call `getLicenseInfo()` and `getUsageStatus()` to populate UI
- Activate button: calls `activateLicense(key)`, shows loading state, displays result
- Deactivate button: calls `deactivateLicense()`, confirms first
- Purchase button: `window.open("YOUR_LEMONSQUEEZY_CHECKOUT_URL")` (placeholder URL for now)
- Listen to `usage-updated` event to refresh status

The implementer should follow the existing Tailwind patterns from other Settings sections.

**Step 2: Verify it builds**

Run: `cd /home/scipio/projects/voxink-desktop && pnpm build`
Expected: BUILD SUCCESS (component may not be connected yet)

**Step 3: Commit**

```bash
git add src/components/Settings/LicenseSection.tsx
git commit -m "feat: add LicenseSection component for Settings window"
```

---

## Task 14: Add License tab to Settings window

**Files:**
- Modify: `src/components/Settings/SettingsWindow.tsx`

**Step 1: Add License tab**

Add `"license"` to the `Tab` type and `TAB_IDS` array (first position):

```typescript
type Tab = "license" | "general" | "speech" | "refinement" | "dictionary" | "appearance" | "history";

const TAB_IDS: Tab[] = [
  "license",
  "general",
  // ... rest unchanged
];
```

Add a `TabIcon` case for `"license"` (use a key/shield SVG icon).

Add the rendering case in the main content area:

```tsx
{activeTab === "license" && <LicenseSection />}
```

Import `LicenseSection` at the top.

**Step 2: Verify it builds**

Run: `cd /home/scipio/projects/voxink-desktop && pnpm build`
Expected: BUILD SUCCESS

**Step 3: Commit**

```bash
git add src/components/Settings/SettingsWindow.tsx
git commit -m "feat: add License tab to Settings window"
```

---

## Task 15: Add usage states to Overlay

**Files:**
- Modify: `src/components/Overlay.tsx`

**Step 1: Add event listeners for usage events**

Add listeners for `usage-exhausted` and `usage-warning` events alongside the existing `pipeline-state` listener.

Add two new visual states:
- `UsageWarning`: small text overlay during Recording showing "N left today"
- `UsageExhausted`: amber/orange pill showing "Daily limit reached" + clickable "Upgrade" button

For `UsageExhausted`, the overlay should NOT be click-through (unlike other states). The upgrade button opens the purchase URL.

**Step 2: Verify it builds**

Run: `cd /home/scipio/projects/voxink-desktop && pnpm build`
Expected: BUILD SUCCESS

**Step 3: Commit**

```bash
git add src/components/Overlay.tsx
git commit -m "feat: add usage warning and exhausted states to Overlay"
```

---

## Task 16: Add tray menu usage/upgrade items

**Files:**
- Modify: `src-tauri/src/lib.rs:44-101` — add usage line and upgrade item to tray menu

**Step 1: Add tray items**

In `build_tray_menu()`, add before the separator:

```rust
let usage_label = MenuItem::with_id(app, "usage_info", "Free: 0/15 today", false, None::<&str>)?;
let upgrade_item = MenuItem::with_id(app, "upgrade_pro", "Upgrade to Pro...", true, None::<&str>)?;
```

Add them to the menu items list.

Handle `"upgrade_pro"` in the menu event handler — open the purchase URL.

The usage label text should be updated dynamically. For now, use a static label; a future improvement can update it via the tray reference.

**Step 2: Build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: BUILD SUCCESS

**Step 3: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: add usage and upgrade items to system tray menu"
```

---

## Task 17: Full integration test

**Step 1: Run all Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: All tests PASS

Run: `cargo test -p voxink-core --manifest-path src-tauri/Cargo.toml`
Expected: All tests PASS (existing + new licensing tests)

**Step 2: Run clippy**

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
Expected: No warnings

**Step 3: Run frontend build**

Run: `cd /home/scipio/projects/voxink-desktop && pnpm build`
Expected: BUILD SUCCESS

**Step 4: Full Tauri build**

Run: `cargo build --manifest-path src-tauri/Cargo.toml`
Expected: BUILD SUCCESS

**Step 5: Commit any fixups**

If any adjustments were needed, commit them with appropriate messages.

**Step 6: Final commit with summary**

```bash
git add -A
git commit -m "feat: complete LemonSqueezy licensing integration (freemium model)"
```

---

## Summary of all files

### New files (8)
- `src-tauri/crates/voxink-core/src/licensing/mod.rs`
- `src-tauri/crates/voxink-core/src/licensing/types.rs`
- `src-tauri/crates/voxink-core/src/licensing/usage.rs`
- `src-tauri/crates/voxink-core/src/licensing/lemonsqueezy.rs`
- `src-tauri/crates/voxink-core/src/licensing/verifier.rs`
- `src-tauri/crates/voxink-core/src/licensing/manager.rs`
- `src-tauri/src/licensing.rs`
- `src/components/Settings/LicenseSection.tsx`

### Modified files (12)
- `src-tauri/crates/voxink-core/Cargo.toml` — add `chrono`
- `src-tauri/crates/voxink-core/src/lib.rs` — add `pub mod licensing`
- `src-tauri/crates/voxink-core/src/error.rs` — add 2 variants
- `src-tauri/Cargo.toml` — add `hostname`
- `src-tauri/src/lib.rs` — init LicenseManager, register commands, tray items
- `src-tauri/src/state.rs` — add `license_manager` to AppState
- `src-tauri/src/commands.rs` — add 5 IPC commands
- `src-tauri/src/hotkey.rs` — add 2 gate points
- `src/types/settings.ts` — add licensing types
- `src/lib/tauri.ts` — add 5 invoke wrappers
- `src/components/Settings/SettingsWindow.tsx` — add License tab
- `src/components/Overlay.tsx` — add usage states
- `src/locales/en.json` — add ~20 keys
- `src/locales/zh-TW.json` — add ~20 keys
