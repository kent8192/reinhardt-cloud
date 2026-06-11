//! GitHub repository import page.

use reinhardt::pages::component::Page;
use reinhardt::pages::form;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{ResourceState, Signal, use_form, use_resource};

use crate::apps::clusters::server_fn::ClusterInfo;
#[cfg(wasm)]
use crate::apps::clusters::server_fn::list_clusters_for_current_org;
use crate::apps::dashboard::client::layout::dashboard_app_shell;
use crate::apps::github::server_fn::{
	GitHubOnboardingInfo, GitHubRepositoryInfo, import_github_repository_for_current_org,
};
#[cfg(wasm)]
use crate::apps::github::server_fn::{
	get_github_onboarding_for_current_org, list_github_repositories_for_current_org,
};
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
async fn load_onboarding() -> Result<GitHubOnboardingInfo, String> {
	get_github_onboarding_for_current_org()
		.await
		.map_err(|e| e.to_string())
}

#[cfg(not(wasm))]
async fn load_onboarding() -> Result<GitHubOnboardingInfo, String> {
	Ok(GitHubOnboardingInfo {
		github_account_linked: true,
		install_url: None,
	})
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

fn repository_select_options(items: &[GitHubRepositoryInfo]) -> Vec<EntitySelectOption> {
	items
		.iter()
		.map(|repository| {
			let visibility = if repository.private {
				"private"
			} else {
				"public"
			};
			EntitySelectOption::new(
				repository.id.to_string(),
				repository.full_name.clone(),
				Some(format!("{visibility} / {}", repository.default_branch)),
			)
		})
		.collect()
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

/// Render the GitHub repository import page.
pub fn github_repositories_page() -> Page {
	let repositories = use_resource(|| async move { self::load_repositories().await }, ());
	let onboarding = use_resource(|| async move { self::load_onboarding().await }, ());
	let clusters = use_resource(|| async move { self::load_clusters().await }, ());

	let import_form = form! {
		name: ImportGitHubRepositoryForm,
		server_fn: import_github_repository_for_current_org,
		method: Post,
		success_url: |_form| route_href("github:repositories", "/github"),
		class: "rc-form-grid",
		fields: {
			repository_id: HiddenField {
				initial: String::new(),
			}
			cluster_id: HiddenField {
				initial: String::new(),
			}
			app_name: CharField {
				max_length: 63,
				label: "App Name",
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
				class: "btn-primary min-h-11 w-full md:w-auto md:justify-self-start"
			}
		}
	};
	let import_runtime = use_form(&import_form).build();
	let import_state = import_runtime.form_state();
	let import_repository_id =
		import_runtime.watch_field::<String>(import_form.repository_id_field());
	let import_cluster_id = import_runtime.watch_field::<String>(import_form.cluster_id_field());
	let import_app_name = import_runtime.watch_field::<String>(import_form.app_name_field());
	let import_error = import_form.error().clone();
	let import_view = import_form.into_page();
	let repositories_for_inventory = repositories.clone();
	let repositories_for_import = repositories.clone();
	let clusters_for_import = clusters.clone();
	let clusters_for_inventory = clusters.clone();

	let content = page!(|repositories_for_inventory: reinhardt::pages::prelude::Resource<Vec<GitHubRepositoryInfo>, String>, repositories_for_import: reinhardt::pages::prelude::Resource<Vec<GitHubRepositoryInfo>, String>, onboarding: reinhardt::pages::prelude::Resource<GitHubOnboardingInfo, String>, clusters_for_import: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, clusters_for_inventory: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, import_view: Page, import_error: Signal<Option<String>>, import_submitting: Signal<bool>, import_repository_id: Signal<String>, import_cluster_id: Signal<String>, import_app_name: Signal<String>| {
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
									}
								}
								tbody {
									class: "divide-y divide-cloud-100",
									{
										match repositories_for_inventory.get() {
											ResourceState::Loading => page!(|| {
												tr {
													td {
														class: "px-3 py-3 text-cloud-500",
														colspan: 4,
														"Loading repositories..."
													}
												}
											})(),
											ResourceState::Error(err) => page!(|err: String| {
												tr {
													td {
														class: "px-3 py-3 text-red-700",
														colspan: 4,
														{
															self::format_server_error(&err)
														}
													}
												}
											})(err),
											ResourceState::Success(items)if items.is_empty() => page!(|onboarding: reinhardt::pages::prelude::Resource<GitHubOnboardingInfo, String>| {
												tr {
													td {
														class: "px-3 py-4 text-cloud-500",
														colspan: 4,
														{
															match onboarding.get() {
																ResourceState::Success(info)if !info.github_account_linked => page!(|| {
																	div {
																		class: "flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
																		span { "Link your GitHub account before installing the GitHub App." }
																		a {
																			class: "btn-secondary text-xs",
																			href: "/api/auth/oauth/github/start/",
																			"Link GitHub account"
																		}
																	}
																})(),
																ResourceState::Success(info) => {
																	if let Some(url) = info.install_url {
																		page!(|url: String| {
																			div {
																				class: "flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
																				span { "No GitHub App repositories are available." }
																				a {
																					class: "btn-secondary text-xs",
																					href: url,
																					"Connect GitHub repositories"
																				}
																			}
																		})(url)
																	} else {
																		page!(|| { "No GitHub App repositories are available." })()
																	}
																}
																_ => page!(|| { "No GitHub App repositories are available." })(),
															}
														}
													}
												}
											})(onboarding.clone()),
											ResourceState::Success(items) => page!(|items: Vec<GitHubRepositoryInfo>| { {
												items.clone().into_iter().map(|repo| {
													page!(|repo: GitHubRepositoryInfo| {
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
														}
													})(repo)
												}).collect::<Vec<_>>()
											} })(items),
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
								match repositories_for_import.get() {
									ResourceState::Success(items) => {
										let repositories_for_change = items.clone();
										let app_name_signal = import_app_name.clone();
										self::entity_select("Repository", "Select repository", self::repository_select_options(&items), import_repository_id.clone(), move |value| {
											if let Some(repository) = repositories_for_change.iter().find(|repository| repository.id.to_string() == value) {
												app_name_signal.set(repository.name.clone());
											}
										}, )
									}
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Loading repositories..."
										}
									})(),
									ResourceState::Error(err) => page!(|err: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{
												self::format_server_error(&err)
											}
										}
									})(err),
								}
							}
							{
								match clusters_for_import.get() {
									ResourceState::Success(items) => self::entity_select("Cluster", "Select target cluster", self::cluster_select_options(&items), import_cluster_id.clone(), |_value| {}, ),
									ResourceState::Loading => page!(|| {
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Loading clusters..."
										}
									})(),
									ResourceState::Error(err) => page!(|err: String| {
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{
												self::format_server_error(&err)
											}
										}
									})(err),
								}
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
									match clusters_for_inventory.get() {
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
										ResourceState::Success(items) => page!(|items: Vec<ClusterInfo>| { {
											items.clone().into_iter().map(|cluster| {
												page!(|cluster: ClusterInfo| {
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
													}
												})(cluster)
											}).collect::<Vec<_>>()
										} })(items),
									}
								}
							}
						}
					}
				}
			}
		}
	})(
		repositories_for_inventory,
		repositories_for_import,
		onboarding,
		clusters_for_import,
		clusters_for_inventory,
		import_view,
		import_error,
		import_state.is_submitting,
		import_repository_id,
		import_cluster_id,
		import_app_name,
	);

	dashboard_app_shell("github", content)
}
