//! Deployment ClientForm and route-bound log selection tests.

#![cfg(all(test, not(wasm)))]

use std::cell::Cell;
use std::rc::Rc;

use reinhardt::pages::app::{
	__clear_spa_router_for_test, __current_path_for_test, __install_client_router_for_test,
};
use reinhardt::pages::component::Page;
use reinhardt::pages::event::ClickEvent;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Callback, FieldError, QueryOptions, Signal, queries, use_action, use_form, use_query,
	use_router,
};
use reinhardt::pages::reactive::{ReactiveScope, with_runtime};
use reinhardt::pages::server_fn::ServerFnError;
use reinhardt::pages::testing::component::{Role, render};
use reinhardt::urls::routers::ClientRouter;
use rstest::rstest;
use serial_test::serial;

use crate::apps::deployments::client::pages::list::{
	DeploymentLogsRouteSelection, deployment_logs_path, deployment_logs_selection_from_path,
	install_deployment_log_route_sync,
};
use crate::apps::deployments::client::pages::list::{
	invalidate_deployment_delete_queries, invalidate_deployment_queries,
};
use crate::apps::deployments::server_fn::{
	CreateDeploymentFormRequest, CreateDeploymentFormRequestClientForm, DeploymentInfo,
	DeploymentLogInfo, ProjectPreviewSummary, UpdateDeploymentFormRequest,
	UpdateDeploymentFormRequestClientForm, UpdateDeploymentStatusFormRequestClientForm,
	deployment_logs_for_current_org, list_deployment_previews_for_current_org,
	list_deployments_for_current_org,
};
use crate::apps::github::server_fn::list_github_project_previews_for_current_org;

struct InstalledRouterGuard;

impl InstalledRouterGuard {
	fn install(router: ClientRouter) -> Self {
		__install_client_router_for_test(router);
		Self
	}
}

impl Drop for InstalledRouterGuard {
	fn drop(&mut self) {
		__clear_spa_router_for_test();
	}
}

fn deployments_router() -> ClientRouter {
	ClientRouter::new().route("deployments", "/deployments", || Page::Empty)
}

#[rstest]
#[case::absent("/deployments", DeploymentLogsRouteSelection::Absent)]
#[case::unrelated_query("/deployments?tab=overview", DeploymentLogsRouteSelection::Absent)]
#[case::valid("/deployments?logs=41", DeploymentLogsRouteSelection::Selected(41))]
#[case::valid_with_other_query(
	"/deployments?tab=overview&logs=42",
	DeploymentLogsRouteSelection::Selected(42)
)]
#[case::zero("/deployments?logs=0", DeploymentLogsRouteSelection::Invalid)]
#[case::negative("/deployments?logs=-1", DeploymentLogsRouteSelection::Invalid)]
#[case::non_numeric("/deployments?logs=not-an-id", DeploymentLogsRouteSelection::Invalid)]
fn deployment_log_route_parser_distinguishes_absent_valid_and_invalid(
	#[case] path: &str,
	#[case] expected: DeploymentLogsRouteSelection,
) {
	// Act
	let selection = deployment_logs_selection_from_path(path);

	// Assert
	assert_eq!(selection, expected);
}

#[rstest]
#[serial(deployment_log_router)]
fn deployment_log_route_sync_follows_replace_and_browser_path_changes() {
	ReactiveScope::run(|| {
		// Arrange
		let router = deployments_router();
		let router_for_popstate = router.clone();
		let _router = InstalledRouterGuard::install(router);
		let selected_deployment_id = Signal::new(String::new());
		install_deployment_log_route_sync(selected_deployment_id.clone());

		// Act: selector chooses a deployment through replace navigation.
		let selected_path = deployment_logs_path("/deployments", Some(41));
		let replace_result = use_router().replace(selected_path.clone());
		with_runtime(|runtime| runtime.flush_updates());

		// Assert: the canonical URL and reactive selection agree.
		assert!(
			replace_result.is_ok(),
			"selector replace must succeed: {replace_result:?}"
		);
		assert_eq!(
			__current_path_for_test().as_deref(),
			Some(selected_path.as_str())
		);
		assert_eq!(selected_deployment_id.get(), "41");
		let DeploymentLogsRouteSelection::Selected(deployment_id) =
			deployment_logs_selection_from_path(&selected_path)
		else {
			panic!("the selected URL must parse as a deployment log selection");
		};
		assert_eq!(
			deployment_logs_for_current_org::key(deployment_id.to_string()).id(),
			deployment_logs_for_current_org::key("41".to_owned()).id(),
			"a deep link must derive the same exact logs query key as the selector"
		);

		// Act: simulate a browser Back/Forward popstate update.
		router_for_popstate
			.current_path()
			.set("/deployments?logs=42".to_owned());
		with_runtime(|runtime| runtime.flush_updates());

		// Assert: router path updates flow back into the signal used by logs/WebSocket.
		assert_eq!(selected_deployment_id.get(), "42");

		// Act: an invalid deep link and a selector clear both produce no selection.
		router_for_popstate
			.current_path()
			.set("/deployments?logs=bad".to_owned());
		with_runtime(|runtime| runtime.flush_updates());
		let clear_path = deployment_logs_path("/deployments", None);
		let clear_result = use_router().replace(clear_path.clone());
		with_runtime(|runtime| runtime.flush_updates());

		// Assert
		assert_eq!(selected_deployment_id.get(), "");
		assert!(
			clear_result.is_ok(),
			"clear replace must succeed: {clear_result:?}"
		);
		assert_eq!(
			__current_path_for_test().as_deref(),
			Some(clear_path.as_str())
		);
	});
}

#[rstest]
fn generated_deployment_forms_validate_fields_and_map_structured_errors() {
	let scope = ReactiveScope::new();
	scope.enter(|| {
		// Arrange
		let create_form = CreateDeploymentFormRequestClientForm::new();
		let create_runtime = use_form(&create_form).build();
		let update_form = UpdateDeploymentFormRequestClientForm::new().with_defaults(
			UpdateDeploymentFormRequest {
				deployment_id: "17".to_owned(),
				project_name: "web".to_owned(),
				image: "ghcr.io/example/web:latest".to_owned(),
				status: "running".to_owned(),
			},
		);
		let update_runtime = use_form(&update_form).build();
		let status_form = UpdateDeploymentStatusFormRequestClientForm::new();
		let status_runtime = use_form(&status_form).build();

		// Act
		let create_result = create_runtime.trigger();
		let status_result = status_runtime.trigger();
		update_runtime.apply_server_error(&ServerFnError::validation_with_message(
			"Correct the deployment values",
			[
				("image", "Image registry is not permitted"),
				("unknown", "An unmapped validation error"),
			],
		));

		// Assert
		assert!(create_result.is_err());
		assert!(
			create_runtime
				.get_field_state(create_form.project_name_field())
				.error
				.is_some()
		);
		assert!(
			create_runtime
				.get_field_state(create_form.cluster_id_field())
				.error
				.is_some()
		);
		assert!(
			create_runtime
				.get_field_state(create_form.image_field())
				.error
				.is_some()
		);
		assert!(status_result.is_err());
		assert!(
			status_runtime
				.get_field_state(status_form.deployment_id_field())
				.error
				.is_some()
		);
		assert!(
			status_runtime
				.get_field_state(status_form.status_field())
				.error
				.is_some()
		);
		assert_eq!(
			update_runtime
				.get_field_state(update_form.image_field())
				.error
				.as_ref()
				.map(FieldError::message),
			Some("Image registry is not permitted")
		);
		assert_eq!(
			update_runtime.form_state().form_error.get(),
			Some("Correct the deployment values\nunknown: An unmapped validation error".to_owned())
		);
		assert_eq!(
			UpdateDeploymentFormRequestClientForm::to_request(&update_runtime),
			UpdateDeploymentFormRequest {
				deployment_id: "17".to_owned(),
				project_name: "web".to_owned(),
				image: "ghcr.io/example/web:latest".to_owned(),
				status: "running".to_owned(),
			}
		);
	});
}

fn create_deployment_invalidation_probe(
	deployment_fetches: Rc<Cell<u32>>,
	preview_fetches: Rc<Cell<u32>>,
	first_log_fetches: Rc<Cell<u32>>,
	second_log_fetches: Rc<Cell<u32>>,
	github_project_preview_fetches: Rc<Cell<u32>>,
) -> Page {
	let deployments = use_query(
		list_deployments_for_current_org::family().query((), move || {
			deployment_fetches.set(deployment_fetches.get() + 1);
			async { Ok::<Vec<DeploymentInfo>, ServerFnError>(Vec::new()) }
		}),
		QueryOptions::new(),
	);
	let previews = use_query(
		list_deployment_previews_for_current_org::family().query((), move || {
			preview_fetches.set(preview_fetches.get() + 1);
			async { Ok::<Vec<ProjectPreviewSummary>, ServerFnError>(Vec::new()) }
		}),
		QueryOptions::new(),
	);
	let first_logs = use_query(
		deployment_logs_for_current_org::family().query(("41".to_owned(),), move || {
			first_log_fetches.set(first_log_fetches.get() + 1);
			async { Ok::<Vec<DeploymentLogInfo>, ServerFnError>(Vec::new()) }
		}),
		QueryOptions::new(),
	);
	let second_logs = use_query(
		deployment_logs_for_current_org::family().query(("42".to_owned(),), move || {
			second_log_fetches.set(second_log_fetches.get() + 1);
			async { Ok::<Vec<DeploymentLogInfo>, ServerFnError>(Vec::new()) }
		}),
		QueryOptions::new(),
	);
	let github_project_previews = use_query(
		list_github_project_previews_for_current_org::family().query((), move || {
			github_project_preview_fetches.set(github_project_preview_fetches.get() + 1);
			async { Ok::<Vec<ProjectPreviewSummary>, ServerFnError>(Vec::new()) }
		}),
		QueryOptions::new(),
	);
	let query_client = queries();
	let form =
		CreateDeploymentFormRequestClientForm::new().with_defaults(CreateDeploymentFormRequest {
			project_name: "web".to_owned(),
			cluster_id: "7".to_owned(),
			image: "ghcr.io/example/web:latest".to_owned(),
			project_yaml: String::new(),
		});
	let create_runtime = use_form(&form)
		.on_submit_success(move |_runtime| {
			invalidate_deployment_queries(&query_client);
		})
		.build();
	let create_action_runtime = create_runtime.clone();
	let create_action = use_action(move |(): ()| {
		let runtime = create_action_runtime.clone();
		async move {
			runtime
				.submit_server_fn(|| async {
					Ok::<DeploymentInfo, ServerFnError>(DeploymentInfo {
						id: 99,
						project_name: "web".to_owned(),
						cluster_id: 7,
						status: "pending".to_owned(),
						image: "ghcr.io/example/web:latest".to_owned(),
					})
				})
				.await
		}
	});
	let submit = Callback::new(move |_event: ClickEvent| create_action.dispatch(()));
	let query_state = Page::reactive(move || {
		if deployments.data().is_some()
			&& previews.data().is_some()
			&& first_logs.data().is_some()
			&& second_logs.data().is_some()
			&& github_project_previews.data().is_some()
		{
			Page::text("Deployment queries loaded")
		} else {
			Page::text("Loading deployment queries")
		}
	});
	page!(|query_state: Page, submit: Callback<ClickEvent, ()>| {
		{ query_state }
		button {
			type: "button",
			@click: submit,
			"Create deployment"
		}
	})(query_state, submit)
}

fn shared_deployment_list_query_probe(fetches: Rc<Cell<u32>>) -> Page {
	let first = use_query(
		list_deployments_for_current_org::family().query((), {
			let fetches = Rc::clone(&fetches);
			move || {
				fetches.set(fetches.get() + 1);
				async { Ok::<Vec<DeploymentInfo>, ServerFnError>(Vec::new()) }
			}
		}),
		QueryOptions::new(),
	);
	let second = use_query(
		list_deployments_for_current_org::family().query((), {
			let fetches = Rc::clone(&fetches);
			move || {
				fetches.set(fetches.get() + 1);
				async { Ok::<Vec<DeploymentInfo>, ServerFnError>(Vec::new()) }
			}
		}),
		QueryOptions::new(),
	);
	Page::reactive(move || {
		if first.data().is_some() && second.data().is_some() {
			Page::text("Shared deployment list loaded")
		} else {
			Page::text("Loading deployment list")
		}
	})
}

fn github_project_preview_delete_invalidation_probe(fetches: Rc<Cell<u32>>) -> Page {
	let github_project_previews = use_query(
		list_github_project_previews_for_current_org::family().query((), move || {
			fetches.set(fetches.get() + 1);
			async { Ok::<Vec<ProjectPreviewSummary>, ServerFnError>(Vec::new()) }
		}),
		QueryOptions::new(),
	);
	let query_client = queries();
	let delete = Callback::new(move |_event: ClickEvent| {
		invalidate_deployment_delete_queries(&query_client);
	});
	let query_state = Page::reactive(move || {
		if github_project_previews.data().is_some() {
			Page::text("GitHub project previews loaded")
		} else {
			Page::text("Loading GitHub project previews")
		}
	});
	page!(|query_state: Page, delete: Callback<ClickEvent, ()>| {
		{ query_state }
		button {
			type: "button",
			@click: delete,
			"Delete deployment"
		}
	})(query_state, delete)
}

#[tokio::test]
async fn generated_create_form_success_invalidates_deployment_query_families() {
	// Arrange
	let deployment_fetches = Rc::new(Cell::new(0));
	let preview_fetches = Rc::new(Cell::new(0));
	let first_log_fetches = Rc::new(Cell::new(0));
	let second_log_fetches = Rc::new(Cell::new(0));
	let github_project_preview_fetches = Rc::new(Cell::new(0));
	let screen = render({
		let deployment_fetches = Rc::clone(&deployment_fetches);
		let preview_fetches = Rc::clone(&preview_fetches);
		let first_log_fetches = Rc::clone(&first_log_fetches);
		let second_log_fetches = Rc::clone(&second_log_fetches);
		let github_project_preview_fetches = Rc::clone(&github_project_preview_fetches);
		move || {
			create_deployment_invalidation_probe(
				deployment_fetches,
				preview_fetches,
				first_log_fetches,
				second_log_fetches,
				github_project_preview_fetches,
			)
		}
	});
	screen.settle().await;
	assert_eq!(deployment_fetches.get(), 1);
	assert_eq!(preview_fetches.get(), 1);
	assert_eq!(first_log_fetches.get(), 1);
	assert_eq!(second_log_fetches.get(), 1);
	assert_eq!(github_project_preview_fetches.get(), 1);

	// Act
	screen
		.get_by_role(Role::Button, "Create deployment")
		.click();
	screen.settle().await;

	// Assert
	assert_eq!(deployment_fetches.get(), 2);
	assert_eq!(preview_fetches.get(), 2);
	assert_eq!(first_log_fetches.get(), 2);
	assert_eq!(second_log_fetches.get(), 2);
	assert_eq!(github_project_preview_fetches.get(), 1);
}

#[tokio::test]
async fn identical_deployment_list_observers_share_one_fetch() {
	// Arrange
	let fetches = Rc::new(Cell::new(0));
	let screen = render({
		let fetches = Rc::clone(&fetches);
		move || shared_deployment_list_query_probe(fetches)
	});

	// Act
	screen.settle().await;

	// Assert
	assert_eq!(screen.pretty(), "Shared deployment list loaded\n");
	assert_eq!(fetches.get(), 1);
}

#[tokio::test]
async fn successful_deployment_delete_invalidates_github_project_preview_queries() {
	// Arrange
	let fetches = Rc::new(Cell::new(0));
	let screen = render({
		let fetches = Rc::clone(&fetches);
		move || github_project_preview_delete_invalidation_probe(fetches)
	});
	screen.settle().await;
	assert_eq!(fetches.get(), 1);

	// Act: this callback is reached only after a successful deletion.
	screen
		.get_by_role(Role::Button, "Delete deployment")
		.click();
	screen.settle().await;

	// Assert
	assert_eq!(fetches.get(), 2);
}
