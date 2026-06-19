//! Control-plane HTTP client for CLI commands.
//!
//! Carries the target base URL, an optional bearer API token, and a
//! `reqwest::Client` for issuing authenticated requests to the dashboard
//! control plane. Token-path endpoints (e.g. `GET /api/auth/me/`) are the
//! foundation for `login` and the future deploy relay (#703).

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;
use url::Url;

/// Errors from control-plane requests.
#[derive(Debug, Error)]
pub(crate) enum ClientError {
	#[error("invalid URL: {0}")]
	InvalidUrl(#[from] url::ParseError),
	#[error("not logged in; run `reinhardt-cloud login` or set REINHARDT_CLOUD_API_TOKEN")]
	NoToken,
	#[error("token is invalid, expired, or revoked; generate a new one from the dashboard")]
	Unauthorized,
	#[error("forbidden")]
	Forbidden,
	#[error("not found")]
	NotFound,
	#[error("conflict: {0}")]
	Conflict(String),
	#[error("service unavailable: {0}")]
	ServiceUnavailable(String),
	#[error("control plane returned {0}: {1}")]
	Server(u16, String),
	#[error("cannot reach control plane: {0}")]
	Network(#[from] reqwest::Error),
	#[error("failed to decode response: {0}")]
	Decode(#[from] serde_json::Error),
}

/// User info returned by the control plane after token verification.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct UserInfo {
	pub id: String,
	pub username: String,
}

#[allow(dead_code)] // Constructed by the follow-up #703 deploy command wiring.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CliDeployRequest {
	pub project_name: String,
	pub cluster: String,
	pub namespace: String,
	pub image: String,
	pub project_yaml: String,
}

#[allow(dead_code)] // Decoded by the follow-up #703 deploy command wiring.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(crate) struct CliDeployResponse {
	pub deployment_id: i64,
	pub project_name: String,
	pub cluster: String,
	pub status: String,
	pub image: String,
}

#[derive(Debug, Deserialize)]
struct ApiErrorBody {
	error: String,
}

/// Target endpoint metadata for the Reinhardt Cloud platform.
#[derive(Debug, Clone)]
pub(crate) struct ReinhardtCloudClient {
	base_url: Url,
	token: Option<String>,
	http: reqwest::Client,
}

impl ReinhardtCloudClient {
	/// Creates a new control-plane target with the given base URL.
	///
	/// # Errors
	///
	/// Returns [`ClientError::InvalidUrl`] if `base_url` is not a valid URL.
	pub(crate) fn new(base_url: &str) -> Result<Self, ClientError> {
		Ok(Self {
			base_url: Url::parse(base_url)?,
			token: None,
			http: reqwest::Client::new(),
		})
	}

	/// Stores the authentication token for commands that need it.
	pub(crate) fn with_token(mut self, token: String) -> Self {
		self.token = Some(token);
		self
	}

	/// Returns the base URL as a string (without trailing slash).
	pub(crate) fn base_url(&self) -> &str {
		self.base_url.as_str().trim_end_matches('/')
	}

	/// Current bearer token, if set.
	pub(crate) fn token(&self) -> Option<&str> {
		self.token.as_deref()
	}

	fn require_token(&self) -> Result<&str, ClientError> {
		self.token.as_deref().ok_or(ClientError::NoToken)
	}

	/// Verify the token and resolve the user. `GET /api/auth/me/`.
	///
	/// # Errors
	///
	/// Returns [`ClientError::NoToken`] when no token is configured,
	/// [`ClientError::Unauthorized`] on a 401 (invalid / expired / revoked
	/// token), and [`ClientError::Network`] when the control plane is
	/// unreachable.
	pub(crate) async fn me(&self) -> Result<UserInfo, ClientError> {
		let resp = self
			.http
			.get(format!("{}/api/auth/me/", self.base_url()))
			.bearer_auth(self.require_token()?)
			.send()
			.await?;
		Self::decode(resp).await
	}

	/// Submit a CLI-generated Project manifest to the Dashboard control plane.
	#[allow(dead_code)] // Called by the follow-up #703 deploy command wiring.
	pub(crate) async fn submit_deploy(
		&self,
		request: &CliDeployRequest,
	) -> Result<CliDeployResponse, ClientError> {
		self.post("/api/deployments/cli/", request).await
	}

	/// Reusable POST helper — foundation for the #703 deploy endpoint.
	#[allow(dead_code)] // Exercised by the #703 deploy relay (CLI -> Dashboard POST).
	pub(crate) async fn post<T, R>(&self, path: &str, body: &T) -> Result<R, ClientError>
	where
		T: Serialize,
		R: DeserializeOwned,
	{
		let resp = self
			.http
			.post(format!("{}{path}", self.base_url()))
			.bearer_auth(self.require_token()?)
			.json(body)
			.send()
			.await?;
		Self::decode(resp).await
	}

	/// Decode a JSON body on success, or map the status code to an error.
	async fn decode<R: DeserializeOwned>(resp: reqwest::Response) -> Result<R, ClientError> {
		let status = resp.status();
		if status.is_success() {
			return Ok(serde_json::from_str(&resp.text().await?)?);
		}
		let body = resp.text().await.unwrap_or_default();
		let message = error_message_from_body(&body);
		Err(match status.as_u16() {
			401 => ClientError::Unauthorized,
			403 => ClientError::Forbidden,
			404 => ClientError::NotFound,
			409 => ClientError::Conflict(message),
			503 => ClientError::ServiceUnavailable(message),
			code => ClientError::Server(code, message),
		})
	}
}

fn error_message_from_body(body: &str) -> String {
	serde_json::from_str::<ApiErrorBody>(body)
		.map(|parsed| parsed.error)
		.unwrap_or_else(|_| body.to_string())
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
		// Arrange & Act
		let client = ReinhardtCloudClient::new("http://localhost:8000")
			.unwrap()
			.with_token("my-secret-token".to_string());

		// Assert
		assert_eq!(client.token(), Some("my-secret-token"));
	}

	#[rstest]
	fn test_new_client_has_no_token() {
		// Arrange & Act
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Assert
		assert!(client.token().is_none());
	}

	#[rstest]
	#[tokio::test]
	async fn test_me_sends_bearer_and_decodes_username() {
		// Arrange
		let server = wiremock::MockServer::start().await;
		wiremock::Mock::given(wiremock::matchers::method("GET"))
			.and(wiremock::matchers::path("/api/auth/me/"))
			.and(wiremock::matchers::header(
				"Authorization",
				"Bearer rct_abc",
			))
			.respond_with(
				wiremock::ResponseTemplate::new(200)
					.set_body_json(serde_json::json!({ "id": "u-1", "username": "alice" })),
			)
			.mount(&server)
			.await;
		let client = ReinhardtCloudClient::new(&server.uri())
			.unwrap()
			.with_token("rct_abc".to_string());

		// Act
		let info = client.me().await.unwrap();

		// Assert
		assert_eq!(info.username, "alice");
		assert_eq!(info.id, "u-1");
	}

	#[rstest]
	#[tokio::test]
	async fn test_me_maps_401_to_unauthorized() {
		// Arrange
		let server = wiremock::MockServer::start().await;
		wiremock::Mock::given(wiremock::matchers::method("GET"))
			.respond_with(wiremock::ResponseTemplate::new(401))
			.mount(&server)
			.await;
		let client = ReinhardtCloudClient::new(&server.uri())
			.unwrap()
			.with_token("bad".to_string());

		// Act
		let result = client.me().await;

		// Assert
		assert!(matches!(result, Err(ClientError::Unauthorized)));
	}

	#[rstest]
	#[tokio::test]
	async fn test_me_without_token_is_no_token_error() {
		// Arrange — no token configured
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();

		// Act
		let result = client.me().await;

		// Assert
		assert!(matches!(result, Err(ClientError::NoToken)));
	}

	#[rstest]
	fn test_cli_deploy_request_serializes_expected_fields() {
		// Arrange
		let project_yaml = "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n";
		let request = CliDeployRequest {
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			namespace: "default".to_string(),
			image: "demo:latest".to_string(),
			project_yaml: project_yaml.to_string(),
		};

		// Act
		let json = serde_json::to_value(&request).unwrap();

		// Assert
		assert_eq!(json["project_name"], "demo");
		assert_eq!(json["cluster"], "prod");
		assert_eq!(json["namespace"], "default");
		assert_eq!(json["image"], "demo:latest");
		assert_eq!(json["project_yaml"], project_yaml);
	}

	#[rstest]
	#[tokio::test]
	async fn test_submit_deploy_posts_bearer_json_and_decodes_response() {
		// Arrange
		let server = wiremock::MockServer::start().await;
		let project_yaml = "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n";
		wiremock::Mock::given(wiremock::matchers::method("POST"))
			.and(wiremock::matchers::path("/api/deployments/cli/"))
			.and(wiremock::matchers::header(
				"Authorization",
				"Bearer rct_valid",
			))
			.and(wiremock::matchers::body_json(serde_json::json!({
				"project_name": "demo",
				"cluster": "prod",
				"namespace": "default",
				"image": "demo:latest",
				"project_yaml": project_yaml
			})))
			.respond_with(
				wiremock::ResponseTemplate::new(202).set_body_json(serde_json::json!({
					"deployment_id": 7,
					"project_name": "demo",
					"cluster": "prod",
					"status": "pending",
					"image": "demo:latest"
				})),
			)
			.mount(&server)
			.await;
		let client = ReinhardtCloudClient::new(&server.uri())
			.unwrap()
			.with_token("rct_valid".to_string());
		let request = CliDeployRequest {
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			namespace: "default".to_string(),
			image: "demo:latest".to_string(),
			project_yaml: project_yaml.to_string(),
		};

		// Act
		let response = client.submit_deploy(&request).await.unwrap();

		// Assert
		assert_eq!(response.deployment_id, 7);
		assert_eq!(response.project_name, "demo");
		assert_eq!(response.cluster, "prod");
		assert_eq!(response.status, "pending");
	}

	#[rstest]
	#[tokio::test]
	async fn test_submit_deploy_without_token_is_no_token_error() {
		// Arrange
		let client = ReinhardtCloudClient::new("http://localhost:8000").unwrap();
		let request = CliDeployRequest {
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			namespace: "default".to_string(),
			image: "demo:latest".to_string(),
			project_yaml: "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n"
				.to_string(),
		};

		// Act
		let result = client.submit_deploy(&request).await;

		// Assert
		assert!(matches!(result, Err(ClientError::NoToken)));
	}

	#[rstest]
	#[tokio::test]
	async fn test_submit_deploy_maps_503_error_body() {
		// Arrange
		let server = wiremock::MockServer::start().await;
		wiremock::Mock::given(wiremock::matchers::method("POST"))
			.and(wiremock::matchers::path("/api/deployments/cli/"))
			.respond_with(wiremock::ResponseTemplate::new(503).set_body_json(
				serde_json::json!({ "error": "No agent connected for cluster prod" }),
			))
			.mount(&server)
			.await;
		let client = ReinhardtCloudClient::new(&server.uri())
			.unwrap()
			.with_token("rct_valid".to_string());
		let request = CliDeployRequest {
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			namespace: "default".to_string(),
			image: "demo:latest".to_string(),
			project_yaml: "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n"
				.to_string(),
		};

		// Act
		let result = client.submit_deploy(&request).await;

		// Assert
		assert!(matches!(
			result,
			Err(ClientError::ServiceUnavailable(message))
				if message == "No agent connected for cluster prod"
		));
	}

	#[rstest]
	#[tokio::test]
	async fn test_submit_deploy_maps_409_error_body() {
		// Arrange
		let server = wiremock::MockServer::start().await;
		wiremock::Mock::given(wiremock::matchers::method("POST"))
			.and(wiremock::matchers::path("/api/deployments/cli/"))
			.respond_with(wiremock::ResponseTemplate::new(409).set_body_json(
				serde_json::json!({ "error": "Deployment already exists for project demo" }),
			))
			.mount(&server)
			.await;
		let client = ReinhardtCloudClient::new(&server.uri())
			.unwrap()
			.with_token("rct_valid".to_string());
		let request = CliDeployRequest {
			project_name: "demo".to_string(),
			cluster: "prod".to_string(),
			namespace: "default".to_string(),
			image: "demo:latest".to_string(),
			project_yaml: "apiVersion: paas.reinhardt-cloud.dev/v1alpha2\nkind: Project\n"
				.to_string(),
		};

		// Act
		let result = client.submit_deploy(&request).await;

		// Assert
		assert!(matches!(
			result,
			Err(ClientError::Conflict(message))
				if message == "Deployment already exists for project demo"
		));
	}
}
