//! Deployment Query Client V2 contracts.

#![cfg(test)]

use std::collections::HashMap;

use reinhardt::pages::component::Page;
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::router::request::{ExtractError, FromRequest, RouteContext};
use reinhardt::pages::router::{ClientRouter, Query};
use rstest::rstest;

use crate::apps::deployments::client::pages::list::{
	DeploymentsListPageProps, deployment_logs_path, selected_log_deployment_id,
};
use crate::apps::deployments::server_fn::deployment_logs_for_current_org;

#[reinhardt::pages::component("/deployments", name = "deployments:history-probe")]
fn deployment_log_history_probe(Query(logs): Query<Option<i64>>) -> Page {
	Page::text(logs.map_or_else(
		|| "none".to_owned(),
		|deployment_id| deployment_id.to_string(),
	))
}

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

#[rstest]
#[case::absent("", None, "")]
#[case::unrelated_query("tab=overview", None, "")]
#[case::selected("logs=41", Some(41), "41")]
#[case::selected_with_other_query("tab=overview&logs=42", Some(42), "42")]
#[case::zero("logs=0", Some(0), "")]
#[case::negative("logs=-1", Some(-1), "")]
fn deployments_route_extracts_optional_log_selection(
	#[case] query: &str,
	#[case] expected: Option<i64>,
	#[case] expected_selected_deployment_id: &str,
) {
	// Arrange
	let context = RouteContext::new("/deployments".to_owned(), HashMap::new(), query.to_owned());

	// Act
	let props = DeploymentsListPageProps::from_request(&context)
		.expect("the optional logs query should extract");

	// Assert
	assert_eq!(props.logs, expected);
	assert_eq!(
		selected_log_deployment_id(props.logs),
		expected_selected_deployment_id
	);
}

#[rstest]
fn deployments_route_rejects_invalid_log_selection() {
	// Arrange
	let context = RouteContext::new(
		"/deployments".to_owned(),
		HashMap::new(),
		"logs=invalid".to_owned(),
	);

	// Act
	let extraction = DeploymentsListPageProps::from_request(&context);

	// Assert
	let Err(ExtractError::Parse { name, .. }) = extraction else {
		panic!("invalid logs query must return a parse error");
	};
	assert_eq!(name, "logs");
}

#[rstest]
fn deployment_log_deep_links_select_the_generated_exact_query_key() {
	// Arrange
	let path = deployment_logs_path("/deployments", Some(41));
	let (_, query) = path
		.split_once('?')
		.expect("a selected deployment must produce a logs query string");
	let context = RouteContext::new("/deployments".to_owned(), HashMap::new(), query.to_owned());

	// Act
	let deployment_id = DeploymentsListPageProps::from_request(&context)
		.expect("a deployment log deep link should extract")
		.logs
		.expect("a selected deep link should contain a deployment ID");
	let selected_key = deployment_logs_for_current_org::key(deployment_id.to_string());

	// Assert
	assert_eq!(
		selected_key.id(),
		deployment_logs_for_current_org::key("41".to_owned()).id()
	);
}

#[rstest]
fn deployment_log_route_rerenders_for_history_navigation() {
	ReactiveScope::run(|| {
		// Arrange
		let router = ClientRouter::new().component(deployment_log_history_probe);

		// Act + Assert: the initial URL, a selection, a subsequent selection,
		// and a Back navigation each reconstruct the generated route input.
		router.current_path().set("/deployments".to_owned());
		assert_eq!(router.render_current().render_to_string(), "none");
		router.current_path().set("/deployments?logs=41".to_owned());
		assert_eq!(router.render_current().render_to_string(), "41");
		router.current_path().set("/deployments?logs=42".to_owned());
		assert_eq!(router.render_current().render_to_string(), "42");
		router.current_path().set("/deployments?logs=41".to_owned());
		assert_eq!(router.render_current().render_to_string(), "41");
	});
}
