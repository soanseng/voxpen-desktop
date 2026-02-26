use crate::error::AppError;
use crate::licensing::types::{
    LicenseInfo, LicenseTier, UsageStatus, FREE_DAILY_LIMIT, OFFLINE_GRACE_DAYS,
    VERIFY_GRACE_DAYS, VERIFY_INTERVAL_DAYS,
};
use crate::licensing::usage::{compute_status, today_local};
use crate::licensing::verifier::LicenseVerifier;

/// Current app major version — licenses are bound to a major version.
pub const CURRENT_MAJOR_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// Storage and DB traits (DI for testability without Tauri/SQLite)
// ---------------------------------------------------------------------------

/// Trait for persisting license information.
pub trait LicenseStore: Send + Sync {
    fn load(&self) -> Option<LicenseInfo>;
    fn save(&self, info: &LicenseInfo) -> Result<(), AppError>;
    fn clear(&self) -> Result<(), AppError>;
}

/// Trait for daily usage counting.
pub trait UsageDb: Send + Sync {
    fn get_count(&self, date: &str) -> u32;
    fn increment(&self, date: &str) -> Result<u32, AppError>;
}

// ---------------------------------------------------------------------------
// LicenseManager
// ---------------------------------------------------------------------------

/// Orchestrates license verification, usage tracking, and access gating.
///
/// Generic over its dependencies for testability:
/// - `V`: license verification backend (LemonSqueezy or mock)
/// - `S`: license persistence (Tauri store or mock)
/// - `D`: daily usage database (SQLite or mock)
pub struct LicenseManager<V: LicenseVerifier, S: LicenseStore, D: UsageDb> {
    verifier: V,
    store: S,
    usage_db: D,
}

impl<V: LicenseVerifier, S: LicenseStore, D: UsageDb> LicenseManager<V, S, D> {
    pub fn new(verifier: V, store: S, usage_db: D) -> Self {
        Self {
            verifier,
            store,
            usage_db,
        }
    }

    /// Return the current tier based on stored license data.
    ///
    /// Returns Free if no license is stored or if the license's major version
    /// does not match the current app version.
    pub fn current_tier(&self) -> LicenseTier {
        match self.store.load() {
            Some(info) if info.licensed_version == CURRENT_MAJOR_VERSION => info.tier,
            _ => LicenseTier::Free,
        }
    }

    /// Return stored license info if present.
    pub fn license_info(&self) -> Option<LicenseInfo> {
        self.store.load()
    }

    /// Check access: piggyback a silent verify, then return usage status.
    ///
    /// For Pro users, returns `Unlimited`.
    /// For Free users, returns the computed status based on today's count.
    pub async fn check_access(&self) -> UsageStatus {
        // Piggyback verify — errors are swallowed (best-effort)
        let _ = self.verify_if_needed().await;

        match self.current_tier() {
            LicenseTier::Pro => UsageStatus::Unlimited,
            LicenseTier::Free => {
                let count = self.usage_db.get_count(&today_local());
                compute_status(count)
            }
        }
    }

    /// Record one transcription usage, returning the new status.
    ///
    /// Pro users always get `Unlimited`. Free users get an error if exhausted.
    pub fn record_usage(&self) -> Result<UsageStatus, AppError> {
        if self.current_tier() == LicenseTier::Pro {
            return Ok(UsageStatus::Unlimited);
        }

        let date = today_local();
        let current_count = self.usage_db.get_count(&date);

        if current_count >= FREE_DAILY_LIMIT {
            return Err(AppError::UsageLimitReached);
        }

        let new_count = self.usage_db.increment(&date)?;
        Ok(compute_status(new_count))
    }

    /// Activate a license key, storing the result on success.
    pub async fn activate(&self, key: &str) -> Result<LicenseInfo, AppError> {
        let hostname = hostname_or_default();
        let response = self.verifier.activate(key, &hostname).await?;

        let instance_id = response
            .instance
            .as_ref()
            .and_then(|i| i.id.clone())
            .ok_or_else(|| AppError::License("no instance ID in response".to_string()))?;

        let now = chrono::Utc::now().timestamp();
        let info = LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: key.to_string(),
            instance_id,
            licensed_version: CURRENT_MAJOR_VERSION,
            activated_at: now,
            last_verified_at: now,
            verification_grace_until: None,
        };

        self.store.save(&info)?;
        Ok(info)
    }

    /// Deactivate the current license, clearing local storage.
    ///
    /// If the remote API reports `instance_id not found` (HTTP 404), the
    /// instance is already gone server-side, so we still clear locally.
    pub async fn deactivate(&self) -> Result<(), AppError> {
        let info = self
            .store
            .load()
            .ok_or_else(|| AppError::License("no active license".to_string()))?;

        match self
            .verifier
            .deactivate(&info.license_key, &info.instance_id)
            .await
        {
            Ok(()) => {}
            Err(ref e) if e.to_string().contains("instance_id not found") => {
                eprintln!("[license] deactivate: instance already gone on server, clearing locally");
            }
            Err(ref e) if e.to_string().contains("HTTP 404") => {
                eprintln!("[license] deactivate: 404 from server, clearing locally");
            }
            Err(e) => return Err(e),
        }

        self.store.clear()
    }

    /// Verify the license if needed. Implements the silent verification state machine:
    ///
    /// 1. No license stored => Free (no-op)
    /// 2. Version mismatch  => clear license, return Free
    /// 3. Recently verified (< VERIFY_INTERVAL_DAYS) => skip, return current tier
    /// 4. Online + valid    => update last_verified_at, clear grace, return Pro
    /// 5. Online + invalid  => first failure: set grace_until, keep Pro.
    ///    Past grace: clear license, return Free
    /// 6. Offline (network) => within OFFLINE_GRACE_DAYS of last verify: keep Pro.
    ///    Beyond: clear license, return Free
    pub async fn verify_if_needed(&self) -> Result<LicenseTier, AppError> {
        let info = match self.store.load() {
            Some(info) => info,
            None => return Ok(LicenseTier::Free),
        };

        // Version mismatch — license no longer valid for this major version
        if info.licensed_version != CURRENT_MAJOR_VERSION {
            let _ = self.store.clear();
            return Ok(LicenseTier::Free);
        }

        let now = chrono::Utc::now().timestamp();
        let secs_per_day: i64 = 86400;
        let days_since_verify = (now - info.last_verified_at) / secs_per_day;

        // Recently verified — skip network call
        if days_since_verify < VERIFY_INTERVAL_DAYS {
            return Ok(info.tier);
        }

        // Attempt online verification
        let verify_result = self
            .verifier
            .validate(&info.license_key, &info.instance_id)
            .await;

        match verify_result {
            Ok(response) if response.valid => {
                // Success — update timestamp, clear any grace period
                let updated = LicenseInfo {
                    last_verified_at: now,
                    verification_grace_until: None,
                    ..info
                };
                let _ = self.store.save(&updated);
                Ok(LicenseTier::Pro)
            }
            Ok(_) => {
                // Online but invalid — check grace period
                match info.verification_grace_until {
                    None => {
                        // First failure: set grace period
                        let grace_until = now + VERIFY_GRACE_DAYS * secs_per_day;
                        let updated = LicenseInfo {
                            verification_grace_until: Some(grace_until),
                            ..info
                        };
                        let _ = self.store.save(&updated);
                        Ok(LicenseTier::Pro)
                    }
                    Some(grace_until) if now < grace_until => {
                        // Still within grace period
                        Ok(LicenseTier::Pro)
                    }
                    Some(_) => {
                        // Past grace — downgrade
                        let _ = self.store.clear();
                        Ok(LicenseTier::Free)
                    }
                }
            }
            Err(_) => {
                // Offline / network error — check offline grace
                let days_offline = (now - info.last_verified_at) / secs_per_day;
                if days_offline < OFFLINE_GRACE_DAYS {
                    Ok(LicenseTier::Pro)
                } else {
                    let _ = self.store.clear();
                    Ok(LicenseTier::Free)
                }
            }
        }
    }
}

/// Best-effort hostname for the instance name during activation.
fn hostname_or_default() -> String {
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "VoxPen Desktop".to_string())
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Mutex;

    use crate::licensing::lemonsqueezy::LsLicenseResponse;

    // -- Mock LicenseStore --------------------------------------------------

    struct MockStore {
        data: Mutex<Option<LicenseInfo>>,
    }

    impl MockStore {
        fn new(initial: Option<LicenseInfo>) -> Self {
            Self {
                data: Mutex::new(initial),
            }
        }
    }

    impl LicenseStore for MockStore {
        fn load(&self) -> Option<LicenseInfo> {
            self.data.lock().unwrap().clone()
        }
        fn save(&self, info: &LicenseInfo) -> Result<(), AppError> {
            *self.data.lock().unwrap() = Some(info.clone());
            Ok(())
        }
        fn clear(&self) -> Result<(), AppError> {
            *self.data.lock().unwrap() = None;
            Ok(())
        }
    }

    // -- Mock UsageDb -------------------------------------------------------

    struct MockUsageDb {
        count: Mutex<u32>,
    }

    impl MockUsageDb {
        fn new(initial: u32) -> Self {
            Self {
                count: Mutex::new(initial),
            }
        }
    }

    impl UsageDb for MockUsageDb {
        fn get_count(&self, _date: &str) -> u32 {
            *self.count.lock().unwrap()
        }
        fn increment(&self, _date: &str) -> Result<u32, AppError> {
            let mut val = self.count.lock().unwrap();
            *val += 1;
            Ok(*val)
        }
    }

    // -- Mock Verifier (configurable responses) -----------------------------

    #[derive(Clone, Debug)]
    enum VerifyBehavior {
        ActivateOk,
        ActivateErr(String),
        ValidateOk,
        ValidateInvalid,
        ValidateErr,
        DeactivateOk,
        DeactivateErr(String),
    }

    struct MockVerifier {
        behaviors: Mutex<Vec<VerifyBehavior>>,
    }

    impl MockVerifier {
        fn new(behaviors: Vec<VerifyBehavior>) -> Self {
            Self {
                behaviors: Mutex::new(behaviors),
            }
        }

        fn pop_behavior(&self) -> VerifyBehavior {
            let mut vec = self.behaviors.lock().unwrap();
            if vec.is_empty() {
                panic!("MockVerifier: no more behaviors configured");
            }
            vec.remove(0)
        }
    }

    fn ok_license_response(instance_id: &str) -> LsLicenseResponse {
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
            }),
            instance: Some(LsInstance {
                id: Some(instance_id.to_string()),
                name: Some("Test".to_string()),
            }),
            meta: None,
        }
    }

    fn invalid_license_response() -> LsLicenseResponse {
        LsLicenseResponse {
            valid: false,
            error: Some("expired".to_string()),
            license_key: None,
            instance: None,
            meta: None,
        }
    }

    impl LicenseVerifier for MockVerifier {
        fn activate(
            &self,
            _key: &str,
            _instance_name: &str,
        ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>> {
            let behavior = self.pop_behavior();
            Box::pin(async move {
                match behavior {
                    VerifyBehavior::ActivateOk => Ok(ok_license_response("inst-new")),
                    VerifyBehavior::ActivateErr(msg) => Err(AppError::License(msg)),
                    other => panic!("unexpected behavior for activate: {other:?}"),
                }
            })
        }

        fn validate(
            &self,
            _key: &str,
            _instance_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>> {
            let behavior = self.pop_behavior();
            Box::pin(async move {
                match behavior {
                    VerifyBehavior::ValidateOk => Ok(ok_license_response("inst-001")),
                    VerifyBehavior::ValidateInvalid => Ok(invalid_license_response()),
                    VerifyBehavior::ValidateErr => {
                        Err(AppError::License("network error".to_string()))
                    }
                    other => panic!("unexpected behavior for validate: {other:?}"),
                }
            })
        }

        fn deactivate(
            &self,
            _key: &str,
            _instance_id: &str,
        ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>> {
            let behavior = self.pop_behavior();
            Box::pin(async move {
                match behavior {
                    VerifyBehavior::DeactivateOk => Ok(()),
                    VerifyBehavior::DeactivateErr(msg) => Err(AppError::License(msg)),
                    other => panic!("unexpected behavior for deactivate: {other:?}"),
                }
            })
        }
    }

    // -- Helper to build a stored LicenseInfo -------------------------------

    fn pro_license(last_verified_at: i64) -> LicenseInfo {
        LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: "KEY-123".to_string(),
            instance_id: "inst-001".to_string(),
            licensed_version: CURRENT_MAJOR_VERSION,
            activated_at: 1700000000,
            last_verified_at,
            verification_grace_until: None,
        }
    }

    fn pro_license_wrong_version() -> LicenseInfo {
        LicenseInfo {
            licensed_version: CURRENT_MAJOR_VERSION + 1,
            ..pro_license(chrono::Utc::now().timestamp())
        }
    }

    fn pro_license_with_grace(last_verified_at: i64, grace_until: i64) -> LicenseInfo {
        LicenseInfo {
            verification_grace_until: Some(grace_until),
            ..pro_license(last_verified_at)
        }
    }

    // =======================================================================
    // current_tier tests
    // =======================================================================

    #[test]
    fn current_tier_should_be_free_without_license() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Free);
    }

    #[test]
    fn current_tier_should_be_pro_with_license() {
        let now = chrono::Utc::now().timestamp();
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(Some(pro_license(now))),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Pro);
    }

    #[test]
    fn current_tier_should_be_free_on_version_mismatch() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(Some(pro_license_wrong_version())),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Free);
    }

    // =======================================================================
    // check_access tests
    // =======================================================================

    #[tokio::test]
    async fn check_access_should_return_unlimited_for_pro() {
        let now = chrono::Utc::now().timestamp();
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(Some(pro_license(now))),
            MockUsageDb::new(5),
        );
        assert_eq!(mgr.check_access().await, UsageStatus::Unlimited);
    }

    #[tokio::test]
    async fn check_access_should_return_available_for_free_low_count() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(3),
        );
        assert_eq!(
            mgr.check_access().await,
            UsageStatus::Available { remaining: 12 }
        );
    }

    #[tokio::test]
    async fn check_access_should_return_warning_for_free_high_count() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(13),
        );
        assert_eq!(
            mgr.check_access().await,
            UsageStatus::Warning { remaining: 2 }
        );
    }

    #[tokio::test]
    async fn check_access_should_return_exhausted_for_free_at_limit() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(15),
        );
        assert_eq!(mgr.check_access().await, UsageStatus::Exhausted);
    }

    // =======================================================================
    // record_usage tests
    // =======================================================================

    #[test]
    fn record_usage_should_return_unlimited_for_pro() {
        let now = chrono::Utc::now().timestamp();
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(Some(pro_license(now))),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.record_usage().unwrap(), UsageStatus::Unlimited);
    }

    #[test]
    fn record_usage_should_increment_count_for_free() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );
        // After increment: count = 1, remaining = 14
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Available { remaining: 14 });
    }

    #[test]
    fn record_usage_should_return_warning_near_limit() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(11),
        );
        // After increment: count = 12, remaining = 3
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Warning { remaining: 3 });
    }

    #[test]
    fn record_usage_should_return_exhausted_at_limit() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(14),
        );
        // After increment: count = 15
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Exhausted);
    }

    #[test]
    fn record_usage_should_error_when_already_exhausted() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(15),
        );
        let result = mgr.record_usage();
        assert!(matches!(result, Err(AppError::UsageLimitReached)));
    }

    // =======================================================================
    // activate tests
    // =======================================================================

    #[tokio::test]
    async fn activate_should_store_license_on_success() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ActivateOk]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );

        let info = mgr.activate("KEY-NEW").await.unwrap();
        assert_eq!(info.tier, LicenseTier::Pro);
        assert_eq!(info.instance_id, "inst-new");
        assert_eq!(info.licensed_version, CURRENT_MAJOR_VERSION);

        // Should now be stored
        assert_eq!(mgr.current_tier(), LicenseTier::Pro);
    }

    #[tokio::test]
    async fn activate_should_propagate_error() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ActivateErr(
                "invalid key".to_string(),
            )]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );

        let result = mgr.activate("BAD-KEY").await;
        assert!(matches!(result, Err(AppError::License(_))));
        assert_eq!(mgr.current_tier(), LicenseTier::Free);
    }

    // =======================================================================
    // deactivate tests
    // =======================================================================

    #[tokio::test]
    async fn deactivate_should_clear_license() {
        let now = chrono::Utc::now().timestamp();
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::DeactivateOk]),
            MockStore::new(Some(pro_license(now))),
            MockUsageDb::new(0),
        );

        assert_eq!(mgr.current_tier(), LicenseTier::Pro);
        mgr.deactivate().await.unwrap();
        assert_eq!(mgr.current_tier(), LicenseTier::Free);
    }

    #[tokio::test]
    async fn deactivate_should_error_without_license() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );

        let result = mgr.deactivate().await;
        assert!(matches!(result, Err(AppError::License(_))));
    }

    // =======================================================================
    // verify_if_needed tests
    // =======================================================================

    #[tokio::test]
    async fn verify_should_return_free_without_license() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.verify_if_needed().await.unwrap(), LicenseTier::Free);
    }

    #[tokio::test]
    async fn verify_should_return_free_on_version_mismatch() {
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]),
            MockStore::new(Some(pro_license_wrong_version())),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.verify_if_needed().await.unwrap(), LicenseTier::Free);
        // Should have cleared the store
        assert!(mgr.license_info().is_none());
    }

    #[tokio::test]
    async fn verify_should_skip_when_recently_verified() {
        let now = chrono::Utc::now().timestamp();
        // Verified 1 day ago — should skip
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![]), // no verify behavior needed
            MockStore::new(Some(pro_license(now - 86400))),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.verify_if_needed().await.unwrap(), LicenseTier::Pro);
    }

    #[tokio::test]
    async fn verify_should_update_timestamp_on_success() {
        let now = chrono::Utc::now().timestamp();
        // Verified 8 days ago — should trigger verify
        let stale_time = now - 8 * 86400;
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ValidateOk]),
            MockStore::new(Some(pro_license(stale_time))),
            MockUsageDb::new(0),
        );

        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro);

        // Timestamp should be updated
        let info = mgr.license_info().unwrap();
        assert!(info.last_verified_at > stale_time);
        assert!(info.verification_grace_until.is_none());
    }

    #[tokio::test]
    async fn verify_should_set_grace_on_first_failure() {
        let now = chrono::Utc::now().timestamp();
        let stale_time = now - 8 * 86400;
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ValidateInvalid]),
            MockStore::new(Some(pro_license(stale_time))),
            MockUsageDb::new(0),
        );

        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro); // Still Pro during grace

        let info = mgr.license_info().unwrap();
        assert!(info.verification_grace_until.is_some());
        let grace = info.verification_grace_until.unwrap();
        // Grace should be ~7 days from now
        assert!(grace > now);
        assert!(grace <= now + VERIFY_GRACE_DAYS * 86400 + 1);
    }

    #[tokio::test]
    async fn verify_should_keep_pro_within_grace_period() {
        let now = chrono::Utc::now().timestamp();
        let stale_time = now - 8 * 86400;
        // Grace period set in the future
        let grace_until = now + 3 * 86400;
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ValidateInvalid]),
            MockStore::new(Some(pro_license_with_grace(stale_time, grace_until))),
            MockUsageDb::new(0),
        );

        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro);
    }

    #[tokio::test]
    async fn verify_should_downgrade_past_grace_period() {
        let now = chrono::Utc::now().timestamp();
        let stale_time = now - 15 * 86400;
        // Grace period already expired
        let grace_until = now - 1 * 86400;
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ValidateInvalid]),
            MockStore::new(Some(pro_license_with_grace(stale_time, grace_until))),
            MockUsageDb::new(0),
        );

        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Free);
        assert!(mgr.license_info().is_none());
    }

    #[tokio::test]
    async fn verify_should_keep_pro_on_recent_offline() {
        let now = chrono::Utc::now().timestamp();
        // Verified 10 days ago, offline now — within 30-day offline grace
        let stale_time = now - 10 * 86400;
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ValidateErr]), // network error
            MockStore::new(Some(pro_license(stale_time))),
            MockUsageDb::new(0),
        );

        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Pro);
    }

    #[tokio::test]
    async fn verify_should_downgrade_on_extended_offline() {
        let now = chrono::Utc::now().timestamp();
        // Verified 31 days ago, offline — past 30-day offline grace
        let stale_time = now - 31 * 86400;
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ValidateErr]), // network error
            MockStore::new(Some(pro_license(stale_time))),
            MockUsageDb::new(0),
        );

        let tier = mgr.verify_if_needed().await.unwrap();
        assert_eq!(tier, LicenseTier::Free);
        assert!(mgr.license_info().is_none());
    }

    // =======================================================================
    // Full lifecycle integration tests
    // =======================================================================

    #[tokio::test]
    async fn should_activate_then_exhaust_free_quota_after_deactivate() {
        // 1. Start as Free
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![
                VerifyBehavior::ActivateOk,
                VerifyBehavior::DeactivateOk,
            ]),
            MockStore::new(None),
            MockUsageDb::new(0),
        );
        assert_eq!(mgr.current_tier(), LicenseTier::Free);

        // 2. Activate → Pro
        mgr.activate("KEY-123").await.unwrap();
        assert_eq!(mgr.current_tier(), LicenseTier::Pro);

        // 3. Record usage as Pro → unlimited
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Unlimited);

        // 4. Deactivate → back to Free
        mgr.deactivate().await.unwrap();
        assert_eq!(mgr.current_tier(), LicenseTier::Free);

        // 5. Pro record_usage returns Unlimited without incrementing the DB,
        // so count is still 0. Now as Free, record 14 times to reach count 14.
        // After each call i (0-based): count = i + 1, remaining = 15 - (i + 1) = 14 - i
        // Warning when remaining <= 3, i.e. 14 - i <= 3, i.e. i >= 11
        for i in 0..14 {
            let status = mgr.record_usage().unwrap();
            let count_after = (i + 1) as u32;
            let remaining = 15 - count_after;
            if remaining > 3 {
                assert!(
                    matches!(status, UsageStatus::Available { .. }),
                    "Expected Available at iteration {i} (count={count_after}), got {status:?}"
                );
            } else {
                assert!(
                    matches!(status, UsageStatus::Warning { .. }),
                    "Expected Warning at iteration {i} (count={count_after}), got {status:?}"
                );
            }
        }

        // Count is now 14. Next record → count 15 → Exhausted
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Exhausted);

        // 6. Further usage should be blocked
        let result = mgr.record_usage();
        assert!(
            matches!(result, Err(AppError::UsageLimitReached)),
            "Expected UsageLimitReached, got {result:?}"
        );
    }

    #[tokio::test]
    async fn should_exhaust_free_quota_then_activate_pro_for_unlimited() {
        // 1. Start as Free, already at limit
        let mgr = LicenseManager::new(
            MockVerifier::new(vec![VerifyBehavior::ActivateOk]),
            MockStore::new(None),
            MockUsageDb::new(15),
        );

        // 2. Verify exhausted
        let status = mgr.check_access().await;
        assert_eq!(status, UsageStatus::Exhausted);

        // 3. Recording should fail
        let result = mgr.record_usage();
        assert!(matches!(result, Err(AppError::UsageLimitReached)));

        // 4. Activate Pro → should unlock
        mgr.activate("KEY-PRO").await.unwrap();
        assert_eq!(mgr.current_tier(), LicenseTier::Pro);

        // 5. Now usage should be unlimited even though count is 15
        let status = mgr.record_usage().unwrap();
        assert_eq!(status, UsageStatus::Unlimited);

        let status = mgr.check_access().await;
        assert_eq!(status, UsageStatus::Unlimited);
    }
}
