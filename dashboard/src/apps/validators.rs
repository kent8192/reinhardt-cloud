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

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case("a", true)]
	#[case("abc", true)]
	#[case("a-b", true)]
	#[case("a1", true)]
	#[case("1a", true)]
	#[case("abc-def-123", true)]
	#[case("a1b2c3", true)]
	fn test_valid_dns_labels(#[case] input: &str, #[case] expected_valid: bool) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert_eq!(result.is_ok(), expected_valid);
	}

	#[rstest]
	#[case("", false, "must be between 1 and 63")]
	#[case("a", true, "")]
	#[case(&"a".repeat(63), true, "")]
	#[case(&"a".repeat(64), false, "must be between 1 and 63")]
	fn test_dns_label_length_boundary(
		#[case] input: &str,
		#[case] expected_ok: bool,
		#[case] expected_msg: &str,
	) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert_eq!(result.is_ok(), expected_ok);
		if !expected_ok {
			assert!(result.unwrap_err().contains(expected_msg));
		}
	}

	#[rstest]
	#[case("-abc", "must start")]
	#[case("_abc", "must start")]
	#[case("Abc", "must start")]
	fn test_dns_label_invalid_start(#[case] input: &str, #[case] expected_msg: &str) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert!(result.is_err());
		assert!(
			result.unwrap_err().contains(expected_msg),
			"expected error containing '{expected_msg}'"
		);
	}

	#[rstest]
	#[case("abc-", "must end")]
	fn test_dns_label_invalid_end(#[case] input: &str, #[case] expected_msg: &str) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert!(result.is_err());
		assert!(
			result.unwrap_err().contains(expected_msg),
			"expected error containing '{expected_msg}'"
		);
	}

	#[rstest]
	#[case("abc_def", "must contain only")]
	#[case("abc.def", "must contain only")]
	#[case("ab cd", "must contain only")]
	#[case("ABC", "must start")]
	fn test_dns_label_invalid_chars(#[case] input: &str, #[case] expected_msg: &str) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert!(result.is_err());
		assert!(
			result.unwrap_err().contains(expected_msg),
			"expected error containing '{expected_msg}'"
		);
	}

	#[rstest]
	#[case("Abc")]
	#[case("aBc")]
	#[case("abC")]
	fn test_dns_label_uppercase_rejected(#[case] input: &str) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	#[case("a", true)]
	#[case("1", true)]
	#[case("-", false)]
	#[case("A", false)]
	fn test_dns_label_single_char_boundary(#[case] input: &str, #[case] expected_ok: bool) {
		// Act
		let result = validate_k8s_dns_label(input);

		// Assert
		assert_eq!(result.is_ok(), expected_ok);
	}

	#[rstest]
	fn test_dns_label_combinatorial() {
		// Arrange
		let valid_starts = ['a', 'z', '0', '9'];
		let valid_ends = ['a', 'z', '0', '9'];
		let invalid_starts = ['-', 'A', '_'];
		let invalid_ends = ['-'];

		// Act & Assert — valid start × valid end
		for &start in &valid_starts {
			for &end in &valid_ends {
				let label = format!("{start}x{end}");
				assert!(
					validate_k8s_dns_label(&label).is_ok(),
					"expected '{label}' to be valid"
				);
			}
		}

		// Act & Assert — invalid start
		for &start in &invalid_starts {
			let label = format!("{start}abc");
			assert!(
				validate_k8s_dns_label(&label).is_err(),
				"expected '{label}' to be invalid"
			);
		}

		// Act & Assert — invalid end
		for &end in &invalid_ends {
			let label = format!("abc{end}");
			assert!(
				validate_k8s_dns_label(&label).is_err(),
				"expected 'abc{end}' to be invalid"
			);
		}
	}

	mod property_tests {
		use super::super::*;
		use proptest::prelude::*;

		proptest! {
			#[test]
			fn test_dns_label_property_valid_always_pass(
				s in "[a-z0-9][a-z0-9\\-]{0,61}[a-z0-9]"
			) {
				// Valid labels that match the regex should always pass
				if s.len() <= 63 {
					prop_assert!(validate_k8s_dns_label(&s).is_ok());
				}
			}

			#[test]
			fn test_dns_label_fuzz_never_panics(s in "\\PC{0,100}") {
				// Any string should never panic, only return Ok or Err
				let _ = validate_k8s_dns_label(&s);
			}
		}
	}
}
