//! Deployments list and CRUD page.

use std::collections::HashMap;
use std::hash::Hash;

use reinhardt::pages::component;
use reinhardt::pages::component::Page;
use reinhardt::pages::event::{ClickEvent, SubmitEvent};
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Action, Callback, FieldError, FormState, QueryClient, QueryHandle, QueryOptions, QuerySnapshot,
	QueryStatus, RouterHandle, Signal, UseFormAsyncSubmitOutcome, queries, use_action, use_form,
	use_query, use_router,
};
use reinhardt::pages::router::Query;
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::clusters::server_fn::{ClusterInfo, list_clusters_for_current_org};
use crate::apps::deployments::client::components::log_viewer::log_viewer_container;
use crate::apps::deployments::client::components::preview_list::{
	render_preview_list, render_project_identity,
};
use crate::apps::deployments::server_fn::{
	CreateDeploymentFormRequest, CreateDeploymentFormRequestClientForm,
	CreateDeploymentFormRequestClientFormField, DeploymentInfo, ProjectPreviewSummary,
	UpdateDeploymentFormRequest, UpdateDeploymentFormRequestClientForm,
	UpdateDeploymentFormRequestClientFormField, UpdateDeploymentStatusFormRequest,
	UpdateDeploymentStatusFormRequestClientForm, UpdateDeploymentStatusFormRequestClientFormField,
	deployment_logs_for_current_org, list_deployment_previews_for_current_org,
	list_deployments_for_current_org,
};
#[cfg(wasm)]
use crate::apps::deployments::server_fn::{
	create_deployment_for_current_org, delete_deployment_for_current_org,
	update_deployment_for_current_org, update_deployment_status_for_current_org,
};
use crate::apps::github::server_fn::list_github_project_previews_for_current_org;
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};
use crate::shared::client::components::status_badge;
use crate::shared::client::routes::route_href;
use crate::shared::client::style::STYLES;
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
						class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700",
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
						class: "rounded-md border border-green-200 bg-green-50 px-3 py-2 text-sm font-medium text-green-800",
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
						class: "mt-1 text-xs font-medium text-red-700",
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

#[cfg(wasm)]
async fn submit_create_deployment(
	request: CreateDeploymentFormRequest,
) -> Result<DeploymentInfo, ServerFnError> {
	create_deployment_for_current_org(request).await
}

#[cfg(not(wasm))]
async fn submit_create_deployment(
	_request: CreateDeploymentFormRequest,
) -> Result<DeploymentInfo, ServerFnError> {
	Err(ServerFnError::application(
		"Deployment creation is only available in the browser client",
	))
}

#[cfg(wasm)]
async fn submit_update_deployment(
	request: UpdateDeploymentFormRequest,
) -> Result<DeploymentInfo, ServerFnError> {
	update_deployment_for_current_org(request).await
}

#[cfg(not(wasm))]
async fn submit_update_deployment(
	_request: UpdateDeploymentFormRequest,
) -> Result<DeploymentInfo, ServerFnError> {
	Err(ServerFnError::application(
		"Deployment updates are only available in the browser client",
	))
}

#[cfg(wasm)]
async fn submit_update_deployment_status(
	request: UpdateDeploymentStatusFormRequest,
) -> Result<DeploymentInfo, ServerFnError> {
	update_deployment_status_for_current_org(request).await
}

#[cfg(not(wasm))]
async fn submit_update_deployment_status(
	_request: UpdateDeploymentStatusFormRequest,
) -> Result<DeploymentInfo, ServerFnError> {
	Err(ServerFnError::application(
		"Deployment status changes are only available in the browser client",
	))
}

#[cfg(wasm)]
async fn submit_delete_deployment(deployment_id: String) -> Result<(), ServerFnError> {
	delete_deployment_for_current_org(deployment_id).await
}

#[cfg(not(wasm))]
async fn submit_delete_deployment(_deployment_id: String) -> Result<(), ServerFnError> {
	Err(ServerFnError::application(
		"Deployment deletion is only available in the browser client",
	))
}

#[derive(Clone)]
struct CreateDeploymentFormView {
	state: FormState<CreateDeploymentFormRequestClientFormField>,
	action: Action<UseFormAsyncSubmitOutcome<DeploymentInfo>, ServerFnError>,
	success: Signal<Option<String>>,
	project_name: Signal<String>,
	cluster_id: Signal<String>,
	image: Signal<String>,
	project_yaml: Signal<String>,
}

fn render_create_deployment_form(view: CreateDeploymentFormView) -> Page {
	let CreateDeploymentFormView {
		state,
		action,
		success,
		project_name,
		cluster_id,
		image,
		project_yaml,
	} = view;
	let submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		action.dispatch(());
	});
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
					class: "mt-2 text-xs text-ink-600",
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
				class: "rc-form-grid mt-3",
				@submit: submit,
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Project name"
						}
						input {
							id: "create-deployment-project-name",
							aria_label: "Project name",
							class: "rc-input",
							type: "text",
							maxlength: 63,
							placeholder: "web",
							bind: project_name,
						}
					}
					{ project_name_error }
				}
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Cluster"
						}
						input {
							id: "create-deployment-cluster-id",
							aria_label: "Cluster",
							class: "rc-input",
							type: "text",
							readonly: true,
							bind: cluster_id,
						}
					}
					{ cluster_error }
				}
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Image"
						}
						input {
							id: "create-deployment-image",
							aria_label: "Image",
							class: "rc-input",
							type: "text",
							maxlength: 512,
							placeholder: "ghcr.io/example/web:latest",
							bind: image,
						}
					}
					{ image_error }
				}
				div {
					class: "rc-field md:col-span-2",
					label {
						id: "create-deployment-project-yaml-label",
						span {
							class: "rc-label",
							"Project YAML"
						}
					}
					textarea {
						id: "create-deployment-project-yaml",
						aria_labelledby: "create-deployment-project-yaml-label",
						class: "rc-input rc-textarea",
						maxlength: 65535,
						bind: project_yaml,
					}
					{ project_yaml_error }
				}
				button {
					type: "submit",
					class: "btn-primary min-h-11 w-full md:w-auto md:justify-self-start",
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
	state: FormState<UpdateDeploymentFormRequestClientFormField>,
	action: Action<UseFormAsyncSubmitOutcome<DeploymentInfo>, ServerFnError>,
	success: Signal<Option<String>>,
	project_name: Signal<String>,
	image: Signal<String>,
	status: Signal<String>,
}

fn render_update_deployment_form(view: UpdateDeploymentFormView) -> Page {
	let UpdateDeploymentFormView {
		state,
		action,
		success,
		project_name,
		image,
		status,
	} = view;
	let submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		action.dispatch(());
	});
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
					class: "mt-2 text-xs text-amber-700",
					"Unsaved changes"
				}
			})
		} else {
			Page::Empty
		};
		let submit_status = if is_submitting {
			page!({
				p {
					class: "mt-2 text-xs text-ink-600",
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
				class: "rc-form-stack mt-3",
				@submit: submit,
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Project name"
						}
						input {
							id: "update-deployment-project-name",
							aria_label: "Project name",
							class: "rc-input",
							type: "text",
							maxlength: 63,
							bind: project_name,
						}
					}
					{ project_name_error }
				}
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Image"
						}
						input {
							id: "update-deployment-image",
							aria_label: "Image",
							class: "rc-input",
							type: "text",
							maxlength: 512,
							bind: image,
						}
					}
					{ image_error }
				}
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Status"
						}
						input {
							id: "update-deployment-status",
							aria_label: "Status",
							class: "rc-input",
							type: "text",
							maxlength: 50,
							bind: status,
						}
					}
					{ status_error }
				}
				button {
					type: "submit",
					class: "btn-dark min-h-11 w-full",
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
	state: FormState<UpdateDeploymentStatusFormRequestClientFormField>,
	action: Action<UseFormAsyncSubmitOutcome<DeploymentInfo>, ServerFnError>,
	success: Signal<Option<String>>,
	status: Signal<String>,
}

fn render_update_deployment_status_form(view: UpdateDeploymentStatusFormView) -> Page {
	let UpdateDeploymentStatusFormView {
		state,
		action,
		success,
		status,
	} = view;
	let submit = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		action.dispatch(());
	});
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
					class: "mt-2 text-xs text-ink-600",
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
				class: "rc-form-stack mt-3",
				@submit: submit,
				div {
					class: "rc-field",
					label {
						span {
							class: "rc-label",
							"Status"
						}
						input {
							id: "update-deployment-status-only",
							aria_label: "Status",
							class: "rc-input",
							type: "text",
							maxlength: 50,
							placeholder: "running",
							bind: status,
						}
					}
					{ status_error }
				}
				button {
					type: "submit",
					class: "btn-warning min-h-11 w-full",
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
	action: Action<(), ServerFnError>,
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
	let delete = Callback::new(move |_event: ClickEvent| action.dispatch(()));
	let success_view = success_alert(success);
	let error_view = alert(error);
	Page::reactive(move || {
		let is_pending = action.is_pending();
		let is_confirmed = confirmed.get();
		page!({
			{ success_view }
			{ error_view }
			div {
				class: "rc-form-stack mt-3",
				label {
					class: "flex items-start gap-2 text-sm text-ink-700",
					input {
						id: "confirm-deployment-delete",
						type: "checkbox",
						bind: confirmed,
					}
					span { "I understand this permanently deletes the selected deployment." }
				}
				button {
					type: "button",
					class: "btn-danger min-h-11 w-full",
					disabled: !is_confirmed || is_pending,
					@click: delete,
					"Delete deployment"
				} {
					if is_pending {
						page!( {
							p {
								class: "text-xs text-ink-600",
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
				class: "border-b border-amber-100 bg-amber-50 px-4 py-2 text-xs font-medium text-amber-700",
				{ message }
			}
		});
	}
	if is_fetching {
		return page!({
			div {
				class: "border-b border-cloud-100 bg-cloud-50 px-4 py-2 text-xs font-medium text-cloud-600",
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
				class: "font-semibold text-ink-950",
				{ project_name }
			}
			div {
				class: "mt-2 text-xs font-medium text-cloud-500",
				"No active previews"
			}
		}
	})
}

fn render_deployment_status_badge(status: &str) -> Page {
	let (color, label) = status_badge::badge_style(&self::state_from_status(status));
	page!({
		span {
			class: format!(
				"{} inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold {color}",
				STYLES.status_badge().as_str(),
			),
			{ label }
		}
	})
}

#[cfg(test)]
mod tests {
	use super::render_deployment_status_badge;
	use crate::shared::client::style::STYLES;

	#[test]
	fn deployment_status_badge_composes_shared_base_and_state_tokens() {
		// Act
		let html = render_deployment_status_badge("running").render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<span class=\"{} inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold {}\">Running</span>",
				STYLES.status_badge().as_str(),
				STYLES.status_running().as_str(),
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
			data_deployment_id: deployment.id.to_string(),
			td {
				class: "px-4 py-2 font-mono text-xs text-ink-600",
				{ deployment.id.to_string() }
			}
			td {
				class: "px-4 py-2",
				{ project_cell }
			}
			td {
				class: "px-4 py-2 font-mono text-xs text-ink-600",
				{ deployment.cluster_id.to_string() }
			}
			td {
				class: "px-4 py-2",
				{ status_cell }
			}
			td {
				class: "max-w-xs truncate px-4 py-2 text-ink-600",
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
				class: "rc-empty",
				"No deployments created."
			}
		});
	}

	let (preview_banner, summaries) = match preview_state.status {
		QueryStatus::Idle => (
			page!({
				div {
					class: "border-b border-cloud-100 px-4 py-2 text-xs font-medium text-cloud-500",
					"Preview status is not available during server rendering."
				}
			}),
			Vec::new(),
		),
		QueryStatus::Pending => (
			page!({
				div {
					class: "border-b border-cloud-100 px-4 py-2 text-xs font-medium text-cloud-500",
					"Loading previews..."
				}
			}),
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
						class: "border-b border-amber-100 bg-amber-50 px-4 py-2 text-xs font-medium text-amber-700",
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
			class: "overflow-x-auto",
			table {
				class: "rc-table",
				thead {
					class: "bg-cloud-50",
					tr {
						th {
							class: "rc-th",
							"ID"
						}
						th {
							class: "rc-th",
							"Project"
						}
						th {
							class: "rc-th",
							"Cluster"
						}
						th {
							class: "rc-th",
							"Status"
						}
						th {
							class: "rc-th",
							"Image"
						}
					}
				}
				tbody {
					class: "divide-y divide-cloud-100 bg-white",
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
		.on_submit_success(move |runtime| {
			self::invalidate_deployment_queries(&create_query_client);
			runtime.reset();
			create_success_callback.set(Some("Deployment created.".to_owned()));
		})
		.build();
	let create_state = create_runtime.form_state();
	let create_cluster_id = create_runtime.watch_field::<String>(create_form.cluster_id_field());
	let create_project_name =
		create_runtime.watch_field::<String>(create_form.project_name_field());
	let create_image = create_runtime.watch_field::<String>(create_form.image_field());
	let create_project_yaml =
		create_runtime.watch_field::<String>(create_form.project_yaml_field());
	let create_action_runtime = create_runtime.clone();
	let create_action = use_action(move |(): ()| {
		let runtime = create_action_runtime.clone();
		async move {
			let request = CreateDeploymentFormRequestClientForm::to_request(&runtime);
			runtime
				.submit_server_fn(|| async move { submit_create_deployment(request).await })
				.await
		}
	});
	let create_view = self::render_create_deployment_form(CreateDeploymentFormView {
		state: create_state,
		action: create_action,
		success: create_success,
		project_name: create_project_name,
		cluster_id: create_cluster_id,
		image: create_image,
		project_yaml: create_project_yaml,
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
		.on_submit_success(move |runtime| {
			self::invalidate_deployment_queries(&edit_query_client);
			runtime.reset();
			edit_success_callback.set(Some("Deployment updated.".to_owned()));
		})
		.build();
	let edit_state = edit_runtime.form_state();
	let edit_deployment_id = edit_runtime.watch_field::<String>(edit_form.deployment_id_field());
	let edit_project_name = edit_runtime.watch_field::<String>(edit_form.project_name_field());
	let edit_image = edit_runtime.watch_field::<String>(edit_form.image_field());
	let edit_status = edit_runtime.watch_field::<String>(edit_form.status_field());
	let edit_action_runtime = edit_runtime.clone();
	let edit_action = use_action(move |(): ()| {
		let runtime = edit_action_runtime.clone();
		async move {
			let request = UpdateDeploymentFormRequestClientForm::to_request(&runtime);
			runtime
				.submit_server_fn(|| async move { submit_update_deployment(request).await })
				.await
		}
	});
	let edit_view = self::render_update_deployment_form(UpdateDeploymentFormView {
		state: edit_state,
		action: edit_action,
		success: edit_success,
		project_name: edit_project_name,
		image: edit_image,
		status: edit_status,
	});

	let status_form = UpdateDeploymentStatusFormRequestClientForm::new();
	let status_success = Signal::new(None::<String>);
	let status_query_client = query_client.clone();
	let status_success_callback = status_success;
	let status_runtime = use_form(&status_form)
		.on_submit_success(move |runtime| {
			self::invalidate_deployment_queries(&status_query_client);
			runtime.reset();
			status_success_callback.set(Some("Deployment status updated.".to_owned()));
		})
		.build();
	let status_state = status_runtime.form_state();
	let status_deployment_id =
		status_runtime.watch_field::<String>(status_form.deployment_id_field());
	let status_value = status_runtime.watch_field::<String>(status_form.status_field());
	let status_action_runtime = status_runtime.clone();
	let status_action = use_action(move |(): ()| {
		let runtime = status_action_runtime.clone();
		async move {
			let request = UpdateDeploymentStatusFormRequestClientForm::to_request(&runtime);
			runtime
				.submit_server_fn(|| async move { submit_update_deployment_status(request).await })
				.await
		}
	});
	let status_view = self::render_update_deployment_status_form(UpdateDeploymentStatusFormView {
		state: status_state,
		action: status_action,
		success: status_success,
		status: status_value,
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
	let delete_action = use_action(move |(): ()| {
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
			submit_delete_deployment(deployment_id).await
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
	});
	let delete_view = self::render_delete_deployment_action(DeleteDeploymentActionView {
		action: delete_action,
		error: delete_error,
		success: delete_success,
		confirmed: delete_confirmed,
	});

	let logs = log_viewer_container(log_deployment_id);
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
			class: "rc-shell",
			div {
				class: "space-y-0",
				div {
					class: "rc-topline",
					div {
						p {
							class: "rc-kicker",
							"Release surface"
						}
						h1 {
							class: "rc-title",
							"Deployments"
						}
						p {
							class: "rc-muted mt-1",
							"Applications deployed through Reinhardt Cloud."
						}
					}
				}
				div {
					class: "grid gap-6 lg:grid-cols-[1fr_320px]",
					div {
						class: "space-y-6",
						section {
							class: "rc-panel",
								div {
									class: "rc-panel-head",
									"Deployment Inventory"
								} {
									let snapshot = props.deployments_for_inventory.snapshot();
									match snapshot.status {
										QueryStatus::Idle => page!({
											div {
												class: "rc-empty",
												"Deployments are not available during server rendering."
											}
										}),
										QueryStatus::Pending => page!({
											div {
												class: "rc-empty",
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
												class: "px-4 py-8 text-sm font-medium text-red-700",
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
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
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
												class: "mb-3 text-xs text-cloud-500",
												"Clusters are not available during server rendering."
											}
										}),
										QueryStatus::Pending => page!({
											p {
												class: "mb-3 text-xs text-ink-600",
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
												class: "mb-3 text-xs font-medium text-red-700",
												{ message }
											}
											})
										}
									}
								}
							{ props.create_view.clone() }
						}
						section {
							class: "rc-panel-pad",
									h2 {
										class: "mb-3 text-sm font-semibold text-ink-950",
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
													class: "mb-3 text-xs text-cloud-500",
													"Deployments are not available during server rendering."
												}
											}),
											QueryStatus::Pending => page!({
												p {
													class: "mb-3 text-xs text-ink-600",
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
													class: "mb-3 text-xs font-medium text-red-700",
													{ message }
												}
												})
											}
										}
									}
							div {
								class: "mt-3",
								{ props.logs.clone() }
							}
						}
					}
					aside {
						class: "rc-stack",
						section {
							class: "rc-panel-pad",
								h2 {
									class: "mb-3 text-sm font-semibold text-ink-950",
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
												class: "mb-3 text-xs text-cloud-500",
												"Deployments are not available during server rendering."
											}
						}),
						QueryStatus::Pending => page!({
											p {
												class: "mb-3 text-xs text-ink-600",
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
												class: "mb-3 text-xs font-medium text-red-700",
												{ message }
											}
							})
						}
									}
								}
							{ props.edit_view.clone() }
							div {
								class: "my-4 border-t border-cloud-200"
								}
								{
									let snapshot = props.deployments_for_status.snapshot();
									match snapshot.status {
									QueryStatus::Success => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&snapshot.data.unwrap_or_default()), props.status_deployment_id, |_value| {}, ),
						QueryStatus::Idle => page!({
											p {
												class: "mb-3 text-xs text-cloud-500",
												"Deployments are not available during server rendering."
											}
						}),
						QueryStatus::Pending => page!({
											p {
												class: "mb-3 text-xs text-ink-600",
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
												class: "mb-3 text-xs font-medium text-red-700",
												{ message }
											}
							})
						}
									}
								}
							{ props.status_view.clone() }
							div {
								class: "my-4 border-t border-cloud-200"
								}
								{
									let snapshot = props.deployments_for_delete.snapshot();
									match snapshot.status {
									QueryStatus::Success => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&snapshot.data.unwrap_or_default()), props.delete_deployment_id, |_value| {}, ),
						QueryStatus::Idle => page!({
											p {
												class: "mb-3 text-xs text-cloud-500",
												"Deployments are not available during server rendering."
											}
						}),
						QueryStatus::Pending => page!({
											p {
												class: "mb-3 text-xs text-ink-600",
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
												class: "mb-3 text-xs font-medium text-red-700",
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
		}
	})
}
