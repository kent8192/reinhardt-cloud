//! Deployment status badge component rendered with `page!` macro.

use crate::shared::client::style::STYLES;
use crate::shared::ws_messages::DeploymentState;

/// Map deployment state to CSS class and display text.
pub fn badge_style(state: &DeploymentState) -> (&'static str, &'static str) {
	match state {
		DeploymentState::Running => (STYLES.status_running().as_str(), "Running"),
		DeploymentState::Deploying => (STYLES.status_deploying().as_str(), "Deploying"),
		DeploymentState::Degraded => (STYLES.status_degraded().as_str(), "Degraded"),
		DeploymentState::Failed => (STYLES.status_failed().as_str(), "Failed"),
		DeploymentState::Stopped => (STYLES.status_stopped().as_str(), "Stopped"),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(DeploymentState::Running, STYLES.status_running().as_str(), "Running")]
	#[case(DeploymentState::Deploying, STYLES.status_deploying().as_str(), "Deploying")]
	#[case(DeploymentState::Degraded, STYLES.status_degraded().as_str(), "Degraded")]
	#[case(DeploymentState::Failed, STYLES.status_failed().as_str(), "Failed")]
	#[case(DeploymentState::Stopped, STYLES.status_stopped().as_str(), "Stopped")]
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
