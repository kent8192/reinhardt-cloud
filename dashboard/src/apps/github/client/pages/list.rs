//! GitHub repository import page.

use std::collections::HashMap;

use reinhardt::pages::component::Page;
use reinhardt::pages::event::SubmitEvent;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Callback, FieldError, QueryHandle, QueryOptions, QueryStatus, Signal,
	UseFormAsyncSubmitOutcome, use_action_state, use_form, use_query,
};
use reinhardt::pages::server_fn::ServerFnError;

#[cfg(wasm)]
use reinhardt::pages::prelude::queries;

use crate::apps::clusters::server_fn::{ClusterInfo, list_clusters_for_current_org};
use crate::apps::deployments::client::components::preview_list::{
	render_preview_list, render_project_identity,
};
use crate::apps::deployments::server_fn::ProjectPreviewSummary;
#[cfg(wasm)]
use crate::apps::deployments::server_fn::{
	list_deployment_previews_for_current_org, list_deployments_for_current_org,
};
#[cfg(native)]
use crate::apps::github::server_fn::GitHubProjectInfo;
#[cfg(wasm)]
use crate::apps::github::server_fn::list_github_repositories_for_installation;
use crate::apps::github::server_fn::{
	GitHubOnboardingInfo, GitHubRepositoryImportRequestClientForm,
	GitHubRepositoryImportRequestClientFormField, GitHubRepositoryInfo,
	get_github_onboarding_for_current_org, list_github_project_previews_for_current_org,
	list_github_repositories_for_current_org,
};
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};

fn alert(error: Signal<Option<String>>) -> Page {
	page!({
		{
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
		}
	})
}

fn refetch_notice(is_fetching: bool, error: Option<ServerFnError>, label: &'static str) -> Page {
	if let Some(error) = error {
		let message = format!("Refresh failed: {}", error.user_message());
		return page!({
			div {
				class: "mb-3 rounded-md border border-amber-200 bg-amber-50 px-3 py-2 text-xs font-medium text-amber-800",
				{ message }
			}
		});
	}
	if is_fetching {
		return page!({
			div {
				class: "mb-3 rounded-md border border-cloud-100 bg-cloud-50 px-3 py-2 text-xs font-medium text-cloud-600",
				"Refreshing " { label }"..."
			}
		});
	}
	Page::Empty
}

fn query_error_message(error: Option<ServerFnError>) -> String {
	error.map_or_else(String::new, |error| error.user_message().to_owned())
}

fn import_field_error(
	field_errors: Signal<HashMap<GitHubRepositoryImportRequestClientFormField, FieldError>>,
	field: GitHubRepositoryImportRequestClientFormField,
) -> Page {
	page!({
		{
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
		}
	})
}

pub(crate) fn render_imported_project_card(summary: &ProjectPreviewSummary) -> Page {
	let identity = render_project_identity(summary);
	let previews = render_preview_list(summary);
	page!({
		article {
			class: "rounded-md border border-cloud-200 bg-white p-4 shadow-[0_1px_0_rgba(17,16,19,0.03)]",
			{ identity }
			{ previews }
		}
	})
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

#[derive(Clone)]
struct GitHubRepositoriesPageViewProps {
	repositories_for_inventory: QueryHandle<Vec<GitHubRepositoryInfo>, ServerFnError>,
	repositories_for_import: QueryHandle<Vec<GitHubRepositoryInfo>, ServerFnError>,
	imported_project_previews_for_list: QueryHandle<Vec<ProjectPreviewSummary>, ServerFnError>,
	onboarding: QueryHandle<GitHubOnboardingInfo, ServerFnError>,
	clusters_for_import: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	clusters_for_inventory: QueryHandle<Vec<ClusterInfo>, ServerFnError>,
	import_view: Page,
	import_error: Signal<Option<String>>,
	import_field_errors: Signal<HashMap<GitHubRepositoryImportRequestClientFormField, FieldError>>,
	import_submitting: Signal<bool>,
	import_repository_id: Signal<String>,
	import_cluster_id: Signal<String>,
	import_project_name: Signal<String>,
	selected_repository_id: Signal<String>,
	selected_cluster_id: Signal<String>,
	selected_project_name: Signal<String>,
}

/// Render the GitHub repository import page.
#[reinhardt::pages::component("github", name = "github:repositories")]
pub fn github_repositories_page() -> Page {
	let repositories = use_query(
		list_github_repositories_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let onboarding = use_query(
		get_github_onboarding_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let imported_project_previews = use_query(
		list_github_project_previews_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);
	let clusters = use_query(
		list_clusters_for_current_org::query(),
		QueryOptions::new().enabled(cfg!(wasm)),
	);

	let import_form = GitHubRepositoryImportRequestClientForm::new();
	let import_runtime = use_form(&import_form).build();
	let import_state = import_runtime.form_state();
	let import_repository_id =
		import_runtime.watch_field::<String>(import_form.repository_id_field());
	let import_cluster_id = import_runtime.watch_field::<String>(import_form.cluster_id_field());
	let import_project_name =
		import_runtime.watch_field::<String>(import_form.project_name_field());
	let import_registry = import_runtime.watch_field::<String>(import_form.registry_field());
	let import_field_errors = import_state.field_errors;
	#[cfg(wasm)]
	let import_action = use_action_state(move |(): ()| {
		let import_form = import_form.clone();
		let import_runtime = import_runtime.clone();
		async move { import_form.submit(&import_runtime).await }
	})
	.on_success(|outcome| {
		if matches!(outcome, UseFormAsyncSubmitOutcome::Submitted(_)) {
			let query_client = queries();
			query_client.invalidate_family(list_github_repositories_for_current_org::family());
			query_client.invalidate_family(list_github_repositories_for_installation::family());
			query_client.invalidate_family(list_github_project_previews_for_current_org::family());
			query_client.invalidate_family(list_deployments_for_current_org::family());
			query_client.invalidate_family(list_deployment_previews_for_current_org::family());
		}
	})
	.build();
	#[cfg(native)]
	let import_action = use_action_state(move |(): ()| async {
		Ok::<UseFormAsyncSubmitOutcome<GitHubProjectInfo>, ServerFnError>(
			UseFormAsyncSubmitOutcome::ValidationFailed,
		)
	})
	.build();
	let submit_import = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		import_action.dispatch(());
	});
	let import_view = page!({
		form {
			class: "rc-form-stack",
			@submit: submit_import,
			div {
				class: "rc-field",
				label {
					class: "rc-label",
					"Project name"
				}
				input {
					aria_label: "Project name",
					class: "rc-input",
					type: "text",
					placeholder: "leave blank to derive from repository",
					bind: import_project_name,
				}
				{ self::import_field_error(
					import_field_errors,
					GitHubRepositoryImportRequestClientFormField::ProjectName,
				) }
			}
			div {
				class: "rc-field",
				label {
					class: "rc-label",
					"Registry Image Prefix"
				}
				input {
					aria_label: "Registry Image Prefix",
					class: "rc-input",
					type: "text",
					placeholder: "ghcr.io/kent8192/my-app",
					bind: import_registry,
				}
				{ self::import_field_error(
					import_field_errors,
					GitHubRepositoryImportRequestClientFormField::Registry,
				) }
			}
			button {
				type: "submit",
				class: "btn-primary min-h-11 w-full md:w-auto md:justify-self-start",
				"Import repository"
			}
		}
	});
	let import_error = import_state.form_error;
	let repositories_for_inventory = repositories.clone();
	let repositories_for_import = repositories.clone();
	let imported_project_previews_for_list = imported_project_previews.clone();
	let clusters_for_import = clusters.clone();
	let clusters_for_inventory = clusters.clone();
	let imported_project_previews_for_refetch = imported_project_previews.clone();
	let repositories_for_inventory_refetch = repositories.clone();
	let onboarding_for_refetch = onboarding.clone();
	let repositories_for_import_refetch = repositories.clone();
	let clusters_for_import_refetch = clusters.clone();
	let clusters_for_inventory_refetch = clusters.clone();

	let selected_repository_id = import_repository_id;
	let selected_cluster_id = import_cluster_id;
	let selected_project_name = import_project_name;
	let props = GitHubRepositoriesPageViewProps {
		repositories_for_inventory,
		repositories_for_import,
		imported_project_previews_for_list,
		onboarding,
		clusters_for_import,
		clusters_for_inventory,
		import_view,
		import_error,
		import_field_errors,
		import_submitting: import_state.is_submitting,
		import_repository_id,
		import_cluster_id,
		import_project_name,
		selected_repository_id,
		selected_cluster_id,
		selected_project_name,
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
						class: "space-y-6",
						section {
							class: "rc-panel",
							div {
								class: "rc-panel-head",
								"Imported Projects"
							}
							div {
								class: "p-4",
									{
										let snapshot = imported_project_previews_for_refetch.snapshot();
										self::refetch_notice(
											snapshot.status == QueryStatus::Success && snapshot.is_fetching,
											snapshot.refetch_error,
											"imported projects",
										)
									}
								{
									let snapshot = props.imported_project_previews_for_list.snapshot();
									match snapshot.status {
										QueryStatus::Idle | QueryStatus::Pending => page!({
											div {
												class: "rc-empty",
												"Loading imported projects..."
											}
										}),
										QueryStatus::Error => page!({
											div {
												class: "px-4 py-8 text-sm font-medium text-amber-700",
												"Imported projects are temporarily unavailable"
											}
										}),
										QueryStatus::Success => match snapshot.data {
											Some(items) if items.is_empty() => page!({
												div {
													class: "rc-empty",
													"No imported projects yet"
												}
											}),
											Some(items) => page!({
												div {
													class: "grid gap-3 xl:grid-cols-2",
													{ items.iter().map(self::render_imported_project_card).collect::<Vec<_>>() }
												}
											}),
											None => Page::Empty,
										},
									}
								}
							}
						}
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
							{
								let snapshot = repositories_for_inventory_refetch.snapshot();
								self::refetch_notice(
									snapshot.status == QueryStatus::Success && snapshot.is_fetching,
									snapshot.refetch_error,
									"repositories",
								)
							}
							{
								let snapshot = onboarding_for_refetch.snapshot();
								self::refetch_notice(
									snapshot.status == QueryStatus::Success && snapshot.is_fetching,
									snapshot.refetch_error,
									"GitHub App status",
								)
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
										}
									}
									tbody {
										class: "divide-y divide-cloud-100 bg-white",
										{
											let snapshot = props.repositories_for_inventory.snapshot();
											match snapshot.status {
											QueryStatus::Idle | QueryStatus::Pending => page!({
												tr {
													td {
														class: "rc-empty",
														colspan: 4,
														"Loading repositories..."
													}
												}
											}),
											QueryStatus::Error => {
												let err = self::query_error_message(snapshot.error);
												page!({
												tr {
													td {
														class: "px-4 py-8 text-sm font-medium text-red-700",
														colspan: 4,
														{ err }
													}
												}
												})
											},
											QueryStatus::Success if snapshot.data.as_ref().is_some_and(Vec::is_empty) => {
												let onboarding = props.onboarding.clone();
												page!({
												tr {
													td {
														class: "rc-empty",
														colspan: 4,
														{
															let snapshot = onboarding.snapshot();
															match snapshot.status {
																QueryStatus::Success if snapshot.data.as_ref().is_some_and(|info| !info.github_account_linked) => page!({
																	div {
																		class: "flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
																		span { "Link your GitHub account before installing the GitHub App." }
																		a {
																			class: "btn-secondary text-xs",
																			href: "/api/auth/oauth/github/start/",
																			"Link GitHub account"
																		}
																	}
																}),
																QueryStatus::Success => {
																	if let Some(url) = snapshot.data.and_then(|info| info.install_url) {
																		page!({
																			div {
																				class: "flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between",
																				span { "No GitHub App repositories are available." }
																				a {
																					class: "btn-secondary text-xs",
																					href: url,
																					"Connect GitHub repositories"
																				}
																			}
																		})
																	} else {
																		page!({ "No GitHub App repositories are available." })
																	}
																}
																_ => page!({ "No GitHub App repositories are available." }),
															}
														}
													}
												}
												})
											},
											QueryStatus::Success => {
												let items = snapshot.data.unwrap_or_default();
												page!({ {
												items.clone().into_iter().map(|repo| {
													page!({
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
														}
													})
												}).collect::<Vec<_>>()
												} })
											},
											}
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
											let value = props.selected_repository_id.get();
											if value.trim().is_empty() {
												"not selected".to_string()
											} else {
												format!("id {value}")
											}
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
											let value = props.selected_cluster_id.get();
											if value.trim().is_empty() {
												"not selected".to_string()
											} else {
												format!("id {value}")
											}
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
											let value = props.selected_project_name.get();
											if value.trim().is_empty() {
												"derived from repository".to_string()
											} else { value }
										}
									}
								}
							}
							{
								self::alert(props.import_error)
							}
							{
								let snapshot = repositories_for_import_refetch.snapshot();
								self::refetch_notice(
									snapshot.status == QueryStatus::Success && snapshot.is_fetching,
									snapshot.refetch_error,
									"repositories",
								)
							}
							{
								let snapshot = props.repositories_for_import.snapshot();
								match snapshot.status {
									QueryStatus::Success => {
										let items = snapshot.data.unwrap_or_default();
										let repositories_for_change = items.clone();
										let project_name_signal = props.import_project_name;
										let repository_select = self::entity_select("Repository", "Select repository", self::repository_select_options(&items), props.import_repository_id, move |value| {
												if let Some(repository) = repositories_for_change.iter().find(|repository| repository.id.to_string() == value) {
													project_name_signal.set(repository.name.clone());
												}
											}, )
											;
											let import_field_errors = props.import_field_errors;
										page!({
											{ repository_select }
											{ self::import_field_error(import_field_errors, GitHubRepositoryImportRequestClientFormField::RepositoryId) }
										})
									}
									QueryStatus::Idle | QueryStatus::Pending => page!({
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Loading repositories..."
										}
									}),
									QueryStatus::Error => {
										let error = self::query_error_message(snapshot.error);
										page!({
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ error }
										}
										})
									},
								}
							}
							{
								let snapshot = clusters_for_import_refetch.snapshot();
								self::refetch_notice(
									snapshot.status == QueryStatus::Success && snapshot.is_fetching,
									snapshot.refetch_error,
									"clusters",
								)
							}
							{
								let snapshot = props.clusters_for_import.snapshot();
								match snapshot.status {
									QueryStatus::Success => {
										let cluster_select = self::entity_select("Cluster", "Select target cluster", self::cluster_select_options(&snapshot.data.unwrap_or_default()), props.import_cluster_id, |_value| {}, );
										let import_field_errors = props.import_field_errors;
										page!({
											{ cluster_select }
											{ self::import_field_error(import_field_errors, GitHubRepositoryImportRequestClientFormField::ClusterId) }
										})
									}
									QueryStatus::Idle | QueryStatus::Pending => page!({
										p {
											class: "mb-3 text-xs text-cloud-500",
											"Loading clusters..."
										}
									}),
									QueryStatus::Error => {
										let error = self::query_error_message(snapshot.error);
										page!({
										p {
											class: "mb-3 text-xs font-medium text-red-700",
											{ error }
										}
										})
									},
								}
							}
							{
								props.import_view.clone()
							}
							if props.import_submitting.get() {
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
								{
									let snapshot = clusters_for_inventory_refetch.snapshot();
									self::refetch_notice(
										snapshot.status == QueryStatus::Success && snapshot.is_fetching,
										snapshot.refetch_error,
										"clusters",
									)
								}
								div {
									class: "space-y-2 text-sm",
									{
										let snapshot = props.clusters_for_inventory.snapshot();
										match snapshot.status {
										QueryStatus::Idle | QueryStatus::Pending => page!({
											p {
												class: "text-cloud-500",
												"Loading clusters..."
											}
										}),
										QueryStatus::Error => {
											let err = self::query_error_message(snapshot.error);
											page!({
											p {
												class: "text-red-700",
												{ err }
											}
											})
										},
											QueryStatus::Success if snapshot.data.as_ref().is_some_and(Vec::is_empty) => page!({
											p {
												class: "text-cloud-500",
												"No active clusters."
											}
											}),
											QueryStatus::Success => {
												let items = snapshot.data.unwrap_or_default();
												page!({ {
											items.clone().into_iter().map(|cluster| {
												page!({
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
														}
														div {
															class: "font-mono text-xs text-cloud-500",
															{
																cluster.api_url.clone()
															}
														}
													}
												})
											}).collect::<Vec<_>>()
												} })
											},
									}
								}
							}
						}
					}
				}
			}
		}
	})
}

#[cfg(test)]
mod tests {
	#[cfg(native)]
	use std::cell::Cell;
	#[cfg(native)]
	use std::rc::Rc;

	use reinhardt::pages::reactive::ReactiveScope;
	use rstest::rstest;

	use crate::apps::github::server_fn::GitHubRepositoryImportRequest;

	use super::*;

	#[rstest]
	fn github_read_queries_use_generated_server_function_families() {
		// Arrange
		let repositories = list_github_repositories_for_current_org::query();
		let onboarding = get_github_onboarding_for_current_org::query();
		let previews = list_github_project_previews_for_current_org::query();

		// Act
		let repository_family = repositories.key().family_id();
		let onboarding_family = onboarding.key().family_id();
		let preview_family = previews.key().family_id();

		// Assert
		assert_eq!(
			repository_family,
			list_github_repositories_for_current_org::family().id()
		);
		assert_eq!(
			onboarding_family,
			get_github_onboarding_for_current_org::family().id()
		);
		assert_eq!(
			preview_family,
			list_github_project_previews_for_current_org::family().id()
		);
		assert_ne!(repository_family, onboarding_family);
		assert_ne!(repository_family, preview_family);
	}

	#[rstest]
	fn refetch_notice_preserves_the_background_failure() {
		// Arrange
		let error = ServerFnError::application("GitHub refresh timed out");

		// Act
		let html = refetch_notice(false, Some(error), "repositories").render_to_string();

		// Assert
		assert!(html.contains("Refresh failed: GitHub refresh timed out"));
	}

	#[rstest]
	fn refetch_notice_shows_cached_background_refresh() {
		// Act
		let html = refetch_notice(true, None, "repositories").render_to_string();

		// Assert
		assert!(html.contains("Refreshing repositories..."));
	}

	#[rstest]
	fn github_import_client_form_preserves_generated_request_fields() {
		// Arrange
		let expected = GitHubRepositoryImportRequest {
			repository_id: "101".to_string(),
			cluster_id: "202".to_string(),
			project_name: "reinhardt-cloud".to_string(),
			registry: "ghcr.io/kent8192/reinhardt-cloud".to_string(),
		};

		ReactiveScope::run(|| {
			let form =
				GitHubRepositoryImportRequestClientForm::new().with_defaults(expected.clone());
			let runtime = use_form(&form).build();

			// Act
			let request = GitHubRepositoryImportRequestClientForm::to_request(&runtime);

			// Assert
			assert_eq!(request, expected);
		});
	}

	#[rstest]
	fn github_import_client_form_maps_dto_validation_to_fields() {
		ReactiveScope::run(|| {
			// Arrange
			let form = GitHubRepositoryImportRequestClientForm::new();
			let runtime = use_form(&form).build();

			// Act
			let _error = runtime
				.trigger()
				.expect_err("empty import request must be rejected");

			// Assert
			assert_eq!(
				runtime
					.get_field_state(GitHubRepositoryImportRequestClientFormField::RepositoryId)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("Custom validation error: Select a repository")
			);
			assert_eq!(
				runtime
					.get_field_state(GitHubRepositoryImportRequestClientFormField::ClusterId)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("Custom validation error: Select a cluster")
			);
			assert_eq!(
				runtime
					.get_field_state(GitHubRepositoryImportRequestClientFormField::Registry)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("Custom validation error: Registry image prefix must be 1-512 characters")
			);
		});
	}

	#[rstest]
	fn github_import_client_form_routes_structured_server_errors_to_fields_and_global_error() {
		ReactiveScope::run(|| {
			// Arrange
			let form = GitHubRepositoryImportRequestClientForm::new().with_defaults(
				GitHubRepositoryImportRequest {
					repository_id: "101".to_string(),
					cluster_id: "202".to_string(),
					project_name: "reinhardt-cloud".to_string(),
					registry: "ghcr.io/kent8192/reinhardt-cloud".to_string(),
				},
			);
			let runtime = use_form(&form).build();
			let error = ServerFnError::validation_with_message(
				"Please correct the submitted values",
				[
					("registry", "Registry image prefix is unavailable"),
					("import_policy", "Organization policy rejected this import"),
				],
			);

			// Act
			runtime.apply_server_error(&error);

			// Assert
			assert_eq!(
				runtime
					.get_field_state(GitHubRepositoryImportRequestClientFormField::Registry)
					.error
					.as_ref()
					.map(FieldError::message),
				Some("Registry image prefix is unavailable")
			);
			assert_eq!(
				runtime.form_state().form_error.get(),
				Some(
					"Please correct the submitted values\nimport_policy: Organization policy rejected this import"
						.to_string()
				)
			);
		});
	}

	#[cfg(native)]
	#[rstest]
	#[tokio::test]
	async fn github_import_client_form_blocks_invalid_submission_before_server_dispatch() {
		// Arrange
		let scope = ReactiveScope::new();
		let runtime = scope.enter(|| {
			let form = GitHubRepositoryImportRequestClientForm::new();
			use_form(&form).build()
		});
		let submit_calls = Rc::new(Cell::new(0));
		let submit_calls_for_submit = Rc::clone(&submit_calls);

		// Act
		let outcome = runtime
			.submit_server_fn(move || {
				submit_calls_for_submit.set(submit_calls_for_submit.get() + 1);
				async { Ok::<_, ServerFnError>(()) }
			})
			.await
			.expect("validation rejection is a submit outcome");

		// Assert
		assert_eq!(outcome, UseFormAsyncSubmitOutcome::ValidationFailed);
		assert_eq!(submit_calls.get(), 0);
	}
}
