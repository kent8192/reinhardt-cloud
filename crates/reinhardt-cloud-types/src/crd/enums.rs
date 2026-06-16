//! Project lifecycle phase types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Phase of the `Project` lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProjectPhase {
	Pending,
	/// Database/cache being provisioned
	Provisioning,
	Deploying,
	Running,
	/// Partial failure (e.g., migration failed)
	Degraded,
	Failed,
	Terminating,
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	fn app_phase_serialization_roundtrip() {
		// Arrange
		let phases = [
			(ProjectPhase::Pending, "\"pending\""),
			(ProjectPhase::Provisioning, "\"provisioning\""),
			(ProjectPhase::Deploying, "\"deploying\""),
			(ProjectPhase::Running, "\"running\""),
			(ProjectPhase::Degraded, "\"degraded\""),
			(ProjectPhase::Failed, "\"failed\""),
			(ProjectPhase::Terminating, "\"terminating\""),
		];

		for (variant, expected) in &phases {
			// Act
			let json = serde_json::to_string(variant).expect("serialization should succeed");

			// Assert
			assert_eq!(json, *expected);
		}
	}
}
