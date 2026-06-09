//! Clusters list and CRUD page.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResetOnDeps, ResourceState, Signal, use_form, use_resource};

#[cfg(wasm)]
use crate::apps::clusters::server_fn::list_clusters_for_current_org;
use crate::apps::clusters::server_fn::{
	ClusterInfo, create_cluster_for_current_org, delete_cluster_for_current_org,
	rotate_cluster_token_for_current_org, update_cluster_for_current_org,
};
use crate::apps::dashboard::client::layout::dashboard_app_shell;
use crate::apps::deployments::client::components::cluster_health::cluster_health_container;
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};
use crate::shared::client::routes::route_href;

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
async fn load_clusters() -> Result<Vec<ClusterInfo>, String> {
	list_clusters_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_clusters() -> Result<Vec<ClusterInfo>, String> {
	Ok(Vec::new())
}

fn cluster_select_options(items: &[ClusterInfo]) -> Vec<EntitySelectOption> {
	items
		.iter()
		.map(|cluster| {
			let state = if cluster.is_active {
				"active"
			} else {
				"inactive"
			};
			EntitySelectOption::new(
				cluster.id.to_string(),
				cluster.name.clone(),
				Some(format!("{state} / {}", cluster.api_url)),
			)
		})
		.collect()
}

/// Render the clusters page.
pub fn clusters_list_page() -> Page {
	let clusters = use_resource(|| async move { self::load_clusters().await }, ());

	let create_form = form! {
		name: CreateClusterForm,
		server_fn: create_cluster_for_current_org,
		method: Post,
		success_url: |_form| route_href("clusters:list", "/clusters"),
		class: "rc-form-grid",
		fields: {
			name: CharField {
				required,
				max_length: 63,
				label: "Name",
				placeholder: "prod-us-east",
				class: "rc-input",
			}
			api_url: UrlField {
				required,
				max_length: 2048,
				label: "API URL",
				placeholder: "https://kubernetes.example.com:6443",
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Create cluster",
				class: "btn-primary min-h-11 w-full md:w-auto md:justify-self-start"
			}
		}
	};
	let create_runtime = use_form(&create_form).build();
	let create_state = create_runtime.form_state();
	let create_error = create_form.error().clone();
	let create_view = create_form.into_page();

	let edit_form = form! {
		name: UpdateClusterForm,
		server_fn: update_cluster_for_current_org,
		method: Post,
		success_url: |_form| route_href("clusters:list", "/clusters"),
		class: "rc-form-stack",
		fields: {
			cluster_id: HiddenField {
				initial: String::new(),
			}
			name: CharField {
				required,
				max_length: 63,
				label: "Name",
				placeholder: "cluster id required below",
				class: "rc-input",
			}
			api_url: UrlField {
				required,
				max_length: 2048,
				label: "API URL",
				placeholder: "https://kubernetes.example.com:6443",
				class: "rc-input",
			}
			is_active: BooleanField {
				label: "Active",
				initial: true,
				class: "rc-checkbox",
			}
			submit: SubmitButton {
				label: "Update cluster",
				class: "btn-dark min-h-11 w-full"
			}
		}
	};
	let edit_runtime = use_form(&edit_form)
		.deps("manual-cluster-edit")
		.reset_on_deps(ResetOnDeps::ResetAll)
		.build();
	let edit_state = edit_runtime.form_state();
	let edit_cluster_id = edit_runtime.watch_field::<String>(edit_form.cluster_id_field());
	let edit_name = edit_runtime.watch_field::<String>(edit_form.name_field());
	let edit_api_url = edit_runtime.watch_field::<String>(edit_form.api_url_field());
	let edit_is_active = edit_runtime.watch_field::<bool>(edit_form.is_active_field());
	let edit_error = edit_form.error().clone();
	let edit_view = edit_form.into_page();

	let delete_form = form! {
		name: DeleteClusterForm,
		server_fn: delete_cluster_for_current_org,
		method: Post,
		success_url: |_form| route_href("clusters:list", "/clusters"),
		class: "rc-form-stack",
		fields: {
			cluster_id: HiddenField {
				initial: String::new(),
			}
			submit: SubmitButton {
				label: "Delete cluster",
				class: "btn-danger min-h-11 w-full"
			}
		}
	};
	let delete_runtime = use_form(&delete_form).build();
	let delete_state = delete_runtime.form_state();
	let delete_cluster_id = delete_runtime.watch_field::<String>(delete_form.cluster_id_field());
	let delete_error = delete_form.error().clone();
	let delete_view = delete_form.into_page();

	let rotate_form = form! {
		name: RotateClusterTokenForm,
		server_fn: rotate_cluster_token_for_current_org,
		method: Post,
		success_url: |_form| route_href("clusters:list", "/clusters"),
		class: "rc-form-stack",
		fields: {
			cluster_id: HiddenField {
				initial: String::new(),
			}
			submit: SubmitButton {
				label: "Rotate token",
				class: "btn-warning min-h-11 w-full"
			}
		}
	};
	let rotate_runtime = use_form(&rotate_form).build();
	let rotate_state = rotate_runtime.form_state();
	let rotate_cluster_id = rotate_runtime.watch_field::<String>(rotate_form.cluster_id_field());
	let rotate_error = rotate_form.error().clone();
	let rotate_view = rotate_form.into_page();

	let health = cluster_health_container();
	let clusters_for_inventory = clusters.clone();
	let clusters_for_edit = clusters.clone();
	let clusters_for_rotate = clusters.clone();
	let clusters_for_delete = clusters.clone();

	let content = page!(|clusters_for_inventory: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, clusters_for_edit: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, clusters_for_rotate: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, clusters_for_delete: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, create_view: Page, create_error: Signal<Option<String>>, create_submitting: Signal<bool>, edit_view: Page, edit_error: Signal<Option<String>>, edit_dirty: Signal<bool>, edit_submitting: Signal<bool>, edit_cluster_id: Signal<String>, edit_name: Signal<String>, edit_api_url: Signal<String>, edit_is_active: Signal<bool>, delete_view: Page, delete_error: Signal<Option<String>>, delete_submitting: Signal<bool>, delete_cluster_id: Signal<String>, rotate_view: Page, rotate_error: Signal<Option<String>>, rotate_submitting: Signal<bool>, rotate_cluster_id: Signal<String>, health: Page| {
		div {
			class: "rc-shell",
			div {
				class: "space-y-0",
				div {
					class: "rc-topline",
					div {
						p {
							class: "rc-kicker",
							"Infrastructure"
						}
						h1 {
							class: "rc-title",
							"Clusters"
						}
						p {
							class: "rc-muted mt-1",
							"Registered Kubernetes clusters and agent health."
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
								"Cluster Inventory"
							}
							{
								match clusters_for_inventory.get() {
									ResourceState::Loading => page!(|| {
										div {
											class: "rc-empty",
											"Loading clusters..."
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
										if items.is_empty() {
											page!(|| {
												div {
													class: "rc-empty",
													"No clusters registered."
												}
											})()
										} else {
											page!(|items: Vec<ClusterInfo>| {
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
																	"Name"
																}
																th {
																	class: "rc-th",
																	"API URL"
																}
																th {
																	class: "rc-th",
																	"Active"
																}
																th {
																	class: "rc-th",
																	"Token Rotated"
																}
															}
														}
														tbody {
															class: "divide-y divide-cloud-100 bg-white",
															{
																items.clone().into_iter().map(|cluster| page!(|cluster: ClusterInfo| {
																	tr {
																		td {
																			class: "px-4 py-2 font-mono text-xs text-ink-600",
																			{
																				cluster.id.to_string()
																			}
																		}
																		td {
																			class: "px-4 py-2 font-semibold text-ink-950",
																			{ cluster.name }
																		}
																		td {
																			class: "px-4 py-2 text-ink-600",
																			{ cluster.api_url }
																		}
																		td {
																			class: "px-4 py-2",
																			span {
																				class: if cluster.is_active { "rounded-full bg-control-500/10 px-2 py-0.5 text-xs font-semibold text-control-700" } else { "rounded-full bg-cloud-100 px-2 py-0.5 text-xs font-semibold text-ink-600" },
																				{
																					if cluster.is_active { "Active" } else { "Inactive" }
																				}
																			}
																		}
																		td {
																			class: "px-4 py-2 text-ink-600",
																			{
																				cluster.token_last_rotated_at.clone().unwrap_or_else(||"never".to_string())
																			}
																		}
																	}
																})(cluster)).collect::<Vec<_>>()
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
								"Register Cluster"
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
								"Agent Health"
							}
							{ health }
						}
					}
					aside {
						class: "rc-stack",
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Cluster Operations"
							}
							{
								self::alert(edit_error.clone())
							}
							{
								match clusters_for_edit.get() {
									ResourceState::Success(items) => {
										let clusters_for_change = items.clone();
										let name_signal = edit_name.clone();
										let api_url_signal = edit_api_url.clone();
										let is_active_signal = edit_is_active.clone();
										self::entity_select("Cluster", "Select cluster", self::cluster_select_options(&items), edit_cluster_id.clone(), move |value| {
											if let Some(cluster) = clusters_for_change.iter().find(|cluster| cluster.id.to_string() == value) {
												name_signal.set(cluster.name.clone());
												api_url_signal.set(cluster.api_url.clone());
												is_active_signal.set(cluster.is_active);
											}
										}, )
									}
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{
												self::format_server_error(&message)
											}
										}
									})(message),
								}
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
								self::alert(rotate_error.clone())
							}
							{
								match clusters_for_rotate.get() {
									ResourceState::Success(items) => self::entity_select("Cluster", "Select cluster", self::cluster_select_options(&items), rotate_cluster_id.clone(), |_value| {}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{
												self::format_server_error(&message)
											}
										}
									})(message),
								}
							}
							{ rotate_view }
							{
								if rotate_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-ink-600",
											"Rotating..."
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
							{
								match clusters_for_delete.get() {
									ResourceState::Success(items) => self::entity_select("Cluster", "Select cluster", self::cluster_select_options(&items), delete_cluster_id.clone(), |_value| {}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-ink-600",
											"Loading clusters..."
										}
									})(),
									ResourceState::Error(message) => page!(|message: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{
												self::format_server_error(&message)
											}
										}
									})(message),
								}
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
		clusters_for_inventory,
		clusters_for_edit,
		clusters_for_rotate,
		clusters_for_delete,
		create_view,
		create_error,
		create_state.is_submitting,
		edit_view,
		edit_error,
		edit_state.is_dirty,
		edit_state.is_submitting,
		edit_cluster_id,
		edit_name,
		edit_api_url,
		edit_is_active,
		delete_view,
		delete_error,
		delete_state.is_submitting,
		delete_cluster_id,
		rotate_view,
		rotate_error,
		rotate_state.is_submitting,
		rotate_cluster_id,
		health,
	);
	dashboard_app_shell("clusters", content)
}
