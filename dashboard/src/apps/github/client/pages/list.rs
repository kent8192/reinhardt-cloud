//! GitHub repository import page.

use reinhardt::pages::component::{IntoPage, Page, PageElement};
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
use crate::shared::client::routes::route_href;

fn format_server_error(raw: &str) -> String {
	let json_start = raw.find('{').unwrap_or(0);
	let candidate = &raw[json_start..];
	if let Ok(value) = serde_json::from_str::<serde_json::Value>(candidate)
		&& let Some(obj) = value.as_object()
		&& let Some((_, payload)) = obj.iter().next()
	{
		if let Some(s) = payload.as_str() {
			if s.starts_with("CurrentUser:") {
				return "Sign in to continue.".to_string();
			}
			return s.to_string();
		}
		if let Some(msg) = payload.get("message").and_then(|v| v.as_str()) {
			if msg.starts_with("CurrentUser:") {
				return "Sign in to continue.".to_string();
			}
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
		class: "rc-form-stack",
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

	let selected_repository_id = import_repository_id.clone();
	let selected_cluster_id = import_cluster_id.clone();
	let selected_app_name = import_app_name.clone();
	let content = page!(|repositories: reinhardt::pages::prelude::Resource<Vec<GitHubRepositoryInfo>, String>, onboarding: reinhardt::pages::prelude::Resource<GitHubOnboardingInfo, String>, clusters: reinhardt::pages::prelude::Resource<Vec<ClusterInfo>, String>, import_view: Page, import_error: Signal<Option<String>>, import_submitting: Signal<bool>, import_repository_id: Signal<String>, import_cluster_id: Signal<String>, import_app_name: Signal<String>, selected_repository_id: Signal<String>, selected_cluster_id: Signal<String>, selected_app_name: Signal<String>| {
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
					class: "grid gap-6 lg:grid-cols-[minmax(0,1fr)_360px]",
					section {
						class: "rc-panel",
						div {
							class: "rc-panel-head flex items-center justify-between gap-3",
							span { "Repository Inventory" }
							span {
								class: "rounded-full bg-control-500/10 px-2.5 py-1 text-[11px] font-bold text-control-700",
								"GitHub App"
							}
						}
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
											"Repository"
										}
										th {
											class: "rc-th",
											"Branch"
										}
										th {
											class: "rc-th",
											"State"
										}
										th {
											class: "rc-th text-right",
											""
										}
									}
								}
								tbody {
									class: "divide-y divide-cloud-100 bg-white",
									{
										match repositories.get() {
											ResourceState::Loading => page!(|| {
												tr {
													td {
														class: "rc-empty",
														colspan: 5,
														"Loading repositories..."
													}
												}
											})(),
											ResourceState::Error(err) => page!(|err: String| {
												tr {
													td {
														class: "px-4 py-8 text-sm font-medium text-red-700",
														colspan: 5,
														{
															self::format_server_error(&err)
														}
													}
												}
											})(err),
											ResourceState::Success(items)if items.is_empty() => page!(|onboarding: reinhardt::pages::prelude::Resource<GitHubOnboardingInfo, String>| {
												tr {
													td {
														class: "rc-empty",
														colspan: 5,
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
											ResourceState::Success(items) => page!(|items: Vec<GitHubRepositoryInfo>, import_repository_id: Signal<String>, import_app_name: Signal<String>| { {
												items.clone().into_iter().map(|repo| {
													page!(|repo: GitHubRepositoryInfo, import_repository_id: Signal<String>, import_app_name: Signal<String>| {
														tr {
															td {
																class: "px-4 py-3 font-mono text-xs text-ink-600",
																{
																	repo.id.to_string()
																}
															}
															td {
																class: "px-4 py-3",
																div {
																	class: "font-semibold text-ink-950",
																	{
																		repo.full_name.clone()
																	}
																}
																div {
																	class: "mt-0.5 text-xs font-medium text-ink-600",
																	{
																		if repo.private { "private" } else { "public" }
																	}
																}
															}
															td {
																class: "px-4 py-3 font-mono text-xs text-ink-600",
																{
																	repo.default_branch.clone()
																}
															}
															td {
																class: "px-4 py-3",
																span {
																	class: if repo.selected { "rounded-full bg-control-500/10 px-2.5 py-0.5 text-xs font-semibold text-control-700" } else { "rounded-full bg-cloud-100 px-2.5 py-0.5 text-xs font-semibold text-ink-600" },
																	{
																		if repo.selected { "imported" } else { "available" }
																	}
																}
															}
															td {
																class: "px-4 py-3 text-right",
																{
																	let repository_id_signal = import_repository_id.clone();
																	let app_name_signal = import_app_name.clone();
																	let repo_id = repo.id.to_string();
																	let app_name = repo.name.clone();
																	PageElement::new("button").attr("type", "button").attr("class", "btn-secondary min-w-20 px-3 py-1.5 text-xs").listener("click", move |_event| {
																		repository_id_signal.set(repo_id.clone());
																		app_name_signal.set(app_name.clone());
																	}).child("Select").into_page()
																}
															}
														}
													})(repo, import_repository_id.clone(), import_app_name.clone())
												}).collect::<Vec<_>>()
											} })(items, import_repository_id.clone(), import_app_name.clone()),
										}
									}
								}
							}
						}
					}
					aside {
						class: "rc-stack",
						section {
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
								"Import"
							}
							div {
								class: "mb-4 grid gap-2 rounded-md border border-control-500/20 bg-control-500/5 p-3 text-sm",
								div {
									class: "flex items-center justify-between gap-3",
									span {
										class: "text-xs font-bold uppercase text-ink-600",
										"Repository"
									}
									span {
										class: "font-mono text-xs font-semibold text-ink-950",
										{
											let value = selected_repository_id.get();
											if value.trim().is_empty() { "not selected".to_string() } else { format!("id {value}") }
										}
									}
								}
								div {
									class: "flex items-center justify-between gap-3",
									span {
										class: "text-xs font-bold uppercase text-ink-600",
										"Cluster"
									}
									span {
										class: "font-mono text-xs font-semibold text-ink-950",
										{
											let value = selected_cluster_id.get();
											if value.trim().is_empty() { "not selected".to_string() } else { format!("id {value}") }
										}
									}
								}
								div {
									class: "flex items-center justify-between gap-3",
									span {
										class: "text-xs font-bold uppercase text-ink-600",
										"App"
									}
									span {
										class: "truncate text-xs font-semibold text-ink-950",
										{
											let value = selected_app_name.get();
											if value.trim().is_empty() { "derived from repository".to_string() } else { value }
										}
									}
								}
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
							class: "rc-panel-pad",
							h2 {
								class: "mb-3 text-sm font-semibold text-ink-950",
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
														class: "rounded-md border border-cloud-200 bg-white px-3 py-2 shadow-[0_1px_0_rgba(17,16,19,0.03)]",
														div {
															class: "flex items-start justify-between gap-3",
															div {
																class: "min-w-0",
																div {
																	class: "truncate font-semibold text-ink-950",
																	{
																		cluster.name.clone()
																	}
																}
																div {
																	class: "mt-0.5 font-mono text-xs text-ink-600",
																	{
																		format!("id {}", cluster.id)
																	}
																}
															}
															{
																let cluster_id_signal = import_cluster_id.clone();
																let cluster_id = cluster.id.to_string();
																PageElement::new("button").attr("type", "button").attr("class", "btn-secondary shrink-0 px-3 py-1.5 text-xs").listener("click", move |_event| {
																	cluster_id_signal.set(cluster_id.clone());
																}).child("Use").into_page()
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
		onboarding,
		clusters,
		import_view,
		import_error,
		import_state.is_submitting,
		import_repository_id,
		import_cluster_id,
		import_app_name,
		selected_repository_id,
		selected_cluster_id,
		selected_app_name,
	);

	dashboard_app_shell("github", content)
}

#[cfg(test)]
mod tests {
	use rstest::rstest;

	use super::format_server_error;

	#[rstest]
	#[case(
		r#"Server error (401): {"Server":{"status":401,"message":"CurrentUser: User is not authenticated"}}"#,
		"Sign in to continue."
	)]
	#[case(
		r#"{"Server":{"status":500,"message":"Repository import failed"}}"#,
		"Repository import failed"
	)]
	fn server_error_formatter_extracts_actionable_message(
		#[case] raw: &str,
		#[case] expected: &str,
	) {
		// Arrange, Act
		let formatted = format_server_error(raw);

		// Assert
		assert_eq!(formatted, expected);
	}
}
