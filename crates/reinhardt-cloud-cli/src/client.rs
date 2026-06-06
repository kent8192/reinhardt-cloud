//! Control-plane target configuration for CLI commands.

use thiserror::Error;
use url::Url;

/// Errors from control-plane target configuration.
#[derive(Debug, Error)]
pub(crate) enum ClientError {
	#[error("invalid URL: {0}")]
	InvalidUrl(#[from] url::ParseError),
}

/// Target endpoint metadata for the Reinhardt Cloud platform.
#[derive(Debug, Clone)]
pub(crate) struct ReinhardtCloudClient {
	base_url: Url,
	token: Option<String>,
}

impl ReinhardtCloudClient {
	/// Creates a new control-plane target with the given base URL.
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

	/// Stores the authentication token for commands that need target metadata.
	pub(crate) fn with_token(mut self, token: String) -> Self {
		self.token = Some(token);
		self
	}

	/// Returns the base URL as a string (without trailing slash).
	pub(crate) fn base_url(&self) -> &str {
		self.base_url.as_str().trim_end_matches('/')
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
}
