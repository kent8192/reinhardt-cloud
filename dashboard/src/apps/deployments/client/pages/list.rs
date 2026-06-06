//! Deployments list and CRUD page.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResetOnDeps, ResourceState, Signal, use_form, use_resource};

use crate::apps::deployments::client::components::log_viewer::log_viewer_container;
#[cfg(wasm)]
use crate::apps::deployments::server_fn::list_deployments_for_current_org;
use crate::apps::deployments::server_fn::{
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
			error.get().map(|message| {
			page!(|message: String| {
				div {
					class: "rounded-md border border-red-200 bg-red-50 px-3 py-2 text-sm font-medium text-red-700",
					{
						self::format_server_error(&message)
					}
				}
			})(message)
		}).unwrap_or(Page::Empty)
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
	let deployments = use_resource(|| async move { self::load_deployments().await }, ());

	let create_form = form! {
		name: CreateDeploymentForm,
		server_fn: create_deployment_for_current_org,
		method: Post,
		redirect_on_success: "/deployments",
		class: "grid gap-3 md:grid-cols-2",
		fields: {
			app_name: CharField {
				required,
				max_length: 63,
				label: "App Name",
				placeholder: "web",
				class: "rc-input",
			}
			cluster_id: CharField {
				required,
				label: "Cluster ID",
				placeholder: "1",
				class: "rc-input",
			}
			image: CharField {
				required,
				max_length: 512,
				label: "Image",
				placeholder: "ghcr.io/example/web:latest",
				class: "rc-input",
			}
			reinhardt_app_yaml: TextField {
				max_length: 65535,
				label: "ReinhardtApp YAML",
				widget: Textarea,
				class: "rc-input min-h-40 font-mono text-xs md:col-span-2",
			}
			submit: SubmitButton {
				label: "Create deployment",
				class: "btn-primary md:justify-self-start"
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
				class: "rc-input",
			}
			app_name: CharField {
				required,
				max_length: 63,
				label: "App Name",
				placeholder: "web",
				class: "rc-input",
			}
			image: CharField {
				required,
				max_length: 512,
				label: "Image",
				placeholder: "ghcr.io/example/web:latest",
				class: "rc-input",
			}
			status: CharField {
				required,
				max_length: 50,
				label: "Status",
				initial: "pending".to_string(),
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Update deployment",
				class: "btn-dark"
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
				class: "rc-input",
			}
			status: CharField {
				required,
				max_length: 50,
				label: "Status",
				placeholder: "running",
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Set status",
				class: "btn-warning"
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
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Delete deployment",
				class: "btn-danger"
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
			class: "rc-app",
			div {
				class: "rc-shell",
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
					a {
						href: "/clusters".to_string(),
						class: "rc-link",
						"Clusters"
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
							}
							{
								match deployments.get() {
									ResourceState::Loading => page!(|| {
										div {
											class: "rc-empty",
											"Loading deployments..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										div {
											class: "px-4 py-8 text-sm font-medium text-red-700",
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
													class: "rc-empty",
													"No deployments created."
												}
											})()
										} else {
											page!(|items: Vec<DeploymentInfo>| {
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
																	"App"
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
															{
																items.clone().into_iter().map(|deployment| page!(|deployment: DeploymentInfo| {
																	tr {
																		data_deployment_id: deployment.id.to_string(),
																		td {
																			class: "px-4 py-2 font-mono text-xs text-ink-600",
																			{
																				deployment.id.to_string()
																			}
																		}
																		td {
																			class: "px-4 py-2 font-semibold text-ink-950",
																			{
																				deployment.app_name.clone()
																			}
																		}
																		td {
																			class: "px-4 py-2 font-mono text-xs text-ink-600",
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
																						class: format!("status-badge inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-semibold {color}"),
																						{ label }
																					}
																				})(color, label)
																			}
																		}
																		td {
																			class: "max-w-xs truncate px-4 py-2 text-ink-600",
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
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
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
							}
							{ logs }
						}
					}
					aside {
						class: "space-y-4",
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Deployment Operations"
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
											class: "mt-2 text-xs text-ink-600",
											"Submitting..."
										}
									})()
								} else { Page::Empty }
							}
							div {
								class: "my-4 border-t border-cloud-200"
							}
							{
								self::alert(status_error.clone())
							}
							{ status_view }
							{
								if status_submitting.get() {
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
							{
								self::alert(delete_error.clone())
							}
							{ delete_view }
							{
								if delete_submitting.get() {
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
