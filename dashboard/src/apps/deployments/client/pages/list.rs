//! Deployments list and CRUD page.

use std::collections::HashMap;
use std::hash::Hash;

use reinhardt::pages::component;
use reinhardt::pages::component::Page;
use reinhardt::pages::event::{ClickEvent, SubmitEvent};
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Callback, FieldError, QueryClient, QueryHandle, QueryOptions, QuerySnapshot, QueryStatus,
	RouterHandle, ServerMutation, Signal, UseFormReturn, queries, use_form, use_query, use_router,
	use_server_mutation,
};
use reinhardt::pages::router::Query;
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::clusters::server_fn::{ClusterInfo, list_clusters_for_current_org};
#[cfg(wasm)]
use crate::apps::deployments::client::components::log_viewer::log_viewer_container;
use crate::apps::deployments::client::components::preview_list::{
	render_preview_list, render_project_identity,
};
use crate::apps::deployments::client::style::STYLES;
use crate::apps::deployments::server_fn::{
	CreateDeploymentFormRequestClientForm, CreateDeploymentFormRequestClientFormField,
	DeploymentInfo, ProjectPreviewSummary, UpdateDeploymentFormRequest,
	UpdateDeploymentFormRequestClientForm, UpdateDeploymentFormRequestClientFormField,
	UpdateDeploymentStatusFormRequestClientForm, UpdateDeploymentStatusFormRequestClientFormField,
	delete_deployment_for_current_org, deployment_logs_for_current_org,
	list_deployment_previews_for_current_org, list_deployments_for_current_org,
};
use crate::apps::github::server_fn::list_github_project_previews_for_current_org;
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};
use crate::shared::client::components::status_badge;
use crate::shared::client::routes::route_href;
use crate::shared::client::style::STYLES as SHARED_STYLES;
#[cfg(wasm)]
use crate::shared::client::ws::track_subscriptions;
use crate::shared::client::ws::{subscribe_app_logs, unsubscribe_logs};
use crate::shared::ws_messages::DeploymentState;

fn state_from_status(status: &str) -> DeploymentState {
	match status {
		"running" | "succeeded" => DeploymentState::Running,
		"failed" => DeploymentState::Failed,
		"degraded" => DeploymentState::Degraded,
		"stopped" => DeploymentState::Stopped,
		_ => DeploymentState::Deploying,
	}
}

fn alert(error: Signal<Option<String>>) -> Page {
	Page::reactive(move || {
		error
			.get()
			.map(|message| {
				page!({
					div {
						class: STYLES.alert() + STYLES.alert_error(),
						{ message }
					}
				})
			})
			.unwrap_or(Page::Empty)
	})
}

fn success_alert(message: Signal<Option<String>>) -> Page {
	Page::reactive(move || {
		message
			.get()
			.map(|message| {
				page!({
					div {
						class: STYLES.alert() + STYLES.alert_success(),
						{ message }
					}
				})
			})
			.unwrap_or(Page::Empty)
	})
}

fn form_field_error<Field>(field_errors: Signal<HashMap<Field, FieldError>>, field: Field) -> Page
where
	Field: Copy + Eq + Hash + 'static,
{
	Page::reactive(move || {
		field_errors
			.get()
			.get(&field)
			.map(|error| {
				let message = error.message().to_owned();
				page!({
					p {
						class: STYLES.field_error(),
						{ message }
					}
				})
			})
			.unwrap_or(Page::Empty)
	})
}

fn deployment_log_selection_from_value(value: &str) -> Option<i64> {
	value
		.parse::<i64>()
		.ok()
		.filter(|deployment_id| *deployment_id > 0)
}

pub(crate) fn deployment_logs_path(deployments_href: &str, deployment_id: Option<i64>) -> String {
	deployment_id.map_or_else(
		|| deployments_href.to_owned(),
		|deployment_id| format!("{deployments_href}?logs={deployment_id}"),
	)
}

pub(crate) fn selected_log_deployment_id(deployment_id: Option<i64>) -> String {
	deployment_id
		.filter(|deployment_id| *deployment_id > 0)
		.map_or_else(String::new, |deployment_id| deployment_id.to_string())
}

fn synchronize_live_log_subscription(deployment_id: Option<i64>) {
	let deployment_id = selected_log_deployment_id(deployment_id);
	if deployment_id.is_empty() {
		unsubscribe_logs();
	} else {
		subscribe_app_logs(&deployment_id);
	}
}

fn render_live_log_selector(
	options: Vec<EntitySelectOption>,
	selected_deployment_id: Signal<String>,
	router: RouterHandle,
	deployments_href: String,
) -> Page {
	entity_select(
		"Deployment",
		"Select deployment",
		options,
		selected_deployment_id,
		move |value| {
			let selected = deployment_log_selection_from_value(&value);
			let path = deployment_logs_path(&deployments_href, selected);
			let _ = router.replace(path);
		},
	)
}

#[derive(Clone)]
struct CreateDeploymentFormView {
	runtime: UseFormReturn<CreateDeploymentFormRequestClientForm>,
	submit: Callback<SubmitEvent, ()>,
	success: Signal<Option<String>>,
}

fn render_create_deployment_form(view: CreateDeploymentFormView) -> Page {
	let CreateDeploymentFormView {
		runtime,
		submit,
		success,
	} = view;
	let state = runtime.form_state();
	let success_view = success_alert(success);
	let error_view = alert(state.form_error);
	let project_name_error = form_field_error(
		state.field_errors,
		CreateDeploymentFormRequestClientFormField::ProjectName,
	);
	let cluster_error = form_field_error(
		state.field_errors,
		CreateDeploymentFormRequestClientFormField::ClusterId,
	);
	let image_error = form_field_error(
		state.field_errors,
		CreateDeploymentFormRequestClientFormField::Image,
	);
	let project_yaml_error = form_field_error(
		state.field_errors,
		CreateDeploymentFormRequestClientFormField::ProjectYaml,
	);

	Page::reactive(move || {
		let is_submitting = state.is_submitting.get();
		let submit_status = if is_submitting {
			page!({
				p {
					class: STYLES.form_status(),
					"Submitting..."
				}
			})
		} else {
			Page::Empty
		};
		page!({
			{ success_view }
			{ error_view }
			form {
				class: SHARED_STYLES.form_grid() + STYLES.form_margin(),
				@submit: submit,
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Project name"
						}
						input {
							id: "create-deployment-project-name",
							aria_label: "Project name",
							class: SHARED_STYLES.input(),
							type: "text",
							maxlength: 63,
							placeholder: "web",
							bind: runtime.field(CreateDeploymentFormRequestClientFormField::ProjectName),
						}
					}
					{ project_name_error }
				}
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Cluster"
						}
						input {
							id: "create-deployment-cluster-id",
							aria_label: "Cluster",
							class: SHARED_STYLES.input(),
							type: "text",
							readonly: true,
							bind: runtime.field(CreateDeploymentFormRequestClientFormField::ClusterId),
						}
					}
					{ cluster_error }
				}
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Image"
						}
						input {
							id: "create-deployment-image",
							aria_label: "Image",
							class: SHARED_STYLES.input(),
							type: "text",
							maxlength: 512,
							placeholder: "ghcr.io/example/web:latest",
							bind: runtime.field(CreateDeploymentFormRequestClientFormField::Image),
						}
					}
					{ image_error }
				}
				div {
					class: SHARED_STYLES.field(),
					label {
						id: "create-deployment-project-yaml-label",
						span {
							class: SHARED_STYLES.label(),
							"Project YAML"
						}
					}
					textarea {
						id: "create-deployment-project-yaml",
						aria_labelledby: "create-deployment-project-yaml-label",
						class: SHARED_STYLES.input() + SHARED_STYLES.textarea(),
						maxlength: 65535,
						bind: runtime.field(CreateDeploymentFormRequestClientFormField::ProjectYaml),
					}
					{ project_yaml_error }
				}
				button {
					type: "submit",
					class: SHARED_STYLES.button_primary() + STYLES.form_submit() + STYLES.form_submit_create(),
					disabled: is_submitting,
					"Create deployment"
				}
			}
			{ submit_status }
		})
	})
}

#[derive(Clone)]
struct UpdateDeploymentFormView {
	runtime: UseFormReturn<UpdateDeploymentFormRequestClientForm>,
	submit: Callback<SubmitEvent, ()>,
	success: Signal<Option<String>>,
}

fn render_update_deployment_form(view: UpdateDeploymentFormView) -> Page {
	let UpdateDeploymentFormView {
		runtime,
		submit,
		success,
	} = view;
	let state = runtime.form_state();
	let success_view = success_alert(success);
	let error_view = alert(state.form_error);
	let project_name_error = form_field_error(
		state.field_errors,
		UpdateDeploymentFormRequestClientFormField::ProjectName,
	);
	let image_error = form_field_error(
		state.field_errors,
		UpdateDeploymentFormRequestClientFormField::Image,
	);
	let status_error = form_field_error(
		state.field_errors,
		UpdateDeploymentFormRequestClientFormField::Status,
	);

	Page::reactive(move || {
		let is_submitting = state.is_submitting.get();
		let dirty_notice = if state.is_dirty.get() {
			page!({
				p {
					class: STYLES.dirty_notice(),
					"Unsaved changes"
				}
			})
		} else {
			Page::Empty
		};
		let submit_status = if is_submitting {
			page!({
				p {
					class: STYLES.form_status(),
					"Updating..."
				}
			})
		} else {
			Page::Empty
		};
		page!({
			{ success_view }
			{ error_view }
			form {
				class: SHARED_STYLES.form_stack() + STYLES.form_margin(),
				@submit: submit,
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Project name"
						}
						input {
							id: "update-deployment-project-name",
							aria_label: "Project name",
							class: SHARED_STYLES.input(),
							type: "text",
							maxlength: 63,
							bind: runtime.field(UpdateDeploymentFormRequestClientFormField::ProjectName),
						}
					}
					{ project_name_error }
				}
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Image"
						}
						input {
							id: "update-deployment-image",
							aria_label: "Image",
							class: SHARED_STYLES.input(),
							type: "text",
							maxlength: 512,
							bind: runtime.field(UpdateDeploymentFormRequestClientFormField::Image),
						}
					}
					{ image_error }
				}
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Status"
						}
						input {
							id: "update-deployment-status",
							aria_label: "Status",
							class: SHARED_STYLES.input(),
							type: "text",
							maxlength: 50,
							bind: runtime.field(UpdateDeploymentFormRequestClientFormField::Status),
						}
					}
					{ status_error }
				}
				button {
					type: "submit",
					class: SHARED_STYLES.button_dark() + STYLES.form_submit(),
					disabled: is_submitting,
					"Update deployment"
				}
			}
			{ dirty_notice }
			{ submit_status }
		})
	})
}

#[derive(Clone)]
struct UpdateDeploymentStatusFormView {
	runtime: UseFormReturn<UpdateDeploymentStatusFormRequestClientForm>,
	submit: Callback<SubmitEvent, ()>,
	success: Signal<Option<String>>,
}

fn render_update_deployment_status_form(view: UpdateDeploymentStatusFormView) -> Page {
	let UpdateDeploymentStatusFormView {
		runtime,
		submit,
		success,
	} = view;
	let state = runtime.form_state();
	let success_view = success_alert(success);
	let error_view = alert(state.form_error);
	let status_error = form_field_error(
		state.field_errors,
		UpdateDeploymentStatusFormRequestClientFormField::Status,
	);

	Page::reactive(move || {
		let is_submitting = state.is_submitting.get();
		let submit_status = if is_submitting {
			page!({
				p {
					class: STYLES.form_status(),
					"Updating..."
				}
			})
		} else {
			Page::Empty
		};
		page!({
			{ success_view }
			{ error_view }
			form {
				class: SHARED_STYLES.form_stack() + STYLES.form_margin(),
				@submit: submit,
				div {
					class: SHARED_STYLES.field(),
					label {
						span {
							class: SHARED_STYLES.label(),
							"Status"
						}
						input {
							id: "update-deployment-status-only",
							aria_label: "Status",
							class: SHARED_STYLES.input(),
							type: "text",
							maxlength: 50,
							placeholder: "running",
							bind: runtime.field(UpdateDeploymentStatusFormRequestClientFormField::Status),
						}
					}
					{ status_error }
				}
				button {
					type: "submit",
					class: SHARED_STYLES.button_warning() + STYLES.form_submit(),
					disabled: is_submitting,
					"Set status"
				}
			}
			{ submit_status }
		})
	})
}

#[derive(Clone)]
struct DeleteDeploymentActionView {
	action: ServerMutation<(), ()>,
	error: Signal<Option<String>>,
	success: Signal<Option<String>>,
	confirmed: Signal<bool>,
}

fn render_delete_deployment_action(view: DeleteDeploymentActionView) -> Page {
	let DeleteDeploymentActionView {
		action,
		error,
		success,
		confirmed,
	} = view;
	let delete = Callback::new(move |_event: ClickEvent| {
		action.dispatch(());
	});
	let success_view = success_alert(success);
	let error_view = alert(error);
	Page::reactive(move || {
		let is_pending = action.is_pending();
		let is_confirmed = confirmed.get();
		page!({
			{ success_view }
			{ error_view }
			div {
				class: SHARED_STYLES.form_stack() + STYLES.form_margin(),
				label {
					class: STYLES.delete_confirmation(),
					input {
						id: "confirm-deployment-delete",
						type: "checkbox",
						bind: confirmed,
					}
					span { "I understand this permanently deletes the selected deployment." }
				}
				button {
					type: "button",
					class: SHARED_STYLES.button_danger() + STYLES.form_submit(),
					disabled: !is_confirmed || is_pending,
					@click: delete,
					"Delete deployment"
				} {
					if is_pending {
						page!( {
							p {
								class: STYLES.form_status(),
								"Deleting..."
							}
						})
					} else { Page::Empty }
				}
			}
		})
	})
}

fn query_error_message(error: Option<ServerFnError>, fallback: &'static str) -> String {
	error
		.map(|error| error.user_message().to_owned())
		.unwrap_or_else(|| fallback.to_owned())
}

fn query_refetch_notice(
	is_fetching: bool,
	refetch_error: Option<ServerFnError>,
	label: &'static str,
) -> Page {
	if let Some(error) = refetch_error {
		let message = format!(
			"Showing cached {label}; the latest refresh failed: {}",
			error.user_message()
		);
		return page!({
			div {
				class: STYLES.refetch_notice() + STYLES.refetch_warning(),
				{ message }
			}
		});
	}
	if is_fetching {
		return page!({
			div {
				class: STYLES.refetch_notice() + STYLES.refetch_pending(),
				"Refreshing " { label }"..."
			}
		});
	}
	Page::Empty
}

pub(crate) fn invalidate_deployment_queries(query_client: &QueryClient) {
	query_client.invalidate(&list_deployments_for_current_org::key());
	query_client.invalidate(&list_deployment_previews_for_current_org::key());
	query_client.invalidate_family(deployment_logs_for_current_org::family());
}

pub(crate) fn invalidate_deployment_delete_queries(query_client: &QueryClient) {
	self::invalidate_deployment_queries(query_client);
	query_client.invalidate_family(list_github_project_previews_for_current_org::family());
}

#[cfg(wasm)]
fn track_visible_deployments(items: &[DeploymentInfo]) {
	let ids = items
		.iter()
		.map(|item| item.id.to_string())
		.collect::<Vec<_>>();
	track_subscriptions(&ids);
}

#[cfg(not(wasm))]
fn track_visible_deployments(_items: &[DeploymentInfo]) {}

fn cluster_select_options(items: &[ClusterInfo]) -> Vec<EntitySelectOption> {
	items
		.iter()
		.map(|cluster| {
			EntitySelectOption::new(
				cluster.id.to_string(),
				cluster.name.clone(),
				Some(cluster.api_url.clone()),
			)
		})
		.collect()
}

fn deployment_select_options(items: &[DeploymentInfo]) -> Vec<EntitySelectOption> {
	items
		.iter()
		.map(|deployment| {
			EntitySelectOption::new(
				deployment.id.to_string(),
				deployment.project_name.clone(),
				Some(format!("{} / {}", deployment.status, deployment.image)),
			)
		})
		.collect()
}

fn render_deployment_project_cell(
	deployment: &DeploymentInfo,
	summary: Option<&ProjectPreviewSummary>,
) -> Page {
	if let Some(summary) = summary {
		let identity = render_project_identity(summary);
		let previews = render_preview_list(summary);
		return page!({
			div {
				{ identity }
				{ previews }
			}
		});
	}
	let project_name = deployment.project_name.clone();
	page!({
		div {
			div {
				class: STYLES.project_name(),
				{ project_name }
			}
			div {
				class: STYLES.project_empty(),
				"No active previews"
			}
		}
	})
}

fn render_deployment_status_badge(status: &str) -> Page {
	let state = state_from_status(status);
	let (_, label) = status_badge::badge_style(&state);
	page!({
		span {
			class: SHARED_STYLES.status_badge() + deployment_status_token(&state),
			{ label }
		}
	})
}

fn deployment_status_token(state: &DeploymentState) -> reinhardt::pages::prelude::ClassToken {
	match state {
		DeploymentState::Running => SHARED_STYLES.status_running(),
		DeploymentState::Deploying => SHARED_STYLES.status_deploying(),
		DeploymentState::Degraded => SHARED_STYLES.status_degraded(),
		DeploymentState::Failed => SHARED_STYLES.status_failed(),
		DeploymentState::Stopped => SHARED_STYLES.status_stopped(),
	}
}

fn render_neutral_refetch_notice(message: &'static str) -> Page {
	page!({
		div {
			class: STYLES.refetch_notice() + STYLES.refetch_neutral(),
			{ message }
		}
	})
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::{
		deployment_status_token, render_deployment_status_badge, render_neutral_refetch_notice,
	};
	use crate::apps::deployments::client::style::STYLES;
	use crate::shared::client::style::STYLES as SHARED_STYLES;
	use crate::shared::ws_messages::DeploymentState;

	#[cfg(native)]
	#[rstest::rstest]
	fn native_deployment_form_mutations_preserve_state_without_dispatch() {
		use crate::apps::deployments::server_fn::{
			CreateDeploymentFormRequestClientForm, CreateDeploymentFormRequestClientFormField,
			UpdateDeploymentFormRequestClientForm, UpdateDeploymentStatusFormRequestClientForm,
		};
		use reinhardt::pages::prelude::{MutationDispatchOutcome, use_form};
		use reinhardt::pages::reactive::ReactiveScope;

		ReactiveScope::run(|| {
			// Arrange
			let create = CreateDeploymentFormRequestClientForm::new();
			let runtime = use_form(&create).build();
			runtime.set_value(
				CreateDeploymentFormRequestClientFormField::ProjectName,
				"web".to_owned(),
			);
			let create_mutation = create
				.server_mutation(&runtime)
				.reset_form_on_success()
				.build();
			let update = UpdateDeploymentFormRequestClientForm::new();
			let update_runtime = use_form(&update).build();
			let update_mutation = update
				.server_mutation(&update_runtime)
				.reset_form_on_success()
				.build();
			let status = UpdateDeploymentStatusFormRequestClientForm::new();
			let status_runtime = use_form(&status).build();
			let status_mutation = status
				.server_mutation(&status_runtime)
				.reset_form_on_success()
				.build();

			// Act
			let outcomes = [
				create_mutation.dispatch(),
				update_mutation.dispatch(),
				status_mutation.dispatch(),
			];

			// Assert
			assert_eq!(outcomes, [MutationDispatchOutcome::UnsupportedTarget; 3]);
			assert_eq!(runtime.form_state().is_submitting.get(), false);
			assert_eq!(runtime.form_state().field_errors.get().len(), 0);
			assert_eq!(
				CreateDeploymentFormRequestClientForm::to_request(&runtime).project_name,
				"web"
			);
		});
	}

	#[rstest]
	fn deployment_status_badge_composes_shared_base_and_state_tokens() {
		// Act
		let html = render_deployment_status_badge("running").render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<span class=\"{} {}\">Running</span>",
				SHARED_STYLES.status_badge().as_str(),
				SHARED_STYLES.status_running().as_str(),
			)
		);
	}

	#[rstest]
	fn deployment_status_token_remains_typed_until_badge_composition() {
		// Act
		let token = deployment_status_token(&DeploymentState::Running);

		// Assert
		assert_eq!(token.as_str(), SHARED_STYLES.status_running().as_str());
	}

	#[rstest]
	fn operation_query_states_keep_local_text_size_and_distinct_colors() {
		// Arrange
		let source = include_str!("list.rs");
		let production_source = source
			.split_once("\npub fn deployments_list_page")
			.expect("the deployment list source has its page component")
			.1;

		// Act
		let idle = STYLES.operation_state() + STYLES.operation_idle();
		let pending = STYLES.operation_state() + STYLES.operation_pending();

		// Assert
		assert_eq!(
			idle.as_str(),
			format!(
				"{} {}",
				STYLES.operation_state().as_str(),
				STYLES.operation_idle().as_str(),
			)
		);
		assert_eq!(
			pending.as_str(),
			format!(
				"{} {}",
				STYLES.operation_state().as_str(),
				STYLES.operation_pending().as_str(),
			)
		);
		assert_eq!(
			production_source
				.matches("class: STYLES.operation_state() + STYLES.operation_idle(),")
				.count(),
			5
		);
		assert_eq!(
			production_source
				.matches("class: STYLES.operation_state() + STYLES.operation_pending(),")
				.count(),
			5
		);
		assert!(!production_source.contains("SHARED_STYLES.muted() + STYLES.operation_state()"));
	}

	#[rstest]
	fn page_layout_token_is_used_with_desktop_columns_contract() {
		// Arrange
		let source = include_str!("list.rs");
		let page_source = source
			.split_once("\npub fn deployments_list_page")
			.expect("the deployment list source has its page component")
			.1;
		let style_source = include_str!("../style.rs");

		// Assert
		assert!(!STYLES.page_layout().as_str().is_empty());
		assert!(page_source.contains("class: STYLES.page_layout(),"));
		assert!(style_source.contains(
			".page_layout {\n\t\tdisplay: grid;\n\t\tgap: 1.5rem;\n\t\t@media (min-width: 1024px) {\n\t\t\tgrid-template-columns: (1fr, 20rem);\n\t\t}\n\t}"
		));
	}

	#[rstest]
	fn neutral_preview_notice_renders_composed_generated_tokens() {
		// Act
		let html = render_neutral_refetch_notice("Loading previews...").render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<div class=\"{}\">Loading previews...</div>",
				(STYLES.refetch_notice() + STYLES.refetch_neutral()).as_str(),
			)
		);
	}
}

fn render_deployment_inventory_row(
	deployment: &DeploymentInfo,
	summary: Option<&ProjectPreviewSummary>,
) -> Page {
	let deployment = deployment.clone();
	let project_cell = render_deployment_project_cell(&deployment, summary);
	let status_cell = render_deployment_status_badge(&deployment.status);
	page!({
		tr {
			class: STYLES.inventory_row(),
			data_deployment_id: deployment.id.to_string(),
			td {
				class: SHARED_STYLES.table_cell() + STYLES.inventory_id(),
				{ deployment.id.to_string() }
			}
			td {
				class: SHARED_STYLES.table_cell(),
				{ project_cell }
			}
			td {
				class: SHARED_STYLES.table_cell() + STYLES.inventory_id(),
				{ deployment.cluster_id.to_string() }
			}
			td {
				class: SHARED_STYLES.table_cell(),
				{ status_cell }
			}
			td {
				class: SHARED_STYLES.table_cell() + STYLES.inventory_image(),
				{ deployment.image }
			}
		}
	})
}

fn render_deployment_inventory_table(
	items: Vec<DeploymentInfo>,
	preview_state: QuerySnapshot<Vec<ProjectPreviewSummary>, ServerFnError>,
) -> Page {
	if items.is_empty() {
		return page!({
			div {
				class: SHARED_STYLES.empty(),
				"No deployments created."
			}
		});
	}

	let (preview_banner, summaries) = match preview_state.status {
		QueryStatus::Idle => (
			render_neutral_refetch_notice(
				"Preview status is not available during server rendering.",
			),
			Vec::new(),
		),
		QueryStatus::Pending => (
			render_neutral_refetch_notice("Loading previews..."),
			Vec::new(),
		),
		QueryStatus::Error => {
			let message = query_error_message(
				preview_state.error,
				"Preview status is temporarily unavailable.",
			);
			(
				page!({
					div {
						class: STYLES.refetch_notice() + STYLES.refetch_warning(),
						{ message }
					}
				}),
				Vec::new(),
			)
		}
		QueryStatus::Success => (
			query_refetch_notice(
				preview_state.is_fetching,
				preview_state.refetch_error,
				"preview data",
			),
			preview_state.data.unwrap_or_default(),
		),
	};
	let previews_by_deployment = summaries
		.into_iter()
		.map(|summary| (summary.deployment_id, summary))
		.collect::<std::collections::HashMap<_, _>>();
	let rows = items
		.iter()
		.map(|deployment| {
			self::render_deployment_inventory_row(
				deployment,
				previews_by_deployment.get(&deployment.id),
			)
		})
		.collect::<Vec<_>>();

	page!({
		{ preview_banner }
		div {
			class: STYLES.inventory_scroll(),
			table {
				class: SHARED_STYLES.table(),
				thead {
					class: STYLES.inventory_head(),
					tr {
						th {
							class: SHARED_STYLES.table_header(),
							"ID"
						}
						th {
							class: SHARED_STYLES.table_header(),
							"Project"
						}
						th {
							class: SHARED_STYLES.table_header(),
							"Cluster"
						}
						th {
							class: SHARED_STYLES.table_header(),
							"Status"
						}
						th {
							class: SHARED_STYLES.table_header(),
							"Image"
						}
					}
				}
				tbody {
					class: STYLES.inventory_body(),
					{ rows }
				}
			}
		}
	})
}

#[derive(Clone)]
struct DeploymentsListPageViewProps {
	deployments_for_inventory: QueryHandle<Vec<DeploymentInfo>, ServerFnError>,
	deployments_for_logs: QueryHandle<Vec<DeploymentInfo>, ServerFnError>,
	deployments_for_edit: QueryHandle<Vec<DeploymentInfo>, ServerFnError>,
	deployments_for_status: QueryHandle<Vec<DeploymentInfo>, ServerFnError>,
	deployments_for_delete: QueryHandle<Vec<DeploymentInfo>, ServerFnError>,
	deployments_for_previews: QueryHandle<Vec<ProjectPreviewSummary>, ServerFnError>,
	clusters_for_create: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	create_view: Page,
	create_cluster_id: Signal<String>,
	edit_view: Page,
	edit_deployment_id: Signal<String>,
	edit_project_name: Signal<String>,
	edit_image: Signal<String>,
	edit_status: Signal<String>,
	status_view: Page,
	status_deployment_id: Signal<String>,
	delete_view: Page,
	delete_deployment_id: Signal<String>,
	log_deployment_id: Signal<String>,
	log_router: RouterHandle,
	deployments_href: String,
	logs: Page,
}

/// Render the deployments page.
#[component("deployments", name = "deployments:list")]
pub fn deployments_list_page(Query(logs): Query<Option<i64>>) -> Page {
	let deployments = use_query(
		list_deployments_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let deployment_previews = use_query(
		list_deployment_previews_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let clusters = use_query(
		list_clusters_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let query_client = queries();

	let create_form = CreateDeploymentFormRequestClientForm::new();
	let create_success = Signal::new(None::<String>);
	let create_query_client = query_client.clone();
	let create_success_callback = create_success;
	let create_runtime = use_form(&create_form)
		.on_submit_success(move |_| {
			self::invalidate_deployment_queries(&create_query_client);
			create_success_callback.set(Some("Deployment created.".to_owned()));
		})
		.build();
	let create_cluster_id = create_runtime.watch_field::<String>(create_form.cluster_id_field());
	let create_action = create_form
		.server_mutation(&create_runtime)
		.reset_form_on_success()
		.build();
	let create_submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		CreateDeploymentFormRequestClientForm::normalize_values(&create_action.form());
		create_action.dispatch();
	});
	let create_view = self::render_create_deployment_form(CreateDeploymentFormView {
		runtime: create_runtime,
		submit: create_submit,
		success: create_success,
	});

	let edit_form =
		UpdateDeploymentFormRequestClientForm::new().with_defaults(UpdateDeploymentFormRequest {
			deployment_id: String::new(),
			project_name: String::new(),
			image: String::new(),
			status: "pending".to_owned(),
		});
	let edit_success = Signal::new(None::<String>);
	let edit_query_client = query_client.clone();
	let edit_success_callback = edit_success;
	let edit_runtime = use_form(&edit_form)
		.on_submit_success(move |_| {
			self::invalidate_deployment_queries(&edit_query_client);
			edit_success_callback.set(Some("Deployment updated.".to_owned()));
		})
		.build();
	let edit_deployment_id = edit_runtime.watch_field::<String>(edit_form.deployment_id_field());
	let edit_project_name = edit_runtime.watch_field::<String>(edit_form.project_name_field());
	let edit_image = edit_runtime.watch_field::<String>(edit_form.image_field());
	let edit_status = edit_runtime.watch_field::<String>(edit_form.status_field());
	let edit_action = edit_form
		.server_mutation(&edit_runtime)
		.reset_form_on_success()
		.build();
	let edit_submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		UpdateDeploymentFormRequestClientForm::normalize_values(&edit_action.form());
		edit_action.dispatch();
	});
	let edit_view = self::render_update_deployment_form(UpdateDeploymentFormView {
		runtime: edit_runtime,
		submit: edit_submit,
		success: edit_success,
	});

	let status_form = UpdateDeploymentStatusFormRequestClientForm::new();
	let status_success = Signal::new(None::<String>);
	let status_query_client = query_client.clone();
	let status_success_callback = status_success;
	let status_runtime = use_form(&status_form)
		.on_submit_success(move |_| {
			self::invalidate_deployment_queries(&status_query_client);
			status_success_callback.set(Some("Deployment status updated.".to_owned()));
		})
		.build();
	let status_deployment_id =
		status_runtime.watch_field::<String>(status_form.deployment_id_field());
	let status_action = status_form
		.server_mutation(&status_runtime)
		.reset_form_on_success()
		.build();
	let status_submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		UpdateDeploymentStatusFormRequestClientForm::normalize_values(&status_action.form());
		status_action.dispatch();
	});
	let status_view = self::render_update_deployment_status_form(UpdateDeploymentStatusFormView {
		runtime: status_runtime,
		submit: status_submit,
		success: status_success,
	});

	let delete_deployment_id = Signal::new(String::new());
	let delete_confirmed = Signal::new(false);
	let delete_error = Signal::new(None::<String>);
	let delete_success = Signal::new(None::<String>);
	let deployments_href = route_href("deployments:list", "/deployments");
	let log_deployment_id = Signal::new(self::selected_log_deployment_id(logs));
	self::synchronize_live_log_subscription(logs);
	let log_router = use_router();
	let delete_query_client = query_client.clone();
	let delete_deployment_id_for_action = delete_deployment_id;
	let delete_confirmed_for_action = delete_confirmed;
	let delete_error_for_action = delete_error;
	let delete_error_for_callback = delete_error;
	let delete_success_for_callback = delete_success;
	let delete_deployment_id_for_callback = delete_deployment_id;
	let delete_confirmed_for_callback = delete_confirmed;
	let log_deployment_id_for_callback = log_deployment_id;
	let deployments_href_for_callback = deployments_href.clone();
	let delete_action = use_server_mutation(move |(): ()| {
		delete_error_for_action.set(None);
		let deployment_id = delete_deployment_id_for_action.get();
		let confirmed = delete_confirmed_for_action.get();
		async move {
			if !confirmed {
				return Err(ServerFnError::application(
					"Confirm deletion before continuing",
				));
			}
			if deployment_id.trim().is_empty() {
				return Err(ServerFnError::application(
					"Select a deployment before deleting",
				));
			}
			delete_deployment_for_current_org::mutation()(deployment_id).await
		}
	})
	.on_success(move |_| {
		let deleted_deployment_id = delete_deployment_id_for_callback.get();
		self::invalidate_deployment_delete_queries(&delete_query_client);
		delete_deployment_id_for_callback.set(String::new());
		delete_confirmed_for_callback.set(false);
		delete_success_for_callback.set(Some("Deployment deleted.".to_owned()));
		if log_deployment_id_for_callback.get() == deleted_deployment_id {
			let _ = log_router.replace(deployments_href_for_callback.clone());
		}
	})
	.on_error(move |error| {
		delete_error_for_callback.set(Some(error.user_message().to_owned()));
	})
	.build();
	let delete_view = self::render_delete_deployment_action(DeleteDeploymentActionView {
		action: delete_action,
		error: delete_error,
		success: delete_success,
		confirmed: delete_confirmed,
	});

	#[cfg(wasm)]
	let logs = log_viewer_container(log_deployment_id);
	#[cfg(not(wasm))]
	let logs = Page::Empty;
	let deployments_for_inventory = deployments.clone();
	let deployments_for_logs = deployments.clone();
	let deployments_for_edit = deployments.clone();
	let deployments_for_status = deployments.clone();
	let deployments_for_delete = deployments.clone();
	let deployments_for_previews = deployment_previews.clone();
	let clusters_for_create = clusters.clone();

	let props = DeploymentsListPageViewProps {
		deployments_for_inventory,
		deployments_for_logs,
		deployments_for_edit,
		deployments_for_status,
		deployments_for_delete,
		deployments_for_previews,
		clusters_for_create,
		create_view,
		create_cluster_id,
		edit_view,
		edit_deployment_id,
		edit_project_name,
		edit_image,
		edit_status,
		status_view,
		status_deployment_id,
		delete_view,
		delete_deployment_id,
		log_deployment_id,
		log_router,
		deployments_href,
		logs,
	};

	page!({
		div {
			class: SHARED_STYLES.shell(),
				div {
					class: SHARED_STYLES.topline(),
					div {
						p {
							class: SHARED_STYLES.kicker(),
							"Release surface"
						}
						h1 {
							class: SHARED_STYLES.title(),
							"Deployments"
						}
						p {
							class: SHARED_STYLES.muted() + STYLES.intro(),
							"Applications deployed through Reinhardt Cloud."
						}
					}
				}
				div {
					class: STYLES.page_layout(),
					div {
						class: STYLES.content_stack(),
						section {
							class: SHARED_STYLES.panel(),
								div {
									class: SHARED_STYLES.panel_head(),
									"Deployment Inventory"
								} {
									let snapshot = props.deployments_for_inventory.snapshot();
									match snapshot.status {
										QueryStatus::Idle => page!({
											div {
													class: SHARED_STYLES.empty(),
												"Deployments are not available during server rendering."
											}
										}),
										QueryStatus::Pending => page!({
											div {
													class: SHARED_STYLES.empty(),
												"Loading deployments..."
											}
										}),
										QueryStatus::Error => {
											let message = self::query_error_message(
												snapshot.error,
												"Deployments are temporarily unavailable.",
											);
											page!({
											div {
											class: STYLES.query_error(),
												{ message }
											}
											})
										}
										QueryStatus::Success => {
											let items = snapshot.data.unwrap_or_default();
											self::track_visible_deployments(&items);
											let warning = self::query_refetch_notice(
												snapshot.is_fetching,
												snapshot.refetch_error,
												"deployments",
											);
											let inventory = self::render_deployment_inventory_table(
												items,
												props.deployments_for_previews.snapshot(),
											);
											page!({
												{ warning }
												{ inventory }
											})
										}
									}
								}
						}
						section {
							class: SHARED_STYLES.panel_pad(),
							h2 {
								class: STYLES.section_title(),
								"Create Deployment"
								}
								{
									let snapshot = props.clusters_for_create.snapshot();
									match snapshot.status {
										QueryStatus::Success => {
											let items = snapshot.data.unwrap_or_default();
											let warning = self::query_refetch_notice(
												snapshot.is_fetching,
												snapshot.refetch_error,
												"clusters",
											);
											let selector = self::entity_select(
												"Cluster",
												"Select target cluster",
												self::cluster_select_options(&items),
												props.create_cluster_id,
												|_value| {},
											);
											page!({
												{ warning }
												{ selector }
											})
										}
										QueryStatus::Idle => page!({
												p {
													class: STYLES.operation_state() + STYLES.operation_idle(),
												"Clusters are not available during server rendering."
											}
										}),
										QueryStatus::Pending => page!({
												p {
													class: STYLES.operation_state() + STYLES.operation_pending(),
												"Loading clusters..."
											}
										}),
										QueryStatus::Error => {
											let message = self::query_error_message(
												snapshot.error,
												"Clusters are temporarily unavailable.",
											);
											page!({
											p {
												class: STYLES.operation_error(),
												{ message }
											}
											})
										}
									}
								}
							{ props.create_view.clone() }
						}
						section {
							class: SHARED_STYLES.panel_pad(),
									h2 {
											class: STYLES.section_title(),
										"Live Logs"
									} {
										let snapshot = props.deployments_for_logs.snapshot();
										match snapshot.status {
										QueryStatus::Success => self::render_live_log_selector(
											self::deployment_select_options(&snapshot.data.unwrap_or_default()),
											props.log_deployment_id,
											props.log_router,
											props.deployments_href.clone(),
										),
											QueryStatus::Idle => page!({
												p {
													class: STYLES.operation_state() + STYLES.operation_idle(),
													"Deployments are not available during server rendering."
												}
											}),
											QueryStatus::Pending => page!({
												p {
													class: STYLES.operation_state() + STYLES.operation_pending(),
													"Loading deployments..."
												}
											}),
											QueryStatus::Error => {
												let message = self::query_error_message(
													snapshot.error,
													"Deployments are temporarily unavailable.",
												);
												page!({
												p {
													class: STYLES.operation_error(),
													{ message }
												}
												})
											}
										}
									}
							div {
								class: STYLES.section_gap(),
								{ props.logs.clone() }
							}
						}
					}
					aside {
						class: SHARED_STYLES.stack(),
						section {
							class: SHARED_STYLES.panel_pad(),
										h2 {
											class: STYLES.section_title(),
									"Deployment Operations"
									}
								{
									let snapshot = props.deployments_for_edit.snapshot();
									match snapshot.status {
										QueryStatus::Success => {
											let items = snapshot.data.unwrap_or_default();
											let deployments_for_change = items.clone();
										let project_name_signal = props.edit_project_name;
										let image_signal = props.edit_image;
										let status_signal = props.edit_status;
										self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&items), props.edit_deployment_id, move |value| {
											if let Some(deployment) = deployments_for_change.iter().find(|deployment| deployment.id.to_string() == value) {
												project_name_signal.set(deployment.project_name.clone());
												image_signal.set(deployment.image.clone());
												status_signal.set(deployment.status.clone());
												}
											}, )
										}
						QueryStatus::Idle => page!({
											p {
														class: STYLES.operation_state() + STYLES.operation_idle(),
												"Deployments are not available during server rendering."
											}
						}),
						QueryStatus::Pending => page!({
											p {
														class: STYLES.operation_state() + STYLES.operation_pending(),
												"Loading deployments..."
											}
						}),
						QueryStatus::Error => {
							let message = self::query_error_message(
								snapshot.error,
								"Deployments are temporarily unavailable.",
							);
							page!({
											p {
														class: STYLES.operation_error(),
												{ message }
											}
							})
						}
									}
								}
							{ props.edit_view.clone() }
							div {
							class: STYLES.divider()
								}
								{
									let snapshot = props.deployments_for_status.snapshot();
									match snapshot.status {
									QueryStatus::Success => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&snapshot.data.unwrap_or_default()), props.status_deployment_id, |_value| {}, ),
						QueryStatus::Idle => page!({
											p {
														class: STYLES.operation_state() + STYLES.operation_idle(),
												"Deployments are not available during server rendering."
											}
						}),
						QueryStatus::Pending => page!({
											p {
														class: STYLES.operation_state() + STYLES.operation_pending(),
												"Loading deployments..."
											}
						}),
						QueryStatus::Error => {
							let message = self::query_error_message(
								snapshot.error,
								"Deployments are temporarily unavailable.",
							);
							page!({
											p {
														class: STYLES.operation_error(),
												{ message }
											}
							})
						}
									}
								}
							{ props.status_view.clone() }
							div {
							class: STYLES.divider()
								}
								{
									let snapshot = props.deployments_for_delete.snapshot();
									match snapshot.status {
									QueryStatus::Success => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&snapshot.data.unwrap_or_default()), props.delete_deployment_id, |_value| {}, ),
						QueryStatus::Idle => page!({
											p {
													class: STYLES.operation_state() + STYLES.operation_idle(),
												"Deployments are not available during server rendering."
											}
						}),
						QueryStatus::Pending => page!({
											p {
													class: STYLES.operation_state() + STYLES.operation_pending(),
												"Loading deployments..."
											}
						}),
						QueryStatus::Error => {
							let message = self::query_error_message(
								snapshot.error,
								"Deployments are temporarily unavailable.",
							);
							page!({
											p {
													class: STYLES.operation_error(),
												{ message }
											}
							})
						}
									}
								}
							{ props.delete_view.clone() }
						}
					}
			}
		}
	})
}
