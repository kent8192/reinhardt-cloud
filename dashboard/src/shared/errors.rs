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
