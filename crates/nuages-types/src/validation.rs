//! Validation error types for configuration and CRD spec validation.

use std::fmt;

/// An error encountered during validation of configuration or CRD specs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
	/// Human-readable description of the validation failure.
	pub message: String,
}

impl ValidationError {
	/// Creates a new `ValidationError` with the given message.
	pub fn new(message: impl Into<String>) -> Self {
		Self {
			message: message.into(),
		}
	}
}

impl fmt::Display for ValidationError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}", self.message)
	}
}

impl std::error::Error for ValidationError {}
