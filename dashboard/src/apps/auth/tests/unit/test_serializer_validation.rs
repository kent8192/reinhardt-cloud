//! Boundary value analysis and equivalence partitioning tests for auth serializers.

#[cfg(test)]
mod tests {
	use reinhardt::Validate;
	use rstest::rstest;

	use crate::apps::auth::serializers::{LoginRequest, RegisterRequest};

	// --- LoginRequest username boundary ---

	#[rstest]
	#[case::min_valid("a", true)]
	#[case::max_valid(&"a".repeat(150), true)]
	#[case::below_min("", false)]
	#[case::above_max(&"a".repeat(151), false)]
	fn test_login_request_username_boundary(#[case] username: &str, #[case] valid: bool) {
		// Arrange
		let req: LoginRequest = serde_json::from_value(serde_json::json!({
			"username": username,
			"password": "validpass"
		}))
		.expect("Failed to deserialize LoginRequest");

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"username len={} expected valid={valid}",
			username.len()
		);
	}

	// --- LoginRequest password boundary ---

	#[rstest]
	#[case::min_valid("p", true)]
	#[case::max_valid(&"p".repeat(128), true)]
	#[case::below_min("", false)]
	#[case::above_max(&"p".repeat(129), false)]
	fn test_login_request_password_boundary(#[case] password: &str, #[case] valid: bool) {
		// Arrange
		let req: LoginRequest = serde_json::from_value(serde_json::json!({
			"username": "validuser",
			"password": password
		}))
		.expect("Failed to deserialize LoginRequest");

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"password len={} expected valid={valid}",
			password.len()
		);
	}

	// --- RegisterRequest username boundary ---

	#[rstest]
	#[case::min_valid("abc", true)]
	#[case::max_valid(&"u".repeat(32), true)]
	#[case::below_min("ab", false)]
	#[case::above_max(&"u".repeat(33), false)]
	fn test_register_request_username_boundary(#[case] username: &str, #[case] valid: bool) {
		// Arrange
		let req: RegisterRequest = serde_json::from_value(serde_json::json!({
			"username": username,
			"email": "test@example.com",
			"password": "securepassword"
		}))
		.expect("Failed to deserialize RegisterRequest");

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"username len={} expected valid={valid}",
			username.len()
		);
	}

	// --- RegisterRequest password boundary ---

	#[rstest]
	#[case::min_valid(&"p".repeat(8), true)]
	#[case::max_valid(&"p".repeat(128), true)]
	#[case::below_min(&"p".repeat(7), false)]
	#[case::above_max(&"p".repeat(129), false)]
	fn test_register_request_password_boundary(#[case] password: &str, #[case] valid: bool) {
		// Arrange
		let req: RegisterRequest = serde_json::from_value(serde_json::json!({
			"username": "validuser",
			"email": "test@example.com",
			"password": password
		}))
		.expect("Failed to deserialize RegisterRequest");

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"password len={} expected valid={valid}",
			password.len()
		);
	}

	// --- RegisterRequest email max length ---

	#[rstest]
	#[case::max_valid(254, true)]
	#[case::above_max(255, false)]
	fn test_register_request_email_max_length(#[case] length: usize, #[case] valid: bool) {
		// Arrange — construct email of exact length with valid RFC 5321 format.
		// Local part max 64 chars, domain labels max 63 chars each.
		// Format: "user@" + domain padded to fill remaining length
		let local = "user"; // 4 chars
		let at = "@"; // 1 char
		let suffix = ".com"; // 4 chars
		let domain_body_len = length - local.len() - at.len() - suffix.len();
		// Build domain body as "aaa...a.bbb...b" with labels <= 63 chars
		let mut domain_body = String::new();
		let mut remaining = domain_body_len;
		while remaining > 0 {
			if !domain_body.is_empty() {
				domain_body.push('.');
				remaining -= 1;
			}
			let label_len = remaining.min(63);
			domain_body.push_str(&"a".repeat(label_len));
			remaining -= label_len;
		}
		let email = format!("{local}{at}{domain_body}{suffix}");
		assert_eq!(email.len(), length, "constructed email length mismatch");

		let req: RegisterRequest = serde_json::from_value(serde_json::json!({
			"username": "validuser",
			"email": email,
			"password": "securepassword"
		}))
		.expect("Failed to deserialize RegisterRequest");

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"email len={length} expected valid={valid}"
		);
	}

	// --- RegisterRequest email validation ---

	#[rstest]
	#[case::valid_simple("user@example.com", true)]
	#[case::valid_subdomain("user@sub.example.com", true)]
	#[case::valid_plus("user+tag@example.com", true)]
	#[case::invalid_no_at("userexample.com", false)]
	#[case::invalid_no_domain("user@", false)]
	#[case::invalid_double_at("user@@example.com", false)]
	#[case::invalid_empty("", false)]
	fn test_register_request_email_validation(#[case] email: &str, #[case] valid: bool) {
		// Arrange
		let req: RegisterRequest = serde_json::from_value(serde_json::json!({
			"username": "validuser",
			"email": email,
			"password": "securepassword"
		}))
		.expect("Failed to deserialize RegisterRequest");

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			valid,
			"email={email:?} expected valid={valid}"
		);
	}

	// --- LoginRequest missing fields ---

	#[rstest]
	#[case::missing_username(serde_json::json!({"password": "pass"}), "username")]
	#[case::missing_password(serde_json::json!({"username": "user"}), "password")]
	fn test_login_request_missing_field(#[case] json: serde_json::Value, #[case] field: &str) {
		// Arrange & Act
		let result = serde_json::from_value::<LoginRequest>(json);

		// Assert
		let err = result.expect_err("Should fail to deserialize without required field");
		let err_msg = err.to_string();
		assert!(
			err_msg.contains(field) || err_msg.contains("missing field"),
			"Error should mention missing field {field:?}, got: {err_msg}"
		);
	}

	// --- RegisterRequest missing fields ---

	#[rstest]
	#[case::missing_username(serde_json::json!({"email": "a@b.com", "password": "securepassword"}), "username")]
	#[case::missing_email(serde_json::json!({"username": "user", "password": "securepassword"}), "email")]
	#[case::missing_password(serde_json::json!({"username": "user", "email": "a@b.com"}), "password")]
	fn test_register_request_missing_field(#[case] json: serde_json::Value, #[case] field: &str) {
		// Arrange & Act
		let result = serde_json::from_value::<RegisterRequest>(json);

		// Assert
		let err = result.expect_err("Should fail to deserialize without required field");
		let err_msg = err.to_string();
		assert!(
			err_msg.contains(field) || err_msg.contains("missing field"),
			"Error should mention missing field {field:?}, got: {err_msg}"
		);
	}
}
