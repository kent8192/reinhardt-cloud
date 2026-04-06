//! Boundary value analysis and equivalence partitioning for CreateClusterRequest.

#[cfg(test)]
mod tests {
	use reinhardt::Validate;
	use rstest::rstest;

	use crate::apps::clusters::serializers::CreateClusterRequest;

	// -- Name boundary tests --

	#[rstest]
	#[case("a", true)] // min valid (1 char)
	#[case("", false)] // below min (0 chars)
	#[case(&"a".repeat(63), true)] // max valid (63 chars)
	#[case(&"a".repeat(64), false)] // above max (64 chars)
	fn test_cluster_name_boundary(#[case] name: &str, #[case] expected_valid: bool) {
		// Arrange
		let req = CreateClusterRequest {
			name: name.to_string(),
			api_url: "https://k8s.example.com:6443".to_string(),
		};

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			expected_valid,
			"name='{}' (len={}) should be valid={}",
			name,
			name.len(),
			expected_valid
		);
	}

	// -- URL validation tests --

	#[rstest]
	#[case("https://k8s.example.com:6443", true)] // valid HTTPS URL
	#[case("http://10.0.0.1:6443", true)] // valid HTTP URL with IP
	#[case("not-a-url", false)] // invalid: no scheme
	#[case("", false)] // invalid: empty string
	fn test_cluster_api_url_validation(#[case] url: &str, #[case] expected_valid: bool) {
		// Arrange
		let req = CreateClusterRequest {
			name: "valid-cluster".to_string(),
			api_url: url.to_string(),
		};

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			expected_valid,
			"api_url='{}' should be valid={}",
			url,
			expected_valid
		);
	}

	// -- URL max length tests --

	#[rstest]
	#[case(2048, true)] // max valid length
	#[case(2049, false)] // above max length
	fn test_cluster_api_url_max_length(#[case] len: usize, #[case] expected_valid: bool) {
		// Arrange
		let base = "https://k8s.example.com/";
		let padding = "a".repeat(len - base.len());
		let url = format!("{}{}", base, padding);
		assert_eq!(url.len(), len);

		let req = CreateClusterRequest {
			name: "valid-cluster".to_string(),
			api_url: url.clone(),
		};

		// Act
		let result = req.validate();

		// Assert
		assert_eq!(
			result.is_ok(),
			expected_valid,
			"api_url with len={} should be valid={}",
			len,
			expected_valid
		);
	}

	// -- Missing fields deserialization test --

	#[rstest]
	#[case(r#"{"api_url": "https://k8s.example.com:6443"}"#, "missing name")]
	#[case(r#"{"name": "prod"}"#, "missing api_url")]
	#[case(r#"{}"#, "missing both fields")]
	fn test_create_cluster_request_missing_fields(#[case] json: &str, #[case] description: &str) {
		// Arrange / Act
		let result = serde_json::from_str::<CreateClusterRequest>(json);

		// Assert
		assert!(
			result.is_err(),
			"deserialization should fail for: {}",
			description
		);
	}
}
