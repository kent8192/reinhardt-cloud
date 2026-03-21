//! Cluster management business logic.

use reinhardt_cloud_types::Cluster;

use crate::error::ApiError;

/// Validates that a cluster name meets Kubernetes naming requirements.
pub fn validate_cluster_name(name: &str) -> Result<(), ApiError> {
	if name.is_empty() {
		return Err(ApiError::BadRequest(
			"cluster name cannot be empty".to_string(),
		));
	}
	if name.len() > 63 {
		return Err(ApiError::BadRequest(
			"cluster name must be 63 characters or fewer".to_string(),
		));
	}
	// Kubernetes-compatible naming: lowercase alphanumeric and hyphens
	if !name
		.chars()
		.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
	{
		return Err(ApiError::BadRequest(
			"cluster name must contain only lowercase alphanumeric characters and hyphens"
				.to_string(),
		));
	}
	if name.starts_with('-') || name.ends_with('-') {
		return Err(ApiError::BadRequest(
			"cluster name must not start or end with a hyphen".to_string(),
		));
	}
	Ok(())
}

/// Determines if a cluster is healthy based on its active status and API URL.
pub fn is_cluster_healthy(cluster: &Cluster) -> bool {
	cluster.is_active && !cluster.api_url.is_empty()
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case("prod-cluster")]
	#[case("dev-01")]
	#[case("a")]
	#[case("cluster-with-many-segments-123")]
	fn test_validate_cluster_name_valid(#[case] name: &str) {
		// Arrange (provided by case)

		// Act
		let result = validate_cluster_name(name);

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_validate_cluster_name_empty() {
		// Arrange
		let name = "";

		// Act
		let result = validate_cluster_name(name);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}

	#[rstest]
	fn test_validate_cluster_name_too_long() {
		// Arrange
		let name = "a".repeat(64);

		// Act
		let result = validate_cluster_name(&name);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}

	#[rstest]
	#[case("Prod-Cluster")]
	#[case("has space")]
	#[case("under_score")]
	#[case("UPPER")]
	fn test_validate_cluster_name_invalid_chars(#[case] name: &str) {
		// Arrange (provided by case)

		// Act
		let result = validate_cluster_name(name);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}

	#[rstest]
	#[case("-leading")]
	#[case("trailing-")]
	fn test_validate_cluster_name_hyphen_position(#[case] name: &str) {
		// Arrange (provided by case)

		// Act
		let result = validate_cluster_name(name);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}

	#[rstest]
	fn test_is_cluster_healthy_active_with_url() {
		// Arrange
		let cluster = Cluster::new("prod", "https://k8s.example.com:6443");

		// Act
		let healthy = is_cluster_healthy(&cluster);

		// Assert
		assert!(healthy);
	}

	#[rstest]
	fn test_is_cluster_healthy_inactive() {
		// Arrange
		let mut cluster = Cluster::new("prod", "https://k8s.example.com:6443");
		cluster.is_active = false;

		// Act
		let healthy = is_cluster_healthy(&cluster);

		// Assert
		assert!(!healthy);
	}

	#[rstest]
	fn test_is_cluster_healthy_empty_url() {
		// Arrange
		let mut cluster = Cluster::new("prod", "https://k8s.example.com:6443");
		cluster.api_url = String::new();

		// Act
		let healthy = is_cluster_healthy(&cluster);

		// Assert
		assert!(!healthy);
	}
}
