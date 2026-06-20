//! Deployments list and CRUD page.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	ResetOnDeps, Resource, ResourceState, Signal, use_form, use_resource,
};

use crate::apps::clusters::server_fn::ClusterInfo;
#[cfg(wasm)]
use crate::apps::clusters::server_fn::list_clusters_for_current_org;
use crate::apps::dashboard::client::layout::dashboard_app_shell;
use crate::apps::deployments::client::components::log_viewer::log_viewer_container;
use crate::apps::deployments::client::components::preview_list::{
	render_preview_list, render_project_identity,
};
use crate::apps::deployments::server_fn::{
	DeploymentInfo, DeploymentLogInfo, ProjectPreviewSummary, create_deployment_for_current_org,
	delete_deployment_for_current_org, update_deployment_for_current_org,
	update_deployment_status_for_current_org,
};
#[cfg(wasm)]
use crate::apps::deployments::server_fn::{
	deployment_logs_for_current_org, list_deployment_previews_for_current_org,
	list_deployments_for_current_org,
};
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};
use crate::shared::client::components::status_badge;
use crate::shared::client::routes::route_href;
use crate::shared::client::ws::subscribe_app_logs;
#[cfg(wasm)]
use crate::shared::client::ws::track_subscriptions;
use crate::shared::ws_messages::DeploymentState;

fn format_server_error(raw: &str) -> String {
	if let Ok(value) = serde_json::from_str::<serde_json::Value>(raw)
		&& let Some(obj) = value.as_object()
		&& let Some((_, payload)) = obj.iter().next()
	{
		if let Some(s) = payload.as_str() {
			return s.to_string();
		}
		if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
			return msg.to_string();
		}
	}
	raw.to_string()
}

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
	page!(|error: Signal<Option<String>>| {
		{
			error
	.get()
	.map(|message| {
		page!(|message: String| {
			div {
				class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700",
				{
					self::format_server_error(&message)
				}
			}
		})(message)
	})
	.unwrap_or(Page::Empty)
		}
	})(error)
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

#[cfg(wasm)]
async fn load_deployments() -> Result<Vec<DeploymentInfo>, String> {
	list_deployments_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_deployments() -> Result<Vec<DeploymentInfo>, String> {
	Ok(Vec::new())
}

#[cfg(wasm)]
async fn load_deployment_previews() -> Result<Vec<ProjectPreviewSummary>, String> {
	list_deployment_previews_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_deployment_previews() -> Result<Vec<ProjectPreviewSummary>, String> {
	Ok(Vec::new())
}

#[cfg(wasm)]
async fn load_clusters() -> Result<Vec<ClusterInfo>, String> {
	list_clusters_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_clusters() -> Result<Vec<ClusterInfo>, String> {
	Ok(Vec::new())
}

#[cfg(wasm)]
async fn load_deployment_logs(deployment_id: String) -> Result<Vec<DeploymentLogInfo>, String> {
	if deployment_id.trim().is_empty() {
		return Ok(Vec::new());
	}
	deployment_logs_for_current_org(deployment_id)
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_deployment_logs(_deployment_id: String) -> Result<Vec<DeploymentLogInfo>, String> {
	Ok(Vec::new())
}

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
		return page!(|identity: Page, previews: Page| {
			div {
				{ identity }
				{ previews }
			}
		})(
			render_project_identity(summary),
			render_preview_list(summary),
		);
	}
	page!(|project_name: String| {
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
	})(deployment.project_name.clone())
}

fn render_deployment_status_badge(status: &str) -> Page {
	let (color, label) = status_badge::badge_style(&self::state_from_status(status));
	page!(|color: &'static str, label: &'static str| {
		span {
			class: format!(
				"status-badge inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold {color}"
			),
			{ label }
		}
	})(color, label)
}

fn render_deployment_inventory_row(
	deployment: &DeploymentInfo,
	summary: Option<&ProjectPreviewSummary>,
) -> Page {
	page!(|deployment: DeploymentInfo, project_cell: Page, status_cell: Page| {
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
	})(
		deployment.clone(),
		render_deployment_project_cell(deployment, summary),
		render_deployment_status_badge(&deployment.status),
	)
}

fn render_deployment_inventory_table(
	items: Vec<DeploymentInfo>,
	preview_state: ResourceState<Vec<ProjectPreviewSummary>, String>,
) -> Page {
	if items.is_empty() {
		return page!(|| {
			div {
				class: "rc-empty",
				"No deployments created."
			}
		})();
	}

	let (preview_banner, summaries) = match preview_state {
		ResourceState::Success(summaries) => (Page::Empty, summaries),
		ResourceState::Loading => (
			page!(|| {
				div {
					class: "border-b border-cloud-100 px-4 py-2 text-xs font-medium text-cloud-500",
					"Loading previews..."
				}
			})(),
			Vec::new(),
		),
		ResourceState::Error(_) => (
			page!(|| {
				div {
					class: "border-b border-amber-100 bg-amber-50 px-4 py-2 text-xs font-medium text-amber-700",
					"Preview status is temporarily unavailable"
				}
			})(),
			Vec::new(),
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

	page!(|preview_banner: Page, rows: Vec<Page>| {
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
	})(preview_banner, rows)
}

struct DeploymentsListPageViewProps {
	deployments_for_inventory: Resource<Vec<DeploymentInfo>, String>,
	deployments_for_logs: Resource<Vec<DeploymentInfo>, String>,
	deployments_for_edit: Resource<Vec<DeploymentInfo>, String>,
	deployments_for_status: Resource<Vec<DeploymentInfo>, String>,
	deployments_for_delete: Resource<Vec<DeploymentInfo>, String>,
	deployments_for_previews: Resource<Vec<ProjectPreviewSummary>, String>,
	clusters_for_create: Resource<Vec<ClusterInfo>, String>,
	create_view: Page,
	create_error: Signal<Option<String>>,
	create_submitting: Signal<bool>,
	create_cluster_id: Signal<String>,
	edit_view: Page,
	edit_error: Signal<Option<String>>,
	edit_dirty: Signal<bool>,
	edit_submitting: Signal<bool>,
	edit_deployment_id: Signal<String>,
	edit_project_name: Signal<String>,
	edit_image: Signal<String>,
	edit_status: Signal<String>,
	status_view: Page,
	status_error: Signal<Option<String>>,
	status_submitting: Signal<bool>,
	status_deployment_id: Signal<String>,
	delete_view: Page,
	delete_error: Signal<Option<String>>,
	delete_submitting: Signal<bool>,
	delete_deployment_id: Signal<String>,
	log_deployment_id: Signal<String>,
	logs: Page,
}

/// Render the deployments page.
#[reinhardt::pages::component("/deployments", "deployments:list")]
pub fn deployments_list_page() -> Page {
	let deployments = use_resource(|| async move { self::load_deployments().await }, ());
	let deployment_previews =
		use_resource(|| async move { self::load_deployment_previews().await }, ());
	let clusters = use_resource(|| async move { self::load_clusters().await }, ());

	let create_form = form! {
		name: CreateDeploymentForm,
		server_fn: create_deployment_for_current_org,
		method: Post,
		success_url: |_form| route_href("deployments:list", "/deployments"),
		class: "rc-form-grid",
		fields: {
			project_name: CharField {
				required,
				max_length: 63,
				label: "Project name",
				wrapper_class: "rc-field",
				label_class: "rc-label",
				placeholder: "web",
				class: "rc-input",
			}
			cluster_id: HiddenField {
				initial: String::new(),
			}
			image: CharField {
				required,
				max_length: 512,
				label: "Image",
				wrapper_class: "rc-field",
				label_class: "rc-label",
				placeholder: "ghcr.io/example/web:latest",
				class: "rc-input",
			}
			project_yaml: TextField {
				max_length: 65535,
				label: "Project YAML",
				wrapper_class: "rc-field md:col-span-2",
				label_class: "rc-label",
				widget: Textarea,
				class: "rc-input rc-textarea",
			}
			submit: SubmitButton {
				label: "Create deployment",
				class: "btn-primary min-h-11 w-full md:w-auto md:justify-self-start"
			}
		}
	};
	let create_runtime = use_form(&create_form).build();
	let create_state = create_runtime.form_state();
	let create_cluster_id = create_runtime.watch_field::<String>(create_form.cluster_id_field());
	let create_error = create_form.error().clone();
	let create_view = create_form.into_page();

	let edit_form = form! {
		name: UpdateDeploymentForm,
		server_fn: update_deployment_for_current_org,
		method: Post,
		success_url: |_form| route_href("deployments:list", "/deployments"),
		class: "rc-form-stack",
		fields: {
			deployment_id: HiddenField {
				initial: String::new(),
			}
			project_name: CharField {
				required,
				max_length: 63,
				label: "Project name",
				wrapper_class: "rc-field",
				label_class: "rc-label",
				placeholder: "web",
				class: "rc-input",
			}
			image: CharField {
				required,
				max_length: 512,
				label: "Image",
				wrapper_class: "rc-field",
				label_class: "rc-label",
				placeholder: "ghcr.io/example/web:latest",
				class: "rc-input",
			}
			status: CharField {
				required,
				max_length: 50,
				label: "Status",
				wrapper_class: "rc-field",
				label_class: "rc-label",
				initial: "pending".to_string(),
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Update deployment",
				class: "btn-dark min-h-11 w-full"
			}
		}
	};
	let edit_runtime = use_form(&edit_form)
		.deps("manual-deployment-edit")
		.reset_on_deps(ResetOnDeps::ResetAll)
		.build();
	let edit_state = edit_runtime.form_state();
	let edit_deployment_id = edit_runtime.watch_field::<String>(edit_form.deployment_id_field());
	let edit_project_name = edit_runtime.watch_field::<String>(edit_form.project_name_field());
	let edit_image = edit_runtime.watch_field::<String>(edit_form.image_field());
	let edit_status = edit_runtime.watch_field::<String>(edit_form.status_field());
	let edit_error = edit_form.error().clone();
	let edit_view = edit_form.into_page();

	let status_form = form! {
		name: DeploymentStatusForm,
		server_fn: update_deployment_status_for_current_org,
		method: Post,
		success_url: |_form| route_href("deployments:list", "/deployments"),
		class: "rc-form-stack",
		fields: {
			deployment_id: HiddenField {
				initial: String::new(),
			}
			status: CharField {
				required,
				max_length: 50,
				label: "Status",
				wrapper_class: "rc-field",
				label_class: "rc-label",
				placeholder: "running",
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Set status",
				class: "btn-warning min-h-11 w-full"
			}
		}
	};
	let status_runtime = use_form(&status_form).build();
	let status_state = status_runtime.form_state();
	let status_deployment_id =
		status_runtime.watch_field::<String>(status_form.deployment_id_field());
	let status_error = status_form.error().clone();
	let status_view = status_form.into_page();

	let delete_form = form! {
		name: DeleteDeploymentForm,
		server_fn: delete_deployment_for_current_org,
		method: Post,
		success_url: |_form| route_href("deployments:list", "/deployments"),
		class: "rc-form-stack",
		fields: {
			deployment_id: HiddenField {
				initial: String::new(),
			}
			submit: SubmitButton {
				label: "Delete deployment",
				class: "btn-danger min-h-11 w-full"
			}
		}
	};
	let delete_runtime = use_form(&delete_form).build();
	let delete_state = delete_runtime.form_state();
	let delete_deployment_id =
		delete_runtime.watch_field::<String>(delete_form.deployment_id_field());
	let delete_error = delete_form.error().clone();
	let delete_view = delete_form.into_page();

	let log_deployment_id = Signal::new(String::new());
	let log_history = use_resource(
		{
			let log_deployment_id = log_deployment_id.clone();
			move || {
				let deployment_id = log_deployment_id.get();
				async move { self::load_deployment_logs(deployment_id).await }
			}
		},
		(log_deployment_id.clone(),),
	);
	let logs = log_viewer_container(log_history);
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
		create_error,
		create_submitting: create_state.is_submitting,
		create_cluster_id,
		edit_view,
		edit_error,
		edit_dirty: edit_state.is_dirty,
		edit_submitting: edit_state.is_submitting,
		edit_deployment_id,
		edit_project_name,
		edit_image,
		edit_status,
		status_view,
		status_error,
		status_submitting: status_state.is_submitting,
		status_deployment_id,
		delete_view,
		delete_error,
		delete_submitting: delete_state.is_submitting,
		delete_deployment_id,
		log_deployment_id,
		logs,
	};

	let content = page!(|props: DeploymentsListPageViewProps| {
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
								match props.deployments_for_inventory.get() {
									ResourceState::Loading => page!(|| {
										div {
											class: "rc-empty",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										div {
											class: "px-4 py-8 text-sm font-medium text-red-700",
											{ self::format_server_error(&message) }
										}
									})(message),
									ResourceState::Success(items)=> {
										self::track_visible_deployments(&items);
										self::render_deployment_inventory_table(items, props.deployments_for_previews.get(), )
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
							{ self::alert(props.create_error.clone()) } {
								match props.clusters_for_create.get() {
									ResourceState::Success(items) => self::entity_select("Cluster", "Select target cluster", self::cluster_select_options(&items), props.create_cluster_id.clone(), |_value| {}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ self::format_server_error(&message) }
										}
									})(message),
								}
							}
							{ props.create_view.clone() } {
								if props.create_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-ink-600",
											"Submitting..."
										}
									})()
								} else { Page::Empty }
							}
						}
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Live Logs"
							} {
								match props.deployments_for_logs.get() {
									ResourceState::Success(items) => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&items), props.log_deployment_id.clone(), |value| {
										self::subscribe_app_logs(&value);
									}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ self::format_server_error(&message) }
										}
									})(message),
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
							{ self::alert(props.edit_error.clone()) } {
								match props.deployments_for_edit.get() {
									ResourceState::Success(items)=> {
										let deployments_for_change = items.clone();
										let project_name_signal = props.edit_project_name.clone();
										let image_signal = props.edit_image.clone();
										let status_signal = props.edit_status.clone();
										self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&items), props.edit_deployment_id.clone(), move |value| {
											if let Some(deployment) = deployments_for_change.iter().find(|deployment| deployment.id.to_string() == value) {
												project_name_signal.set(deployment.project_name.clone());
												image_signal.set(deployment.image.clone());
												status_signal.set(deployment.status.clone());
											}
										}, )
									}ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ self::format_server_error(&message) }
										}
									})(message),
								}
							}
							{ props.edit_view.clone() } {
								if props.edit_dirty.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-amber-700",
											"Unsaved changes"
										}
									})()
								} else { Page::Empty }
							}
							{
								if props.edit_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-ink-600",
											"Submitting..."
										}
									})()
								} else { Page::Empty }
							}
							div {
								class: "my-4 border-t border-cloud-200"
							}
							{ self::alert(props.status_error.clone()) } {
								match props.deployments_for_status.get() {
									ResourceState::Success(items) => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&items), props.status_deployment_id.clone(), |_value| {}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ self::format_server_error(&message) }
										}
									})(message),
								}
							}
							{ props.status_view.clone() } {
								if props.status_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-ink-600",
											"Updating..."
										}
									})()
								} else { Page::Empty }
							}
							div {
								class: "my-4 border-t border-cloud-200"
							}
							{ self::alert(props.delete_error.clone()) } {
								match props.deployments_for_delete.get() {
									ResourceState::Success(items) => self::entity_select("Deployment", "Select deployment", self::deployment_select_options(&items), props.delete_deployment_id.clone(), |_value| {}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ self::format_server_error(&message) }
										}
									})(message),
								}
							}
							{ props.delete_view.clone() } {
								if props.delete_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-ink-600",
											"Deleting..."
										}
									})()
								} else { Page::Empty }
							}
						}
					}
				}
			}
		}
	})(props);
	dashboard_app_shell("deployments", content)
}
