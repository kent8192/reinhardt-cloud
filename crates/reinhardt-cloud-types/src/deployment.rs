//! Deployment domain type representing an application deployment to a cluster.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a deployment lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeploymentStatus {
	Pending,
	Running,
	Failed,
	Succeeded,
}

/// An application deployment targeting a specific cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
	pub id: Uuid,
	pub app_name: String,
	pub cluster_id: Uuid,
	pub status: DeploymentStatus,
	pub image: String,
}

impl Deployment {
	/// Creates a new deployment in `Pending` status.
	pub fn new(app_name: &str, cluster_id: Uuid, image: &str) -> Self {
		Self {
			id: Uuid::now_v7(),
			app_name: app_name.to_string(),
			cluster_id,
			status: DeploymentStatus::Pending,
			image: image.to_string(),
		}
	}

	/// Returns true if the deployment has reached a terminal state.
	pub fn is_terminal(&self) -> bool {
		matches!(
			self.status,
			DeploymentStatus::Failed | DeploymentStatus::Succeeded
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn test_deployment_new_defaults_to_pending() {
		// Arrange
		let cluster_id = Uuid::now_v7();

		// Act
		let deploy = Deployment::new("my-app", cluster_id, "my-app:latest");

		// Assert
		assert_eq!(deploy.app_name, "my-app");
		assert_eq!(deploy.cluster_id, cluster_id);
		assert_eq!(deploy.image, "my-app:latest");
		assert_eq!(deploy.status, DeploymentStatus::Pending);
	}

	#[rstest]
	#[case(DeploymentStatus::Pending, false)]
	#[case(DeploymentStatus::Running, false)]
	#[case(DeploymentStatus::Failed, true)]
	#[case(DeploymentStatus::Succeeded, true)]
	fn test_is_terminal(#[case] status: DeploymentStatus, #[case] expected: bool) {
		// Arrange
		let mut deploy = Deployment::new("app", Uuid::now_v7(), "img:v1");

		// Act
		deploy.status = status;

		// Assert
		assert_eq!(deploy.is_terminal(), expected);
	}
}
