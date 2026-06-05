//! Clusters list and CRUD page.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResetOnDeps, ResourceState, Signal, use_form, use_resource};

#[cfg(wasm)]
use crate::apps::clusters::server::list_clusters_for_current_org;
use crate::apps::clusters::server::{
	ClusterInfo, create_cluster_for_current_org, delete_cluster_for_current_org,
	rotate_cluster_token_for_current_org, update_cluster_for_current_org,
};
use crate::apps::deployments::client::components::cluster_health::cluster_health_container;

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
async fn load_clusters() -> Result<Vec<ClusterInfo>, String> {
	list_clusters_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_clusters() -> Result<Vec<ClusterInfo>, String> {
	Ok(Vec::new())
}

/// Render the clusters page.
pub fn clusters_list_page() -> Page {
	crate::shared::client::ws::ensure_notifications_connected();

	let clusters = use_resource(|| async move { self::load_clusters().await }, ());

	let create_form = form! {
		name: CreateClusterForm,
		server_fn: create_cluster_for_current_org,
		method: Post,
		redirect_on_success: "/clusters",
		class: "grid gap-3",
		fields: {
			name: CharField {
				required,
				max_length: 63,
				label: "Name",
				placeholder: "prod-us-east",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			api_url: UrlField {
				required,
				max_length: 2048,
				label: "API URL",
				placeholder: "https://kubernetes.example.com:6443",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			submit: SubmitButton {
				label: "Create cluster",
				class: "rounded bg-blue-600 px-3 py-2 text-sm font-medium text-white"
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
		redirect_on_success: "/clusters",
		class: "grid gap-3",
		fields: {
			cluster_id: HiddenField {
				initial: String::new(),
			}
			name: CharField {
				required,
				max_length: 63,
				label: "Name",
				placeholder: "cluster id required below",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			api_url: UrlField {
				required,
				max_length: 2048,
				label: "API URL",
				placeholder: "https://kubernetes.example.com:6443",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			is_active: BooleanField {
				label: "Active",
				initial: true,
				class: "h-4 w-4 rounded border-gray-300",
			}
			submit: SubmitButton {
				label: "Update cluster",
				class: "rounded bg-gray-900 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let edit_runtime = use_form(&edit_form)
		.deps("manual-cluster-edit")
		.reset_on_deps(ResetOnDeps::ResetAll)
		.build();
	let edit_state = edit_runtime.form_state();
	let edit_error = edit_form.error().clone();
	let edit_view = edit_form.into_page();

	let delete_form = form! {
		name: DeleteClusterForm,
		server_fn: delete_cluster_for_current_org,
		method: Post,
		redirect_on_success: "/clusters",
		class: "grid gap-3",
		fields: {
			cluster_id: CharField {
				required,
				label: "Cluster ID",
				placeholder: "1",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			submit: SubmitButton {
				label: "Delete cluster",
				class: "rounded bg-red-600 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let delete_runtime = use_form(&delete_form).build();
	let delete_state = delete_runtime.form_state();
	let delete_error = delete_form.error().clone();
	let delete_view = delete_form.into_page();

	let rotate_form = form! {
		name: RotateClusterTokenForm,
		server_fn: rotate_cluster_token_for_current_org,
		method: Post,
		redirect_on_success: "/clusters",
		class: "grid gap-3",
		fields: {
			cluster_id: CharField {
				required,
				label: "Cluster ID",
				placeholder: "1",
				class: "w-full rounded border border-gray-300 px-3 py-2 text-sm",
			}
			submit: SubmitButton {
				label: "Rotate token",
				class: "rounded bg-amber-600 px-3 py-2 text-sm font-medium text-white"
			}
		}
	};
	let rotate_runtime = use_form(&rotate_form).build();
	let rotate_state = rotate_runtime.form_state();
	let rotate_error = rotate_form.error().clone();
	let rotate_view = rotate_form.into_page();

	let health = cluster_health_container();

	page!(|clusters: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, create_view: Page, create_error: Signal<Option<String>>, create_submitting: Signal<bool>, edit_view: Page, edit_error: Signal<Option<String>>, edit_dirty: Signal<bool>, edit_submitting: Signal<bool>, delete_view: Page, delete_error: Signal<Option<String>>, delete_submitting: Signal<bool>, rotate_view: Page, rotate_error: Signal<Option<String>>, rotate_submitting: Signal<bool>, health: Page| {
		div {
			class: "min-h-screen bg-gray-50",
			div {
				class: "mx-auto max-w-7xl px-6 py-6",
				div {
					class: "mb-6 flex items-center justify-between",
					div {
						h1 {
							class: "text-2xl font-semibold text-gray-950",
							"Clusters"
						}
						p {
							class: "mt-1 text-sm text-gray-600",
							"Registered Kubernetes clusters and agent health."
						}
					}
					a {
						href: "/deployments".to_string(),
						class: "text-sm font-medium text-blue-700 hover:underline",
						"Deployments"
					}
				}
				div {
					class: "grid gap-6 lg:grid-cols-[1fr_360px]",
					div {
						class: "space-y-6",
						section {
							class: "rounded border border-gray-200 bg-white",
							div {
								class: "border-b border-gray-200 px-4 py-3 text-sm font-medium text-gray-700",
								"Cluster Inventory"
							}
							{
								match clusters.get() {
									ResourceState::Loading => page!(|| {
										div {
											class: "px-4 py-8 text-sm text-gray-500",
											"Loading clusters..."
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
										if items.is_empty() {
											page!(|| {
												div {
													class: "px-4 py-8 text-sm text-gray-500",
													"No clusters registered."
												}
											})()
										} else {
											page!(|items: Vec<ClusterInfo>| {
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
																	"Name"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"API URL"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"Active"
																}
																th {
																	class: "px-4 py-2 text-left font-medium text-gray-600",
																	"Token Rotated"
																}
															}
														}
														tbody {
															class: "divide-y divide-gray-100 bg-white",
															{
																items.clone().into_iter().map(|cluster| page!(|cluster: ClusterInfo| {
																	tr {
																		td {
																			class: "px-4 py-2 font-mono text-xs text-gray-700",
																			{
																				cluster.id.to_string()
																			}
																		}
																		td {
																			class: "px-4 py-2 font-medium text-gray-950",
																			{ cluster.name }
																		}
																		td {
																			class: "px-4 py-2 text-gray-600",
																			{ cluster.api_url }
																		}
																		td {
																			class: "px-4 py-2",
																			span {
																				class: if cluster.is_active { "rounded bg-green-100 px-2 py-0.5 text-xs text-green-800" } else { "rounded bg-gray-100 px-2 py-0.5 text-xs text-gray-700" },
																				{
																					if cluster.is_active { "Active" } else { "Inactive" }
																				}
																			}
																		}
																		td {
																			class: "px-4 py-2 text-gray-500",
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
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Agent Health"
							}
							{ health }
						}
					}
					aside {
						class: "space-y-4",
						section {
							class: "rounded border border-gray-200 bg-white p-4",
							h2 {
								class: "mb-3 text-sm font-semibold text-gray-900",
								"Create Cluster"
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
								"Edit Cluster"
							}
							p {
								class: "mb-3 text-xs text-gray-500",
								"Enter an ID from the table, then submit updated values."
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
								"Token / Delete"
							}
							{
								self::alert(rotate_error.clone())
							}
							{ rotate_view }
							{
								if rotate_submitting.get() {
									page!(|| {
										p {
											class: "mt-2 text-xs text-gray-500",
											"Rotating..."
										}
									})()
								} else { Page::Empty }
							}
							div {
								class: "my-4 border-t border-gray-200"
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
		clusters,
		create_view,
		create_error,
		create_state.is_submitting,
		edit_view,
		edit_error,
		edit_state.is_dirty,
		edit_state.is_submitting,
		delete_view,
		delete_error,
		delete_state.is_submitting,
		rotate_view,
		rotate_error,
		rotate_state.is_submitting,
		health,
	)
}
