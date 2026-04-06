//! Shared error types for frontend-server communication.

use serde::{Deserialize, Serialize};

/// Application-level error for server function responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppError {
	/// Invalid username or password.
	InvalidCredentials,
	/// Field-level validation errors.
	ValidationError(Vec<FieldError>),
	/// Session has expired, re-authentication needed.
	SessionExpired,
	/// Requested resource was not found.
	NotFound,
	/// Resource conflict (e.g., duplicate username or email).
	Conflict(String),
	/// Internal server error.
	Internal(String),
}

/// A single field validation error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldError {
	/// Name of the field that failed validation.
	pub field: String,
	/// Human-readable error message.
	pub message: String,
}

impl std::fmt::Display for AppError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::InvalidCredentials => write!(f, "Invalid credentials"),
			Self::ValidationError(errors) => {
				write!(f, "Validation errors: ")?;
				for (i, error) in errors.iter().enumerate() {
					if i > 0 {
						write!(f, ", ")?;
					}
					write!(f, "{}: {}", error.field, error.message)?;
				}
				Ok(())
			}
			Self::Conflict(msg) => write!(f, "Conflict: {msg}"),
			Self::SessionExpired => write!(f, "Session expired"),
			Self::NotFound => write!(f, "Not found"),
			Self::Internal(msg) => write!(f, "Internal error: {msg}"),
		}
	}
}

impl std::error::Error for AppError {}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(AppError::InvalidCredentials, "Invalid credentials")]
	#[case(AppError::SessionExpired, "Session expired")]
	#[case(AppError::NotFound, "Not found")]
	#[case(AppError::Conflict("dup".into()), "Conflict: dup")]
	#[case(AppError::Internal("boom".into()), "Internal error: boom")]
	fn test_app_error_display_variants(#[case] error: AppError, #[case] expected: &str) {
		// Act
		let display = error.to_string();

		// Assert
		assert_eq!(display, expected);
	}

	#[rstest]
	fn test_app_error_validation_display() {
		// Arrange
		let error = AppError::ValidationError(vec![
			FieldError {
				field: "username".to_string(),
				message: "too short".to_string(),
			},
			FieldError {
				field: "email".to_string(),
				message: "invalid format".to_string(),
			},
		]);

		// Act
		let display = error.to_string();

		// Assert
		assert!(display.contains("username: too short"));
		assert!(display.contains("email: invalid format"));
	}

	#[rstest]
	#[case(AppError::InvalidCredentials)]
	#[case(AppError::SessionExpired)]
	#[case(AppError::NotFound)]
	#[case(AppError::Conflict("dup".into()))]
	#[case(AppError::Internal("boom".into()))]
	#[case(AppError::ValidationError(vec![FieldError { field: "f".into(), message: "m".into() }]))]
	fn test_app_error_serde_roundtrip(#[case] error: AppError) {
		// Act
		let json = serde_json::to_string(&error).unwrap();
		let roundtrip: AppError = serde_json::from_str(&json).unwrap();

		// Assert
		assert_eq!(error.to_string(), roundtrip.to_string());
	}

	#[rstest]
	fn test_field_error_construction() {
		// Arrange
		let field_error = FieldError {
			field: "password".to_string(),
			message: "must be at least 8 characters".to_string(),
		};

		// Assert
		assert_eq!(field_error.field, "password");
		assert_eq!(field_error.message, "must be at least 8 characters");
	}
}
