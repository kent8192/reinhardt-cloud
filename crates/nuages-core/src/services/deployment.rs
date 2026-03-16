//! Deployment management business logic.

use nuages_types::DeploymentStatus;

use crate::error::ApiError;

/// Validates that a deployment status transition is allowed.
///
/// Allowed transitions:
/// - `Pending` -> `Running`
/// - `Pending` -> `Failed`
/// - `Running` -> `Succeeded`
/// - `Running` -> `Failed`
pub fn validate_status_transition(
	current: &DeploymentStatus,
	target: &DeploymentStatus,
) -> Result<(), ApiError> {
	let allowed = matches!(
		(current, target),
		(DeploymentStatus::Pending, DeploymentStatus::Running)
			| (DeploymentStatus::Pending, DeploymentStatus::Failed)
			| (DeploymentStatus::Running, DeploymentStatus::Succeeded)
			| (DeploymentStatus::Running, DeploymentStatus::Failed)
	);
	if allowed {
		Ok(())
	} else {
		Err(ApiError::BadRequest(format!(
			"invalid status transition from {current:?} to {target:?}"
		)))
	}
}

/// Validates that a Docker image reference is well-formed.
pub fn validate_image_ref(image: &str) -> Result<(), ApiError> {
	if image.is_empty() {
		return Err(ApiError::BadRequest(
			"image reference cannot be empty".to_string(),
		));
	}
	if image.contains(' ') {
		return Err(ApiError::BadRequest(
			"image reference must not contain spaces".to_string(),
		));
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(DeploymentStatus::Pending, DeploymentStatus::Running)]
	#[case(DeploymentStatus::Pending, DeploymentStatus::Failed)]
	#[case(DeploymentStatus::Running, DeploymentStatus::Succeeded)]
	#[case(DeploymentStatus::Running, DeploymentStatus::Failed)]
	fn test_valid_status_transition(
		#[case] current: DeploymentStatus,
		#[case] target: DeploymentStatus,
	) {
		// Arrange (provided by case)

		// Act
		let result = validate_status_transition(&current, &target);

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	#[case(DeploymentStatus::Succeeded, DeploymentStatus::Running)]
	#[case(DeploymentStatus::Failed, DeploymentStatus::Running)]
	#[case(DeploymentStatus::Running, DeploymentStatus::Pending)]
	#[case(DeploymentStatus::Succeeded, DeploymentStatus::Failed)]
	fn test_invalid_status_transition(
		#[case] current: DeploymentStatus,
		#[case] target: DeploymentStatus,
	) {
		// Arrange (provided by case)

		// Act
		let result = validate_status_transition(&current, &target);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}

	#[rstest]
	#[case("nginx:latest")]
	#[case("registry.example.com/app:v1.2.3")]
	#[case("my-app")]
	fn test_validate_image_ref_valid(#[case] image: &str) {
		// Arrange (provided by case)

		// Act
		let result = validate_image_ref(image);

		// Assert
		assert!(result.is_ok());
	}

	#[rstest]
	fn test_validate_image_ref_empty() {
		// Arrange
		let image = "";

		// Act
		let result = validate_image_ref(image);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}

	#[rstest]
	fn test_validate_image_ref_with_spaces() {
		// Arrange
		let image = "my app:latest";

		// Act
		let result = validate_image_ref(image);

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().status_code(), 400);
	}
}
