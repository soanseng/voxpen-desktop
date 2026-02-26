use serde::Deserialize;

use crate::error::AppError;

/// Default LemonSqueezy API base URL.
const LEMONSQUEEZY_BASE_URL: &str = "https://api.lemonsqueezy.com";

/// HTTP client for the LemonSqueezy license API.
///
/// All endpoints use form-encoded POST bodies and return JSON responses.
pub struct LemonSqueezyClient {
    base_url: String,
    client: reqwest::Client,
}

impl Default for LemonSqueezyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LemonSqueezyClient {
    /// Create a client pointing at the production LemonSqueezy API.
    pub fn new() -> Self {
        Self {
            base_url: LEMONSQUEEZY_BASE_URL.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Create a client with a custom base URL (for wiremock testing).
    pub fn new_with_base_url(base: &str) -> Self {
        Self {
            base_url: base.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Activate a license key on this device.
    ///
    /// POST /v1/licenses/activate  (form-encoded body)
    pub async fn activate(
        &self,
        key: &str,
        instance_name: &str,
    ) -> Result<LsLicenseResponse, AppError> {
        let url = format!("{}/v1/licenses/activate", self.base_url);

        let response = self
            .client
            .post(&url)
            .form(&[
                ("license_key", key),
                ("instance_name", instance_name),
            ])
            .send()
            .await
            .map_err(|e| AppError::License(format!("network error: {e}")))?;

        let body = response
            .text()
            .await
            .map_err(|e| AppError::License(format!("failed to read response: {e}")))?;

        let parsed: LsLicenseResponse = serde_json::from_str(&body)
            .map_err(|e| AppError::License(format!("failed to parse response: {e}")))?;

        if !parsed.valid {
            let msg = parsed
                .error
                .clone()
                .unwrap_or_else(|| "activation failed".to_string());
            return Err(AppError::License(msg));
        }

        Ok(parsed)
    }

    /// Validate an existing license activation.
    ///
    /// POST /v1/licenses/validate  (form-encoded body)
    pub async fn validate(
        &self,
        key: &str,
        instance_id: &str,
    ) -> Result<LsLicenseResponse, AppError> {
        let url = format!("{}/v1/licenses/validate", self.base_url);

        let response = self
            .client
            .post(&url)
            .form(&[
                ("license_key", key),
                ("instance_id", instance_id),
            ])
            .send()
            .await
            .map_err(|e| AppError::License(format!("network error: {e}")))?;

        let body = response
            .text()
            .await
            .map_err(|e| AppError::License(format!("failed to read response: {e}")))?;

        let parsed: LsLicenseResponse = serde_json::from_str(&body)
            .map_err(|e| AppError::License(format!("failed to parse response: {e}")))?;

        Ok(parsed)
    }

    /// Deactivate a license from this device.
    ///
    /// POST /v1/licenses/deactivate  (form-encoded body)
    pub async fn deactivate(&self, key: &str, instance_id: &str) -> Result<(), AppError> {
        let url = format!("{}/v1/licenses/deactivate", self.base_url);

        let response = self
            .client
            .post(&url)
            .form(&[
                ("license_key", key),
                ("instance_id", instance_id),
            ])
            .send()
            .await
            .map_err(|e| AppError::License(format!("network error: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::License(format!(
                "deactivation failed (HTTP {}): {}",
                status.as_u16(),
                body
            )));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// LemonSqueezy API response types
// ---------------------------------------------------------------------------

/// Top-level response from LemonSqueezy license endpoints.
///
/// The activate endpoint returns `activated`, while the validate endpoint
/// returns `valid`. We accept both via `#[serde(alias)]`.
#[derive(Debug, Clone, Deserialize)]
pub struct LsLicenseResponse {
    #[serde(alias = "activated")]
    pub valid: bool,
    pub error: Option<String>,
    pub license_key: Option<LsLicenseKey>,
    pub instance: Option<LsInstance>,
    pub meta: Option<LsMeta>,
}

/// License key details from LemonSqueezy.
#[derive(Debug, Clone, Deserialize)]
pub struct LsLicenseKey {
    pub id: Option<u64>,
    pub status: Option<String>,
    pub key: Option<String>,
    pub activation_limit: Option<u32>,
    pub activation_usage: Option<u32>,
}

/// Activation instance from LemonSqueezy.
#[derive(Debug, Clone, Deserialize)]
pub struct LsInstance {
    pub id: Option<String>,
    pub name: Option<String>,
}

/// Meta information from LemonSqueezy (e.g., product/variant info).
#[derive(Debug, Clone, Deserialize)]
pub struct LsMeta {
    pub store_id: Option<u64>,
    pub product_id: Option<u64>,
    pub variant_id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Real LemonSqueezy activate endpoint returns `activated`, not `valid`.
    fn activation_success_json() -> serde_json::Value {
        serde_json::json!({
            "activated": true,
            "error": null,
            "license_key": {
                "id": 1234,
                "status": "active",
                "key": "AAAA-BBBB-CCCC-DDDD",
                "activation_limit": 3,
                "activation_usage": 1
            },
            "instance": {
                "id": "inst-001",
                "name": "My Desktop"
            },
            "meta": {
                "store_id": 100,
                "product_id": 200,
                "variant_id": 300
            }
        })
    }

    fn activation_invalid_json() -> serde_json::Value {
        serde_json::json!({
            "activated": false,
            "error": "The license key is invalid.",
            "license_key": null,
            "instance": null,
            "meta": null
        })
    }

    fn validate_success_json() -> serde_json::Value {
        serde_json::json!({
            "valid": true,
            "error": null,
            "license_key": {
                "id": 1234,
                "status": "active",
                "key": "AAAA-BBBB-CCCC-DDDD",
                "activation_limit": 3,
                "activation_usage": 1
            },
            "instance": {
                "id": "inst-001",
                "name": "My Desktop"
            },
            "meta": null
        })
    }

    #[tokio::test]
    async fn should_activate_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/activate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(activation_success_json()),
            )
            .mount(&server)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&server.uri());
        let result = client.activate("AAAA-BBBB-CCCC-DDDD", "My Desktop").await;

        let resp = result.unwrap();
        assert!(resp.valid);
        assert!(resp.error.is_none());

        let key = resp.license_key.unwrap();
        assert_eq!(key.id, Some(1234));
        assert_eq!(key.status.as_deref(), Some("active"));
        assert_eq!(key.activation_limit, Some(3));

        let instance = resp.instance.unwrap();
        assert_eq!(instance.id.as_deref(), Some("inst-001"));
        assert_eq!(instance.name.as_deref(), Some("My Desktop"));
    }

    #[tokio::test]
    async fn should_return_error_on_invalid_activation_key() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/activate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(activation_invalid_json()),
            )
            .mount(&server)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&server.uri());
        let result = client.activate("INVALID-KEY", "My Desktop").await;

        match result {
            Err(AppError::License(msg)) => {
                assert!(msg.contains("invalid"), "expected 'invalid' in: {msg}");
            }
            other => panic!("expected License error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn should_validate_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/validate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(validate_success_json()),
            )
            .mount(&server)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&server.uri());
        let result = client
            .validate("AAAA-BBBB-CCCC-DDDD", "inst-001")
            .await;

        let resp = result.unwrap();
        assert!(resp.valid);
    }

    #[tokio::test]
    async fn should_return_error_on_network_failure() {
        // Connect to a port that is not listening
        let client = LemonSqueezyClient::new_with_base_url("http://127.0.0.1:1");
        let result = client.activate("key", "name").await;

        assert!(matches!(result, Err(AppError::License(_))));
    }

    #[tokio::test]
    async fn should_deactivate_successfully() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/deactivate"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "deactivated": true
                })),
            )
            .mount(&server)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&server.uri());
        let result = client.deactivate("AAAA-BBBB-CCCC-DDDD", "inst-001").await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn should_return_error_on_deactivate_failure() {
        let server = MockServer::start().await;

        Mock::given(method("POST"))
            .and(path("/v1/licenses/deactivate"))
            .respond_with(
                ResponseTemplate::new(400).set_body_string("Bad request"),
            )
            .mount(&server)
            .await;

        let client = LemonSqueezyClient::new_with_base_url(&server.uri());
        let result = client.deactivate("key", "inst").await;

        match result {
            Err(AppError::License(msg)) => {
                assert!(msg.contains("400"), "expected HTTP 400 in: {msg}");
            }
            other => panic!("expected License error, got {:?}", other),
        }
    }

    #[test]
    fn should_deserialize_response_with_activated_field() {
        let json = r#"{"activated":true,"error":null,"license_key":{"id":1,"status":"active","key":"K","activation_limit":3,"activation_usage":1},"instance":{"id":"i","name":"n"},"meta":{"store_id":1,"product_id":2,"variant_id":3}}"#;
        let resp: LsLicenseResponse = serde_json::from_str(json).unwrap();
        assert!(resp.valid);
    }

    #[test]
    fn should_deserialize_response_with_valid_field() {
        let json = r#"{"valid":true,"error":null,"license_key":{"id":1,"status":"active","key":"K","activation_limit":3,"activation_usage":1},"instance":{"id":"i","name":"n"},"meta":{"store_id":1,"product_id":2,"variant_id":3}}"#;
        let resp: LsLicenseResponse = serde_json::from_str(json).unwrap();
        assert!(resp.valid);
    }

    #[test]
    fn should_ignore_unknown_fields_in_response() {
        let json = r#"{"activated":true,"error":null,"license_key":{"id":1,"status":"active","key":"K","activation_limit":3,"activation_usage":1,"created_at":"2026-01-01","expires_at":null},"instance":{"id":"i","name":"n","created_at":"2026-01-01"},"meta":{"store_id":1,"order_id":99,"order_item_id":88,"product_id":2,"product_name":"VoxPen Pro","variant_id":3,"variant_name":"Default","customer_id":77,"customer_name":"Test","customer_email":"test@example.com"}}"#;
        let resp: LsLicenseResponse = serde_json::from_str(json).unwrap();
        assert!(resp.valid);
        assert_eq!(resp.meta.unwrap().product_id, Some(2));
    }
}
