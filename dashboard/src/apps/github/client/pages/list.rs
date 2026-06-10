//! GitHub repository import page.

use reinhardt::pages::component::{IntoPage, Page, PageElement};
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResourceState, Signal, use_form, use_resource};

use crate::apps::clusters::server_fn::ClusterInfo;
#[cfg(wasm)]
use crate::apps::clusters::server_fn::list_clusters_for_current_org;
use crate::apps::dashboard::client::layout::dashboard_app_shell;
#[cfg(wasm)]
use crate::apps::github::server_fn::list_github_repositories_for_current_org;
use crate::apps::github::server_fn::{
	GitHubRepositoryInfo, import_github_repository_for_current_org,
};

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
async fn load_repositories() -> Result<Vec<GitHubRepositoryInfo>, String> {
	list_github_repositories_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_repositories() -> Result<Vec<GitHubRepositoryInfo>, String> {
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

/// Render the GitHub repository import page.
pub fn github_repositories_page() -> Page {
	let repositories = use_resource(|| async move { self::load_repositories().await }, ());
	let clusters = use_resource(|| async move { self::load_clusters().await }, ());

	let import_form = form! {
		name: ImportGitHubRepositoryForm,
		server_fn: import_github_repository_for_current_org,
		method: Post,
		redirect_on_success: "/github",
		class: "grid gap-3 md:grid-cols-2",
		fields: {
			repository_id: CharField {
				required,
				label: "Repository ID",
				placeholder: "1",
				class: "rc-input",
			}
			cluster_id: CharField {
				required,
				label: "Cluster ID",
				placeholder: "1",
				class: "rc-input",
			}
			project_name: CharField {
				max_length: 63,
				label: "Project name",
				placeholder: "leave blank to derive from repository",
				class: "rc-input",
			}
			registry: CharField {
				required,
				max_length: 512,
				label: "Registry Image Prefix",
				placeholder: "ghcr.io/kent8192/my-app",
				class: "rc-input",
			}
			submit: SubmitButton {
				label: "Import repository",
				class: "btn-primary md:justify-self-start"
			}
		}
	};
	let import_runtime = use_form(&import_form).build();
	let import_state = import_runtime.form_state();
	let import_repository_id =
		import_runtime.watch_field::<String>(import_form.repository_id_field());
	let import_cluster_id = import_runtime.watch_field::<String>(import_form.cluster_id_field());
	let import_project_name = import_runtime.watch_field::<String>(import_form.project_name_field());
	let import_error = import_form.error().clone();
	let import_view = import_form.into_page();

	let content = page!(|repositories: reinhardt::pages::prelude::Resource<Vec<GitHubRepositoryInfo>, String>, clusters: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, import_view: Page, import_error: Signal<Option<String>>, import_submitting: Signal<bool>, import_repository_id: Signal<String>, import_cluster_id: Signal<String>, import_project_name: Signal<String>| {
		div {
			class: "rc-shell",
			div {
				class: "space-y-0",
				div {
					class: "rc-topline",
					div {
						p {
							class: "rc-kicker",
							"Source Control"
						}
						h1 {
							class: "rc-title",
							"GitHub Repositories"
						}
						p {
							class: "rc-muted mt-1",
							"Import GitHub App repositories into Reinhardt Cloud deployments."
						}
					}
				}
				div {
					class: "grid gap-6 lg:grid-cols-[1fr_340px]",
					section {
						class: "rc-panel",
						div {
							class: "rc-panel-head",
							"Repository Inventory"
						}
						div {
							class: "overflow-x-auto",
							table {
								class: "min-w-full divide-y divide-cloud-200 text-sm",
								thead {
									tr {
										th {
											class: "px-3 py-2 text-left font-semibold text-cloud-600",
											"ID"
										}
										th {
											class: "px-3 py-2 text-left font-semibold text-cloud-600",
											"Repository"
										}
										th {
											class: "px-3 py-2 text-left font-semibold text-cloud-600",
											"Branch"
										}
										th {
											class: "px-3 py-2 text-left font-semibold text-cloud-600",
											"State"
										}
										th {
											class: "px-3 py-2 text-left font-semibold text-cloud-600",
											""
										}
									}
								}
								tbody {
									class: "divide-y divide-cloud-100",
									{
										match repositories.get() {
											ResourceState::Loading => page!(|| {
												tr {
													td {
														class: "px-3 py-3 text-cloud-500",
														colspan: 5,
														"Loading repositories..."
													}
												}
											})(),
											ResourceState::Error(err) => page!(|err: String| {
												tr {
													td {
														class: "px-3 py-3 text-red-700",
														colspan: 5,
														{
															self::format_server_error(&err)
														}
													}
												}
											})(err),
											ResourceState::Success(items)if items.is_empty() => page!(|| {
												tr {
													td {
														class: "px-3 py-3 text-cloud-500",
														colspan: 5,
														"No GitHub App repositories are available."
													}
												}
											})(),
											ResourceState::Success(items) => page!(|items: Vec<GitHubRepositoryInfo>, import_repository_id: Signal<String>, import_project_name: Signal<String>| { {
												items.clone().into_iter().map(|repo| {
													page!(|repo: GitHubRepositoryInfo, import_repository_id: Signal<String>, import_project_name: Signal<String>| {
														tr {
															td {
																class: "px-3 py-2 font-mono text-xs text-cloud-500",
																{
																	repo.id.to_string()
																}
															}
															td {
																class: "px-3 py-2",
																div {
																	class: "font-medium text-cloud-900",
																	{
																		repo.full_name.clone()
																	}
																}
																div {
																	class: "text-xs text-cloud-500",
																	{
																		if repo.private { "private" } else { "public" }
																	}
																}
															}
															td {
																class: "px-3 py-2 text-cloud-700",
																{
																	repo.default_branch.clone()
																}
															}
															td {
																class: "px-3 py-2",
																span {
																	class: if repo.selected { "rounded bg-emerald-50 px-2 py-1 text-xs font-medium text-emerald-700" } else { "rounded bg-cloud-100 px-2 py-1 text-xs font-medium text-cloud-600" },
																	{
																		if repo.selected { "imported" } else { "available" }
																	}
																}
															}
															td {
																class: "px-3 py-2 text-right",
																{
																	let repository_id_signal = import_repository_id.clone();
																	let project_name_signal = import_project_name.clone();
																	let repo_id = repo.id.to_string();
																	let project_name = repo.name.clone();
																	PageElement::new("button").attr("type", "button").attr("class", "btn-secondary text-xs").listener("click", move |_event| {
																		repository_id_signal.set(repo_id.clone());
																		project_name_signal.set(project_name.clone());
																	}).child("Select").into_page()
																}
															}
														}
													})(repo, import_repository_id.clone(), import_project_name.clone())
												}).collect::<Vec<_>>()
											} })(items, import_repository_id.clone(), import_project_name.clone()),
										}
									}
								}
							}
						}
					}
					div {
						class: "space-y-6",
						section {
							class: "rc-panel",
							div {
								class: "rc-panel-head",
								"Import"
							}
							{
								self::alert(import_error.clone())
							}
							{
								import_view.clone()
							}
							if import_submitting.get() {
								p {
									class: "mt-2 text-sm text-cloud-500",
									"Importing..."
								}
							}
						}
						section {
							class: "rc-panel",
							div {
								class: "rc-panel-head",
								"Active Clusters"
							}
							div {
								class: "space-y-2 text-sm",
								{
									match clusters.get() {
										ResourceState::Loading => page!(|| {
											p {
												class: "text-cloud-500",
												"Loading clusters..."
											}
										})(),
										ResourceState::Error(err) => page!(|err: String| {
											p {
												class: "text-red-700",
												{
													self::format_server_error(&err)
												}
											}
										})(err),
										ResourceState::Success(items)if items.is_empty() => page!(|| {
											p {
												class: "text-cloud-500",
												"No active clusters."
											}
										})(),
										ResourceState::Success(items) => page!(|items: Vec<ClusterInfo>, import_cluster_id: Signal<String>| { {
											items.clone().into_iter().map(|cluster| {
												page!(|cluster: ClusterInfo, import_cluster_id: Signal<String>| {
													div {
														class: "rounded border border-cloud-200 px-3 py-2",
														div {
															class: "font-medium text-cloud-900",
															{
																cluster.name.clone()
															}
														}
														div {
															class: "font-mono text-xs text-cloud-500",
															{
																format!("id {}", cluster.id)
															}
														}
														div {
															class: "mt-2",
															{
																let cluster_id_signal = import_cluster_id.clone();
																let cluster_id = cluster.id.to_string();
																PageElement::new("button").attr("type", "button").attr("class", "btn-secondary text-xs").listener("click", move |_event| {
																	cluster_id_signal.set(cluster_id.clone());
																}).child("Select").into_page()
															}
														}
													}
												})(cluster, import_cluster_id.clone())
											}).collect::<Vec<_>>()
										} })(items, import_cluster_id.clone()),
									}
								}
							}
						}
					}
				}
			}
		}
	})(
		repositories,
		clusters,
		import_view,
		import_error,
		import_state.is_submitting,
		import_repository_id,
		import_cluster_id,
		import_project_name,
	);

	dashboard_app_shell("github", content)
}
