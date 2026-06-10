//! Validation error types and helpers for configuration and CRD spec validation.

use std::fmt;

/// Maximum length of a DNS-1123 label, per RFC 1123 and Kubernetes naming rules.
const DNS_1123_LABEL_MAX_LENGTH: usize = 63;

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

/// Validates that a string is a valid DNS-1123 label per RFC 1123.
///
/// A DNS-1123 label must:
/// - Be 1 to 63 characters long
/// - Contain only lowercase alphanumeric characters and hyphens
/// - Start and end with an alphanumeric character
///
/// This is the format required for Kubernetes namespace names and is used to
/// validate the multi-tenant `spec.tenant` field on `Project` CRs.
pub fn validate_dns_1123_label(value: &str) -> Result<(), ValidationError> {
	if value.is_empty() {
		return Err(ValidationError::new("must not be empty"));
	}
	if value.len() > DNS_1123_LABEL_MAX_LENGTH {
		return Err(ValidationError::new(format!(
			"must be at most {DNS_1123_LABEL_MAX_LENGTH} characters, got {}",
			value.len()
		)));
	}
	let bytes = value.as_bytes();
	if !is_dns_1123_alphanumeric(bytes[0]) {
		return Err(ValidationError::new(
			"must start with a lowercase alphanumeric character",
		));
	}
	if !is_dns_1123_alphanumeric(bytes[bytes.len() - 1]) {
		return Err(ValidationError::new(
			"must end with a lowercase alphanumeric character",
		));
	}
	for &b in bytes {
		if !(is_dns_1123_alphanumeric(b) || b == b'-') {
			return Err(ValidationError::new(
				"must contain only lowercase alphanumeric characters and hyphens",
			));
		}
	}
	Ok(())
}

#[inline]
fn is_dns_1123_alphanumeric(b: u8) -> bool {
	b.is_ascii_lowercase() || b.is_ascii_digit()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case("a")]
	#[case("0")]
	#[case("foo")]
	#[case("foo-bar")]
	#[case("foo-bar-baz")]
	#[case("a1b2c3")]
	#[case("123abc")]
	#[case("acme-prod")]
	fn validate_dns_1123_label_accepts_valid_labels(#[case] label: &str) {
		// Arrange + Act
		let result = validate_dns_1123_label(label);

		// Assert
		assert!(
			result.is_ok(),
			"expected {label:?} to be valid, got {:?}",
			result.err()
		);
	}

	#[rstest]
	fn validate_dns_1123_label_accepts_max_length_label() {
		// Arrange
		let label = "a".repeat(DNS_1123_LABEL_MAX_LENGTH);

		// Act
		let result = validate_dns_1123_label(&label);

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	#[case("", "must not be empty")]
	#[case("FOO", "must start with a lowercase alphanumeric character")]
	#[case("-foo", "must start with a lowercase alphanumeric character")]
	#[case("foo-", "must end with a lowercase alphanumeric character")]
	#[case(
		"foo_bar",
		"must contain only lowercase alphanumeric characters and hyphens"
	)]
	#[case(
		"foo bar",
		"must contain only lowercase alphanumeric characters and hyphens"
	)]
	#[case(
		"foo.bar",
		"must contain only lowercase alphanumeric characters and hyphens"
	)]
	#[case("FooBar", "must start with a lowercase alphanumeric character")]
	fn validate_dns_1123_label_rejects_invalid_labels(
		#[case] label: &str,
		#[case] expected_substring: &str,
	) {
		// Arrange + Act
		let result = validate_dns_1123_label(label);

		// Assert
		let err = result.expect_err("expected validation error");
		assert!(
			err.message.contains(expected_substring),
			"expected message containing {expected_substring:?}, got {:?}",
			err.message
		);
	}

	#[rstest]
	fn validate_dns_1123_label_rejects_too_long_label() {
		// Arrange
		let label = "a".repeat(DNS_1123_LABEL_MAX_LENGTH + 1);

		// Act
		let result = validate_dns_1123_label(&label);

		// Assert
		let err = result.expect_err("expected validation error");
		assert!(
			err.message.contains("at most 63 characters"),
			"expected length error, got {:?}",
			err.message
		);
	}
}
