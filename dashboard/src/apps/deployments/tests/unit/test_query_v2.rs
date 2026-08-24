//! Deployment Query Client V2 contracts.

#![cfg(test)]

use rstest::rstest;

use crate::apps::deployments::server_fn::deployment_logs_for_current_org;

#[rstest]
fn deployment_log_queries_use_exact_keys_with_a_shared_family() {
	// Arrange
	let first = deployment_logs_for_current_org::key("41".to_owned());
	let first_again = deployment_logs_for_current_org::key("41".to_owned());
	let second = deployment_logs_for_current_org::key("42".to_owned());

	// Act
	let first_id = first.id();
	let first_again_id = first_again.id();
	let second_id = second.id();

	// Assert
	assert_eq!(first_id, first_again_id);
	assert_ne!(first_id, second_id);
	assert_eq!(first.family_id(), second.family_id());
	assert_eq!(
		first.family_id(),
		deployment_logs_for_current_org::family().id()
	);
}
