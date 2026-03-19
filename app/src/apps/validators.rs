//! Shared validation functions for API request payloads.

/// Validate that a string is a valid Kubernetes DNS label (RFC 1123).
///
/// A valid DNS label:
/// - Contains only lowercase alphanumeric characters or hyphens
/// - Starts with an alphanumeric character
/// - Ends with an alphanumeric character
/// - Is at most 63 characters long
pub fn validate_k8s_dns_label(value: &str) -> Result<(), String> {
	if value.is_empty() || value.len() > 63 {
		return Err("must be between 1 and 63 characters".into());
	}

	let bytes = value.as_bytes();

	// Must start with lowercase alphanumeric
	if !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit() {
		return Err("must start with a lowercase alphanumeric character".into());
	}

	// Must end with lowercase alphanumeric
	if !bytes[bytes.len() - 1].is_ascii_lowercase() && !bytes[bytes.len() - 1].is_ascii_digit() {
		return Err("must end with a lowercase alphanumeric character".into());
	}

	// All characters must be lowercase alphanumeric or hyphen
	for &b in bytes {
		if !b.is_ascii_lowercase() && !b.is_ascii_digit() && b != b'-' {
			return Err("must contain only lowercase alphanumeric characters or hyphens".into());
		}
	}

	Ok(())
}
