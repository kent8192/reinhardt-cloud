//! HTTP client for the nuages REST API.

use reqwest::Client;
use thiserror::Error;

/// Errors from API client operations.
#[derive(Debug, Error)]
pub(crate) enum ClientError {
	#[error("HTTP request failed: {0}")]
	RequestError(#[from] reqwest::Error),

	#[error("API error ({status}): {message}")]
	ApiError { status: u16, message: String },
}

/// REST API client for the nuages platform.
#[derive(Debug, Clone)]
pub(crate) struct NuagesClient {
	http: Client,
	base_url: String,
	token: Option<String>,
}

impl NuagesClient {
	/// Creates a new API client with the given base URL.
	pub(crate) fn new(base_url: &str) -> Self {
		Self {
			http: Client::new(),
			base_url: base_url.trim_end_matches('/').to_string(),
			token: None,
		}
	}

	/// Sets the authentication token.
	pub(crate) fn with_token(mut self, token: String) -> Self {
		self.token = Some(token);
		self
	}

	/// Returns the base URL.
	pub(crate) fn base_url(&self) -> &str {
		&self.base_url
	}

	/// Builds an authenticated request to the given API path.
	pub(crate) fn request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
		let url = format!("{}{}", self.base_url, path);
		let mut req = self.http.request(method, &url);
		if let Some(ref token) = self.token {
			req = req.bearer_auth(token);
		}
		req
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
}
