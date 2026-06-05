//! Deployments list and CRUD page.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResetOnDeps, ResourceState, Signal, use_form, use_resource};

use crate::apps::deployments::client::components::log_viewer::log_viewer_container;
#[cfg(wasm)]
use crate::apps::deployments::server::list_deployments_for_current_org;
use crate::apps::deployments::server::{
	DeploymentInfo, create_deployment_for_current_org, delete_deployment_for_current_org,
	update_deployment_for_current_org, update_deployment_status_for_current_org,
};
use crate::shared::client::components::status_badge;
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
							class: "rounded border border-red-200 bg-red-50 px-3 py-2 text-sm text-red-700",
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
	crate::shared::client::ws::track_subscriptions(&ids);
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

/// Render the deployments page.
pub fn deployments_list_page() -> Page {
	crate::shared::client::ws::ensure_notifications_connected();

	let deployments = use_resource(|| async move { self::load_deployments().await }, ());

	let create_form = form! {
		name: CreateDeploymentForm,
		server_fn: create_deployment_for_current_org,
		method: Post,
		redirect_on_success: "/deployments",
		class: "grid gap-3",
		fields: {
			app_name: CharField {
				required,
				max_length: 63,
				label: "App Name",
				placeholder: "web",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			cluster_id: CharField {
				required,
				label: "Cluster ID",
				placeholder: "1",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			image: CharField {
				required,
				max_length: 512,
				label: "Image",
				placeholder: "ghcr.io/example/web:latest",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			reinhardt_app_yaml: TextField {
				max_length: 65535,
				label: "ReinhardtApp YAML",
				widget: Textarea,
				class: "min-h-32 w-full rounded border border-gray-300 px-3 py-2 font-mono text-xs",
			}
			submit: SubmitButton {
				label: "Create deployment",
				class: "rounded bg-blue-600 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let create_runtime = use_form(&create_form).build();
	let create_state = create_runtime.form_state();
	let create_error = create_form.error().clone();
	let create_view = create_form.into_page();

	let edit_form = form! {
		name: UpdateDeploymentForm,
		server_fn: update_deployment_for_current_org,
		method: Post,
		redirect_on_success: "/deployments",
		class: "grid gap-3",
		fields: {
			deployment_id: CharField {
				required,
				label: "Deployment ID",
				placeholder: "1",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			app_name: CharField {
				required,
				max_length: 63,
				label: "App Name",
				placeholder: "web",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			image: CharField {
				required,
				max_length: 512,
				label: "Image",
				placeholder: "ghcr.io/example/web:latest",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			status: CharField {
				required,
				max_length: 50,
				label: "Status",
				initial: "pending".to_string(),
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			submit: SubmitButton {
				label: "Update deployment",
				class: "rounded bg-gray-900 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let edit_runtime = use_form(&edit_form)
		.deps("manual-deployment-edit")
		.reset_on_deps(ResetOnDeps::ResetAll)
		.build();
	let edit_state = edit_runtime.form_state();
	let edit_error = edit_form.error().clone();
	let edit_view = edit_form.into_page();

	let status_form = form! {
		name: DeploymentStatusForm,
		server_fn: update_deployment_status_for_current_org,
		method: Post,
		redirect_on_success: "/deployments",
		class: "grid gap-3",
		fields: {
			deployment_id: CharField {
				required,
				label: "Deployment ID",
				placeholder: "1",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			status: CharField {
				required,
				max_length: 50,
				label: "Status",
				placeholder: "running",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			submit: SubmitButton {
				label: "Set status",
				class: "rounded bg-amber-600 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let status_runtime = use_form(&status_form).build();
	let status_state = status_runtime.form_state();
	let status_error = status_form.error().clone();
	let status_view = status_form.into_page();

	let delete_form = form! {
		name: DeleteDeploymentForm,
		server_fn: delete_deployment_for_current_org,
		method: Post,
		redirect_on_success: "/deployments",
		class: "grid gap-3",
		fields: {
			deployment_id: CharField {
				required,
				label: "Deployment ID",
				placeholder: "1",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			submit: SubmitButton {
				label: "Delete deployment",
				class: "rounded bg-red-600 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let delete_runtime = use_form(&delete_form).build();
	let delete_state = delete_runtime.form_state();
	let delete_error = delete_form.error().clone();
	let delete_view = delete_form.into_page();

	let logs = log_viewer_container();

	page!(|deployments: reinhardt::pages::prelude::Resource<Vec<DeploymentInfo>, String>, create_view: Page, create_error: Signal<Option<String>>, create_submitting: Signal<bool>, edit_view: Page, edit_error: Signal<Option<String>>, edit_dirty: Signal<bool>, edit_submitting: Signal<bool>, status_view: Page, status_error: Signal<Option<String>>, status_submitting: Signal<bool>, delete_view: Page, delete_error: Signal<Option<String>>, delete_submitting: Signal<bool>, logs: Page| {
		div {
			class: "min-h-screen bg-gray-50",
			div {
				class: "mx-auto max-w-7xl px-6 py-6",
				div {
					class: "mb-6 flex items-center justify-between",
					div {
						h1 {
							class: "text-2xl font-semibold text-gray-950",
							"Deployments"
						}
						p {
							class: "mt-1 text-sm text-gray-600",
							"Applications deployed through Reinhardt Cloud."
						}
					}
					a {
						href: "/clusters".to_string(),
						class: "text-sm font-medium text-blue-700 hover:underline",
						"Clusters"
					}
				}
				div {
					class: "grid gap-6 lg:grid-cols-[1fr_380px]",
					div {
						class: "space-y-6",
						section {
							class: "rounded border border-gray-200 bg-white",
							div {
								class: "border-b border-gray-200 px-4 py-3 text-sm font-medium text-gray-700",
								"Deployment Inventory"
							}
							{
								match deployments.get() {
									ResourceState::Loading => page!(|| {
										div {
											class: "px-4 py-8 text-sm text-gray-500",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										div {
											class: "px-4 py-8 text-sm text-red-700",
											{
												self::format_server_error(&message)
											}
										}
									})(message),
									ResourceState::Success(items) => {
										self::track_visible_deployments(&items);
										if items.is_empty() {
											page!(|| {
												div {
													class: "px-4 py-8 text-sm text-gray-500",
													"No deployments created."
												}
											})()
										} else {
											page!(|items: Vec<DeploymentInfo>| {
												div {
													class: "overflow-x-auto",
													table {
														class: "min-w-full divide-y divide-gray-200 text-sm",
														thead {
															class: "bg-gray-50",
															tr {
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"ID"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"App"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"Cluster"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"Status"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"Image"
																}
															}
														}
														tbody {
															class: "divide-y divide-gray-100 bg-white",
															{
																items.clone().into_iter().map(|deployment| page!(|deployment: DeploymentInfo| {
																	tr {
																		data_deployment_id: deployment.id.to_string(),
																		td {
																			class: "px-4 py-2 font-mono text-xs text-gray-700",
																			{
																				deployment.id.to_string()
																			}
																		}
																		td {
																			class: "px-4 py-2 font-medium text-gray-950",
																			{
																				deployment.app_name.clone()
																			}
																		}
																		td {
																			class: "px-4 py-2 font-mono text-xs text-gray-700",
																			{
																				deployment.cluster_id.to_string()
																			}
																		}
																		td {
																			class: "px-4 py-2",
																			{
																				let(color, label) = status_badge::badge_style(&self::state_from_status(&deployment.status));
																				page!(|color: &'static str, label: &'static str| {
																					span {
																						class: format!("status-badge inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium {color}"),
																						{ label }
																					}
																				})(color, label)
																			}
																		}
																		td {
																			class: "max-w-xs truncate px-4 py-2 text-gray-600",
																			{ deployment.image }
																		}
																	}
																})(deployment)).collect::<Vec<_>>()
															}
														}
													}
												}
											})(items)
										}
									}
								}
							}
						}
						section {
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Live Logs"
							}
							{ logs }
						}
					}
					aside {
						class: "space-y-4",
						section {
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Create Deployment"
							}
							{
								self::alert(create_error.clone())
							}
							{ create_view }
							{
								if create_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-gray-500",
											"Submitting..."
										}
									})()
								} else { Page::Empty }
							}
						}
						section {
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Edit Deployment"
							}
							{
								self::alert(edit_error.clone())
							}
							{ edit_view }
							{
								if edit_dirty.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-amber-700",
											"Unsaved changes"
										}
									})()
								} else { Page::Empty }
							}
							{
								if edit_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-gray-500",
											"Submitting..."
										}
									})()
								} else { Page::Empty }
							}
						}
						section {
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Status"
							}
							{
								self::alert(status_error.clone())
							}
							{ status_view }
							{
								if status_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-gray-500",
											"Updating..."
										}
									})()
								} else { Page::Empty }
							}
						}
						section {
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Delete"
							}
							{
								self::alert(delete_error.clone())
							}
							{ delete_view }
							{
								if delete_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-gray-500",
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
	})(
		deployments,
		create_view,
		create_error,
		create_state.is_submitting,
		edit_view,
		edit_error,
		edit_state.is_dirty,
		edit_state.is_submitting,
		status_view,
		status_error,
		status_state.is_submitting,
		delete_view,
		delete_error,
		delete_state.is_submitting,
		logs,
	)
}
