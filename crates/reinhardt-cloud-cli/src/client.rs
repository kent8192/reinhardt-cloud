//! HTTP client for the reinhardt-cloud REST API.

use std::time::Duration;

use reqwest::{Client, Url};
use thiserror::Error;

/// Errors from API client operations.
// allow(dead_code): Part of the CLI client API scaffold; will be used
// when deploy/status/login commands call the REST API.
#[allow(dead_code)]
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
	// allow(dead_code): Fields are read by methods below; warnings appear
	// because the methods themselves are not yet called from commands.
	#[allow(dead_code)]
	http: Client,
	#[allow(dead_code)]
	base_url: Url,
	#[allow(dead_code)]
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
	// allow(dead_code): Will be called once login command stores a JWT token.
	#[allow(dead_code)]
	pub(crate) fn with_token(mut self, token: String) -> Self {
		self.token = Some(token);
		self
	}

	/// Returns the base URL as a string (without trailing slash).
	// allow(dead_code): Used in tests; will be used by commands for URL display.
	#[allow(dead_code)]
	pub(crate) fn base_url(&self) -> &str {
		self.base_url.as_str().trim_end_matches('/')
	}

	/// Builds an authenticated request to the given API path.
	///
	/// The `path` is joined onto the base URL using [`Url::join`], which
	/// handles leading slashes and relative segments correctly.
	// allow(dead_code): Will be called by deploy/status/login command implementations.
	#[allow(dead_code)]
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
}
