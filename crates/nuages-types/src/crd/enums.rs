//! Application lifecycle phase types.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Phase of the `ReinhardtApp` lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum AppPhase {
	Pending,
	Deploying,
	Running,
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
			(AppPhase::Pending, "\"Pending\""),
			(AppPhase::Deploying, "\"Deploying\""),
			(AppPhase::Running, "\"Running\""),
			(AppPhase::Failed, "\"Failed\""),
			(AppPhase::Terminating, "\"Terminating\""),
		];

		for (variant, expected) in &phases {
			// Act
			let json = serde_json::to_string(variant).expect("serialization should succeed");

			// Assert
			assert_eq!(json, *expected);
		}
	}
}
