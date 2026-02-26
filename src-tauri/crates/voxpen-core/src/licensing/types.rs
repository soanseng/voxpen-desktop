use serde::{Deserialize, Serialize};

/// License tier: Free (15 transcriptions/day) or Pro (unlimited).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LicenseTier {
    Free,
    Pro,
}

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
}

/// Status of the user's daily usage quota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UsageStatus {
    /// Transcriptions remaining for today.
    Available { remaining: u32 },
    /// Low remaining count (<= WARNING_THRESHOLD).
    Warning { remaining: u32 },
    /// Daily limit reached.
    Exhausted,
    /// Pro tier — no limit.
    Unlimited,
}

/// A single day's usage count record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageRecord {
    pub date: String,
    pub count: u32,
}

/// Maximum free transcriptions per day.
pub const FREE_DAILY_LIMIT: u32 = 15;
/// Remaining count at or below which we show a warning.
pub const WARNING_THRESHOLD: u32 = 3;
/// Days between required license verifications.
pub const VERIFY_INTERVAL_DAYS: i64 = 7;
/// Days of grace after a failed verification before downgrading.
pub const VERIFY_GRACE_DAYS: i64 = 7;
/// Days a license remains valid while offline.
pub const OFFLINE_GRACE_DAYS: i64 = 30;
/// Maximum devices a single license key can activate.
pub const MAX_DEVICE_ACTIVATIONS: u32 = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_roundtrip_license_tier_free() {
        let tier = LicenseTier::Free;
        let json = serde_json::to_string(&tier).unwrap();
        let deserialized: LicenseTier = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, LicenseTier::Free);
    }

    #[test]
    fn should_roundtrip_license_tier_pro() {
        let tier = LicenseTier::Pro;
        let json = serde_json::to_string(&tier).unwrap();
        let deserialized: LicenseTier = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, LicenseTier::Pro);
    }

    #[test]
    fn should_roundtrip_license_info() {
        let info = LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: "abc-123".to_string(),
            instance_id: "inst-456".to_string(),
            licensed_version: 1,
            activated_at: 1700000000,
            last_verified_at: 1700100000,
            verification_grace_until: None,
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: LicenseInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.tier, LicenseTier::Pro);
        assert_eq!(deserialized.license_key, "abc-123");
        assert_eq!(deserialized.instance_id, "inst-456");
        assert_eq!(deserialized.licensed_version, 1);
        assert_eq!(deserialized.activated_at, 1700000000);
        assert_eq!(deserialized.last_verified_at, 1700100000);
        assert!(deserialized.verification_grace_until.is_none());
    }

    #[test]
    fn should_roundtrip_license_info_with_grace() {
        let info = LicenseInfo {
            tier: LicenseTier::Pro,
            license_key: "key".to_string(),
            instance_id: "inst".to_string(),
            licensed_version: 2,
            activated_at: 1700000000,
            last_verified_at: 1700100000,
            verification_grace_until: Some(1700700000),
        };
        let json = serde_json::to_string(&info).unwrap();
        let deserialized: LicenseInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.verification_grace_until, Some(1700700000));
    }

    #[test]
    fn should_roundtrip_usage_status_available() {
        let status = UsageStatus::Available { remaining: 10 };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: UsageStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, UsageStatus::Available { remaining: 10 });
    }

    #[test]
    fn should_roundtrip_usage_status_warning() {
        let status = UsageStatus::Warning { remaining: 2 };
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: UsageStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, UsageStatus::Warning { remaining: 2 });
    }

    #[test]
    fn should_roundtrip_usage_status_exhausted() {
        let status = UsageStatus::Exhausted;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: UsageStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, UsageStatus::Exhausted);
    }

    #[test]
    fn should_roundtrip_usage_status_unlimited() {
        let status = UsageStatus::Unlimited;
        let json = serde_json::to_string(&status).unwrap();
        let deserialized: UsageStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, UsageStatus::Unlimited);
    }

    #[test]
    fn should_roundtrip_usage_record() {
        let record = UsageRecord {
            date: "2026-02-26".to_string(),
            count: 5,
        };
        let json = serde_json::to_string(&record).unwrap();
        let deserialized: UsageRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, record);
    }

    #[test]
    fn should_have_correct_constant_values() {
        assert_eq!(FREE_DAILY_LIMIT, 15);
        assert_eq!(WARNING_THRESHOLD, 3);
        assert_eq!(VERIFY_INTERVAL_DAYS, 7);
        assert_eq!(VERIFY_GRACE_DAYS, 7);
        assert_eq!(OFFLINE_GRACE_DAYS, 30);
        assert_eq!(MAX_DEVICE_ACTIVATIONS, 3);
    }

    #[test]
    fn should_serialize_usage_status_with_tag() {
        let status = UsageStatus::Available { remaining: 5 };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"type\":\"Available\""));
        // serde serializes struct variant content as an object
        assert!(json.contains("\"remaining\":5"), "actual json: {json}");
    }
}
