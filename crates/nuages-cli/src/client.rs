//! HTTP client for the nuages REST API.

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

/// REST API client for the nuages platform.
#[derive(Debug, Clone)]
pub(crate) struct NuagesClient {
	http: Client,
	base_url: Url,
	token: Option<String>,
}

impl NuagesClient {
	/// Creates a new API client with the given base URL.
	///
	/// # Panics
	///
	/// Panics if `base_url` is not a valid URL.
	pub(crate) fn new(base_url: &str) -> Self {
		let parsed = Url::parse(base_url).expect("base_url must be a valid URL");
		Self {
			http: Client::new(),
			base_url: parsed,
			token: None,
		}
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
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_new_client_stores_base_url() {
		// Arrange & Act
		let client = NuagesClient::new("http://localhost:8000");

		// Assert
		assert_eq!(client.base_url(), "http://localhost:8000");
	}

	#[rstest]
	fn test_new_client_trims_trailing_slash() {
		// Arrange & Act
		let client = NuagesClient::new("http://localhost:8000/");

		// Assert
		assert_eq!(client.base_url(), "http://localhost:8000");
	}

	#[rstest]
	fn test_with_token_sets_token() {
		// Arrange
		let client = NuagesClient::new("http://localhost:8000");

		// Act
		let client = client.with_token("my-secret-token".to_string());

		// Assert
		assert_eq!(client.token, Some("my-secret-token".to_string()));
	}

	#[rstest]
	fn test_new_client_has_no_token() {
		// Arrange & Act
		let client = NuagesClient::new("http://localhost:8000");

		// Assert
		assert!(client.token.is_none());
	}

	#[rstest]
	fn test_request_joins_path_correctly() {
		// Arrange
		let client = NuagesClient::new("http://localhost:8000");

		// Act
		let result = client.request(reqwest::Method::GET, "/api/v1/apps");

		// Assert
		assert!(result.is_ok());
	}
}
