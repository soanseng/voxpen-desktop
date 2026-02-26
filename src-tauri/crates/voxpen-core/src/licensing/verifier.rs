use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::error::AppError;
use crate::licensing::lemonsqueezy::{LemonSqueezyClient, LsLicenseResponse};

/// Trait abstracting license verification operations.
///
/// Allows swapping between direct LemonSqueezy API calls (v1) and a
/// future proxy-based model without changing the manager logic.
pub trait LicenseVerifier: Send + Sync {
    /// Activate a license key, returning license details on success.
    fn activate(
        &self,
        key: &str,
        instance_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>>;

    /// Validate an existing license activation.
    fn validate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>>;

    /// Deactivate a license from this device.
    fn deactivate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>>;
}

/// Direct implementation that calls the LemonSqueezy API.
///
/// Wraps `LemonSqueezyClient` in `Arc` so the boxed futures are `Send`.
pub struct DirectLemonSqueezy {
    client: Arc<LemonSqueezyClient>,
}

impl Default for DirectLemonSqueezy {
    fn default() -> Self {
        Self::new()
    }
}

impl DirectLemonSqueezy {
    pub fn new() -> Self {
        Self {
            client: Arc::new(LemonSqueezyClient::new()),
        }
    }

    /// Create with a custom base URL (for testing).
    pub fn new_with_base_url(base: &str) -> Self {
        Self {
            client: Arc::new(LemonSqueezyClient::new_with_base_url(base)),
        }
    }
}

impl LicenseVerifier for DirectLemonSqueezy {
    fn activate(
        &self,
        key: &str,
        instance_name: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>> {
        let client = Arc::clone(&self.client);
        let key = key.to_string();
        let instance_name = instance_name.to_string();
        Box::pin(async move { client.activate(&key, &instance_name).await })
    }

    fn validate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<LsLicenseResponse, AppError>> + Send>> {
        let client = Arc::clone(&self.client);
        let key = key.to_string();
        let instance_id = instance_id.to_string();
        Box::pin(async move { client.validate(&key, &instance_id).await })
    }

    fn deactivate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send>> {
        let client = Arc::clone(&self.client);
        let key = key.to_string();
        let instance_id = instance_id.to_string();
        Box::pin(async move { client.deactivate(&key, &instance_id).await })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn activation_success_json() -> serde_json::Value {
        serde_json::json!({
            "valid": true,
            "error": null,
            "license_key": {
                "id": 1,
                "status": "active",
                "key": "KEY-123",
                "activation_limit": 3,
                "activation_usage": 1
            },
            "instance": {
                "id": "inst-abc",
                "name": "Test Device"
            },
            "meta": null
        })
    }

    fn validate_success_json() -> serde_json::Value {
        serde_json::json!({
            "valid": true,
            "error": null,
            "license_key": {
                "id": 1,
                "status": "active",
                "key": "KEY-123",
                "activation_limit": 3,
                "activation_usage": 1
            },
            "instance": {
                "id": "inst-abc",
                "name": "Test Device"
            },
            "meta": null
        })
    }

    #[tokio::test]
    async fn should_activate_via_verifier_trait() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/activate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(activation_success_json()),
            )
            .mount(&server)
            .await;

        let verifier = DirectLemonSqueezy::new_with_base_url(&server.uri());
        let result = verifier.activate("KEY-123", "Test Device").await;

        let resp = result.unwrap();
        assert!(resp.valid);
        assert_eq!(resp.instance.unwrap().id.as_deref(), Some("inst-abc"));
    }

    #[tokio::test]
    async fn should_validate_via_verifier_trait() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/validate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(validate_success_json()),
            )
            .mount(&server)
            .await;

        let verifier = DirectLemonSqueezy::new_with_base_url(&server.uri());
        let result = verifier.validate("KEY-123", "inst-abc").await;

        assert!(result.unwrap().valid);
    }

    #[tokio::test]
    async fn should_deactivate_via_verifier_trait() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/deactivate"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"deactivated": true})),
            )
            .mount(&server)
            .await;

        let verifier = DirectLemonSqueezy::new_with_base_url(&server.uri());
        let result = verifier.deactivate("KEY-123", "inst-abc").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn should_be_usable_as_dyn_trait() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/validate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(validate_success_json()),
            )
            .mount(&server)
            .await;

        let verifier: Box<dyn LicenseVerifier> =
            Box::new(DirectLemonSqueezy::new_with_base_url(&server.uri()));
        let result = verifier.validate("KEY-123", "inst-abc").await;

        assert!(result.unwrap().valid);
    }
}
