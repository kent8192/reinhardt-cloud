//! Framework-agnostic API error types for the nuages platform.

use thiserror::Error;

/// Framework-agnostic API errors for the nuages platform.
#[derive(Debug, Error)]
pub enum ApiError {
	#[error("unauthorized: {0}")]
	Unauthorized(String),

	#[error("not found: {0}")]
	NotFound(String),

	#[error("bad request: {0}")]
	BadRequest(String),

	#[error("internal error: {0}")]
	Internal(String),
}

impl ApiError {
	/// Returns the HTTP status code associated with this error.
	pub fn status_code(&self) -> u16 {
		match self {
			Self::Unauthorized(_) => 401,
			Self::NotFound(_) => 404,
			Self::BadRequest(_) => 400,
			Self::Internal(_) => 500,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(ApiError::Unauthorized("no token".to_string()), 401)]
	#[case(ApiError::NotFound("resource missing".to_string()), 404)]
	#[case(ApiError::BadRequest("invalid input".to_string()), 400)]
	#[case(ApiError::Internal("server failure".to_string()), 500)]
	fn test_status_code(#[case] error: ApiError, #[case] expected: u16) {
		// Arrange (provided by case)

		// Act
		let code = error.status_code();

		// Assert
		assert_eq!(code, expected);
	}

	#[rstest]
	#[case(ApiError::Unauthorized("no token".to_string()), "unauthorized: no token")]
	#[case(ApiError::NotFound("user 42".to_string()), "not found: user 42")]
	#[case(ApiError::BadRequest("missing field".to_string()), "bad request: missing field")]
	#[case(ApiError::Internal("db down".to_string()), "internal error: db down")]
	fn test_display(#[case] error: ApiError, #[case] expected: &str) {
		// Arrange (provided by case)

		// Act
		let msg = error.to_string();

		// Assert
		assert_eq!(msg, expected);
	}
}
