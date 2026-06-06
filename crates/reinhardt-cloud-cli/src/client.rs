//! Client boundary for the reinhardt-cloud control plane.

use reqwest::Url;
use thiserror::Error;

/// Errors from API client operations.
#[derive(Debug, Error)]
pub(crate) enum ClientError {
	#[error("invalid URL: {0}")]
	InvalidUrl(#[from] url::ParseError),

	#[error("invalid cluster id '{value}': expected a positive 64-bit integer")]
	InvalidClusterId { value: String },

	#[error("dashboard REST operation '{operation}' is no longer exposed by the Pages app")]
	UnsupportedDashboardRestOperation { operation: &'static str },
}

/// API client for the Reinhardt Cloud platform.
#[derive(Debug, Clone)]
pub(crate) struct ReinhardtCloudClient {
	base_url: Url,
	token: Option<String>,
}

impl ReinhardtCloudClient {
	/// Creates a new API client with the given base URL.
	///
	/// # Errors
	///
	/// Returns [`ClientError::InvalidUrl`] if `base_url` is not a valid URL.
	pub(crate) fn new(base_url: &str) -> Result<Self, ClientError> {
		let parsed = Url::parse(base_url)?;
		Ok(Self {
			base_url: parsed,
			token: None,
		})
	}

	/// Sets the authentication token.
	pub(crate) fn with_token(mut self, token: String) -> Self {
		self.token = Some(token);
		self
	}

	/// Returns the base URL as a string (without trailing slash).
	pub(crate) fn base_url(&self) -> &str {
		self.base_url.as_str().trim_end_matches('/')
	}

	/// Deploys an application by sending JSON to the dashboard API.
	///
	/// The dashboard create-deployment endpoint expects a JSON body with
	/// `app_name`, `image`, optionally `cluster_id`, and optionally the
	/// generated `ReinhardtApp` manifest YAML.
	///
	/// Returns the response body on success.
	pub(crate) async fn deploy(
		&self,
		app_name: &str,
		image: &str,
		cluster_id: Option<&str>,
		reinhardt_app_yaml: Option<&str>,
	) -> Result<String, ClientError> {
		let _payload = build_deploy_payload(app_name, image, cluster_id, reinhardt_app_yaml)?;
		Err(ClientError::UnsupportedDashboardRestOperation {
			operation: "deploy",
		})
	}

	/// Queries deployment status from the dashboard API.
	///
	/// The dashboard list endpoint returns a paginated list. This method
	/// fetches the list and filters client-side by `app_name`, returning
	/// the matching deployment entry or an error when not found.
	pub(crate) async fn get_status(
		&self,
		app_name: &str,
	) -> Result<serde_json::Value, ClientError> {
		let _ = app_name;
		Err(ClientError::UnsupportedDashboardRestOperation {
			operation: "status",
		})
	}

	/// Authenticates with the control-plane API and returns a JWT token.
	pub(crate) async fn login(
		&self,
		username: &str,
		password: &str,
	) -> Result<String, ClientError> {
		let _ = (username, password);
		Err(ClientError::UnsupportedDashboardRestOperation { operation: "login" })
	}
}

fn build_deploy_payload(
	app_name: &str,
	image: &str,
	cluster_id: Option<&str>,
	reinhardt_app_yaml: Option<&str>,
) -> Result<serde_json::Value, ClientError> {
	let mut payload = serde_json::json!({
		"app_name": app_name,
		"image": image,
	});
	if let Some(cid) = cluster_id {
		let parsed = cid
			.parse::<i64>()
			.ok()
			.filter(|value| *value > 0)
			.ok_or_else(|| ClientError::InvalidClusterId {
				value: cid.to_string(),
			})?;
		payload["cluster_id"] = serde_json::json!(parsed);
	}
	if let Some(manifest) = reinhardt_app_yaml {
		payload["reinhardt_app_yaml"] = serde_json::Value::String(manifest.to_string());
	}

	Ok(payload)
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_new_client_stores_base_url() {
		// Arrange & Act
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Assert
		assert_eq!(client.base_url(), "http://localhost:8000");
	}

	#[rstest]
	fn test_new_client_trims_trailing_slash() {
		// Arrange & Act
		let client = ReinhardtCloudClient::new("http://localhost:8000/").unwrap();

		// Assert
		assert_eq!(client.base_url(), "http://localhost:8000");
	}

	#[rstest]
	fn test_new_client_invalid_url_returns_error() {
		// Arrange
		let invalid_url = "not a url";

		// Act
		let result = ReinhardtCloudClient::new(invalid_url);

		// Assert
		assert!(
			matches!(result, Err(ClientError::InvalidUrl(_))),
			"expected ClientError::InvalidUrl, got {result:?}"
		);
	}

	#[rstest]
	fn test_with_token_sets_token() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let client = client.with_token("my-secret-token".to_string());

		// Assert
		assert_eq!(client.token, Some("my-secret-token".to_string()));
	}

	#[rstest]
	fn test_new_client_has_no_token() {
		// Arrange & Act
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Assert
		assert!(client.token.is_none());
	}

	#[rstest]
	#[tokio::test]
	async fn test_deploy_returns_unsupported_dashboard_rest_operation() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let result = client.deploy("web", "web:v1", None, None).await;

		// Assert
		assert!(
			matches!(
				result,
				Err(ClientError::UnsupportedDashboardRestOperation {
					operation: "deploy"
				})
			),
			"expected unsupported deploy error, got {result:?}"
		);
	}

	#[rstest]
	fn test_build_deploy_payload_serializes_cluster_id_as_number() {
		// Arrange
		let manifest = "apiVersion: paas.reinhardt-cloud.dev/v1\nkind: ReinhardtApp\n";

		// Act
		let payload = build_deploy_payload("web", "web:v1", Some("42"), Some(manifest)).unwrap();

		// Assert
		assert_eq!(payload["app_name"], serde_json::json!("web"));
		assert_eq!(payload["image"], serde_json::json!("web:v1"));
		assert_eq!(payload["cluster_id"], serde_json::json!(42));
		assert_eq!(payload["reinhardt_app_yaml"], serde_json::json!(manifest));
	}

	#[rstest]
	#[case("abc")]
	#[case("0")]
	#[case("-1")]
	fn test_build_deploy_payload_rejects_invalid_cluster_id(#[case] cluster_id: &str) {
		// Arrange
		let invalid_cluster_id = cluster_id;

		// Act
		let result = build_deploy_payload("web", "web:v1", Some(invalid_cluster_id), None);

		// Assert
		assert!(
			matches!(result, Err(ClientError::InvalidClusterId { .. })),
			"expected invalid cluster id error, got {result:?}"
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_status_returns_unsupported_dashboard_rest_operation() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let result = client.get_status("my-app").await;

		// Assert
		assert!(
			matches!(
				result,
				Err(ClientError::UnsupportedDashboardRestOperation {
					operation: "status"
				})
			),
			"expected unsupported status error, got {result:?}"
		);
	}

	#[rstest]
	#[tokio::test]
	async fn test_login_returns_unsupported_dashboard_rest_operation() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let result = client.login("user", "password").await;

		// Assert
		assert!(
			matches!(
				result,
				Err(ClientError::UnsupportedDashboardRestOperation { operation: "login" })
			),
			"expected unsupported login error, got {result:?}"
		);
	}
}
