//! Deployment status badge component rendered with `page!` macro.

use crate::shared::ws_messages::DeploymentState;

/// Map deployment state to CSS class and display text.
pub fn badge_style(state: &DeploymentState) -> (&'static str, &'static str) {
	match state {
		DeploymentState::Running => ("bg-green-100 text-green-800", "Running"),
		DeploymentState::Deploying => ("bg-blue-100 text-blue-800", "Deploying"),
		DeploymentState::Degraded => ("bg-amber-100 text-amber-800", "Degraded"),
		DeploymentState::Failed => ("bg-red-100 text-red-800", "Failed"),
		DeploymentState::Stopped => ("bg-gray-100 text-gray-800", "Stopped"),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(DeploymentState::Running, "bg-green-100 text-green-800", "Running")]
	#[case(DeploymentState::Deploying, "bg-blue-100 text-blue-800", "Deploying")]
	#[case(DeploymentState::Degraded, "bg-amber-100 text-amber-800", "Degraded")]
	#[case(DeploymentState::Failed, "bg-red-100 text-red-800", "Failed")]
	#[case(DeploymentState::Stopped, "bg-gray-100 text-gray-800", "Stopped")]
	fn test_badge_style_returns_correct_classes_and_label(
		#[case] state: DeploymentState,
		#[case] expected_color: &str,
		#[case] expected_label: &str,
	) {
		// Act
		let (color, label) = badge_style(&state);

		// Assert
		assert_eq!(color, expected_color);
		assert_eq!(label, expected_label);
	}
}
