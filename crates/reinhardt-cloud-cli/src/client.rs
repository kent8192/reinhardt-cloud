//! HTTP client for the reinhardt-cloud REST API.

use std::time::Duration;

use reqwest::{Client, Url};
use thiserror::Error;

/// Errors from API client operations.
#[derive(Debug, Error)]
pub(crate) enum ClientError {
	#[error("HTTP request failed: {0}")]
	RequestError(#[from] reqwest::Error),

	#[error("API error ({status}): {message}")]
	ApiError { status: u16, message: String },

	#[error("invalid URL: {0}")]
	InvalidUrl(#[from] url::ParseError),
}

/// REST API client for the Reinhardt Cloud platform.
#[derive(Debug, Clone)]
pub(crate) struct ReinhardtCloudClient {
	http: Client,
	base_url: Url,
	token: Option<String>,
}

impl ReinhardtCloudClient {
	/// Creates a new API client with the given base URL.
	///
	/// # Errors
	///
	/// Returns [`ClientError::InvalidUrl`] if `base_url` is not a valid URL.
	/// Returns [`ClientError::RequestError`] if the HTTP client cannot be built.
	pub(crate) fn new(base_url: &str) -> Result<Self, ClientError> {
		let parsed = Url::parse(base_url)?;
		let http = Client::builder()
			.connect_timeout(Duration::from_secs(10))
			.timeout(Duration::from_secs(30))
			.build()?;
		Ok(Self {
			http,
			base_url: parsed,
			token: None,
		})
	}

	/// Sets the authentication token.
	///
	/// Will be called from the main entry point once token persistence is
	/// implemented; currently exercised only from tests.
	#[expect(
		dead_code,
		reason = "used in tests; production use pending token persistence"
	)]
	pub(crate) fn with_token(mut self, token: String) -> Self {
		self.token = Some(token);
		self
	}

	/// Returns the base URL as a string (without trailing slash).
	///
	/// Used in tests; will be used for user-facing URL display once status
	/// and deploy commands print the target server.
	#[expect(
		dead_code,
		reason = "used in tests; production use pending CLI output improvements"
	)]
	pub(crate) fn base_url(&self) -> &str {
		self.base_url.as_str().trim_end_matches('/')
	}

	/// Builds an authenticated request to the given API path.
	///
	/// The `path` is joined onto the base URL using [`Url::join`], which
	/// handles leading slashes and relative segments correctly.
	pub(crate) fn request(
		&self,
		method: reqwest::Method,
		path: &str,
	) -> Result<reqwest::RequestBuilder, ClientError> {
		let url = self.base_url.join(path)?;
		let mut req = self.http.request(method, url);
		if let Some(ref token) = self.token {
			req = req.bearer_auth(token);
		}
		Ok(req)
	}

	/// Deploys an application by sending CRD YAML to the dashboard API.
	///
	/// Returns the response body on success.
	pub(crate) async fn deploy(&self, crd_yaml: &str) -> Result<String, ClientError> {
		let response = self
			.request(reqwest::Method::POST, "/api/deployments/")?
			.header("Content-Type", "application/x-yaml")
			.body(crd_yaml.to_string())
			.send()
			.await?;

		let status = response.status();
		let body = response.text().await?;

		if status.is_success() {
			Ok(body)
		} else {
			Err(ClientError::ApiError {
				status: status.as_u16(),
				message: body,
			})
		}
	}

	/// Queries deployment status from the dashboard API.
	///
	/// Returns the parsed JSON response for the given application.
	pub(crate) async fn get_status(
		&self,
		app_name: &str,
	) -> Result<serde_json::Value, ClientError> {
		let path = format!("/api/deployments/?app_name={app_name}");
		let response = self.request(reqwest::Method::GET, &path)?.send().await?;

		let status = response.status();
		let body = response.text().await?;

		if status.is_success() {
			let value: serde_json::Value =
				serde_json::from_str(&body).map_err(|e| ClientError::ApiError {
					status: status.as_u16(),
					message: format!("invalid JSON in response: {e}"),
				})?;
			Ok(value)
		} else {
			Err(ClientError::ApiError {
				status: status.as_u16(),
				message: body,
			})
		}
	}

	/// Authenticates with the dashboard API and returns a JWT token.
	pub(crate) async fn login(
		&self,
		username: &str,
		password: &str,
	) -> Result<String, ClientError> {
		let payload = serde_json::json!({
			"username": username,
			"password": password,
		});

		let response = self
			.request(reqwest::Method::POST, "/api/auth/login/")?
			.json(&payload)
			.send()
			.await?;

		let status = response.status();
		let body = response.text().await?;

		if status.is_success() {
			let value: serde_json::Value =
				serde_json::from_str(&body).map_err(|e| ClientError::ApiError {
					status: status.as_u16(),
					message: format!("invalid JSON in response: {e}"),
				})?;
			let token = value["token"]
				.as_str()
				.ok_or_else(|| ClientError::ApiError {
					status: status.as_u16(),
					message: "response missing 'token' field".to_string(),
				})?;
			Ok(token.to_string())
		} else {
			Err(ClientError::ApiError {
				status: status.as_u16(),
				message: body,
			})
		}
	}
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
	fn test_request_joins_path_correctly() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let result = client.request(reqwest::Method::GET, "/api/v1/apps");

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_request_includes_bearer_token_when_set() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000")
			.unwrap()
			.with_token("test-jwt".to_string());

		// Act — build the request and inspect it
		let req = client
			.request(reqwest::Method::GET, "/api/deployments/")
			.unwrap()
			.build()
			.unwrap();

		// Assert — the Authorization header should carry the bearer token
		let auth = req
			.headers()
			.get("authorization")
			.unwrap()
			.to_str()
			.unwrap();
		assert_eq!(auth, "Bearer test-jwt");
	}

	#[rstest]
	fn test_request_omits_bearer_token_when_none() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let req = client
			.request(reqwest::Method::GET, "/api/deployments/")
			.unwrap()
			.build()
			.unwrap();

		// Assert
		assert!(req.headers().get("authorization").is_none());
	}

	#[rstest]
	fn test_deploy_url_construction() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act — verify the request builder resolves to the right URL
		let req = client
			.request(reqwest::Method::POST, "/api/deployments/")
			.unwrap()
			.build()
			.unwrap();

		// Assert
		assert_eq!(req.url().as_str(), "http://localhost:8000/api/deployments/");
		assert_eq!(req.method(), reqwest::Method::POST);
	}

	#[rstest]
	fn test_status_url_construction() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let path = format!("/api/deployments/?app_name={}", "my-app");
		let req = client
			.request(reqwest::Method::GET, &path)
			.unwrap()
			.build()
			.unwrap();

		// Assert
		assert_eq!(
			req.url().as_str(),
			"http://localhost:8000/api/deployments/?app_name=my-app"
		);
		assert_eq!(req.method(), reqwest::Method::GET);
	}

	#[rstest]
	fn test_login_url_construction() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let req = client
			.request(reqwest::Method::POST, "/api/auth/login/")
			.unwrap()
			.build()
			.unwrap();

		// Assert
		assert_eq!(req.url().as_str(), "http://localhost:8000/api/auth/login/");
		assert_eq!(req.method(), reqwest::Method::POST);
	}
}
