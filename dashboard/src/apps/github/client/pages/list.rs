//! GitHub repository import page.

use std::collections::HashMap;

use reinhardt::pages::component;
use reinhardt::pages::component::Page;
use reinhardt::pages::event::SubmitEvent;
use reinhardt::pages::page;
use reinhardt::pages::prelude::{
	Callback, FieldError, QueryHandle, QueryOptions, QueryStatus, Signal, queries, use_form,
	use_query,
};
use reinhardt::pages::server_fn::ServerFnError;

use crate::apps::clusters::server_fn::{ClusterInfo, list_clusters_for_current_org};
use crate::apps::deployments::client::components::preview_list::{
	render_preview_list, render_project_identity,
};
use crate::apps::deployments::server_fn::ProjectPreviewSummary;
use crate::apps::deployments::server_fn::{
	list_deployment_previews_for_current_org, list_deployments_for_current_org,
};
use crate::apps::github::client::style::STYLES;
use crate::apps::github::server_fn::list_github_repositories_for_installation;
use crate::apps::github::server_fn::{
	GitHubOnboardingInfo, GitHubRepositoryImportRequestClientForm,
	GitHubRepositoryImportRequestClientFormField, GitHubRepositoryInfo,
	get_github_onboarding_for_current_org, list_github_project_previews_for_current_org,
	list_github_repositories_for_current_org,
};
use crate::shared::client::components::entity_select::{EntitySelectOption, entity_select};
use crate::shared::client::style::STYLES as SHARED_STYLES;

fn alert(error: Signal<Option<String>>) -> Page {
	page!({
		{
			error
				.get()
				.map(|message| {
					page!({
						div {
							class: STYLES.alert(),
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
				class: STYLES.refetch_notice() + STYLES.refetch_warning(),
				{ message }
			}
		});
	}
	if is_fetching {
		return page!({
			div {
				class: STYLES.refetch_notice() + STYLES.refetch_pending(),
				"Refreshing " { label }"..."
			}
		});
	}
	Page::Empty
}

fn query_error_message(error: Option<ServerFnError>) -> String {
	error.map_or_else(String::new, |error| error.user_message().to_owned())
}

fn render_imported_projects_initial_state(status: QueryStatus) -> Option<Page> {
	match status {
		QueryStatus::Idle | QueryStatus::Pending => Some(page!({
			div {
				class: SHARED_STYLES.empty(),
				"Loading imported projects..."
			}
		})),
		QueryStatus::Error => Some(page!({
			div {
				class: STYLES.query_notice() + STYLES.query_warning(),
				"Imported projects are temporarily unavailable"
			}
		})),
		QueryStatus::Success => None,
	}
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
							class: STYLES.field_error(),
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
			class: STYLES.project_card(),
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
#[component("github", name = "github:repositories")]
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
	let import_field_errors = import_state.field_errors;
	let query_client = queries();
	let import_action = import_form
		.server_mutation(&import_runtime)
		.invalidate_family(
			query_client.clone(),
			list_github_repositories_for_current_org::family(),
		)
		.invalidate_family(
			query_client.clone(),
			list_github_repositories_for_installation::family(),
		)
		.invalidate_family(
			query_client.clone(),
			list_github_project_previews_for_current_org::family(),
		)
		.invalidate_family(
			query_client.clone(),
			list_deployments_for_current_org::family(),
		)
		.invalidate_family(
			query_client,
			list_deployment_previews_for_current_org::family(),
		)
		.build();
	let submit_import = Callback::new(move |event: SubmitEvent| {
		event.prevent_default();
		GitHubRepositoryImportRequestClientForm::normalize_values(&import_action.form());
		import_action.dispatch();
	});
	let import_view = page!({
		form {
			class: SHARED_STYLES.form_stack(),
			@submit: submit_import,
			div {
				class: SHARED_STYLES.field(),
				label {
					class: SHARED_STYLES.label(),
					"Project name"
				}
				input {
					aria_label: "Project name",
					class: SHARED_STYLES.input(),
					type: "text",
					placeholder: "leave blank to derive from repository",
					bind: import_runtime.field(GitHubRepositoryImportRequestClientFormField::ProjectName),
				}
				{ self::import_field_error(
					import_field_errors,
					GitHubRepositoryImportRequestClientFormField::ProjectName,
				) }
			}
			div {
				class: SHARED_STYLES.field(),
				label {
					class: SHARED_STYLES.label(),
					"Registry Image Prefix"
				}
				input {
					aria_label: "Registry Image Prefix",
					class: SHARED_STYLES.input(),
					type: "text",
					placeholder: "ghcr.io/kent8192/my-app",
					bind: import_runtime.field(GitHubRepositoryImportRequestClientFormField::Registry),
				}
				{ self::import_field_error(
					import_field_errors,
					GitHubRepositoryImportRequestClientFormField::Registry,
				) }
			}
			button {
				type: "submit",
				class: SHARED_STYLES.button_primary() + STYLES.form_submit(),
				disabled: import_state.is_submitting.get(),
				{
					if import_state.is_submitting.get() { "Importing..." } else { "Import repository" }
				}
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
			class: SHARED_STYLES.shell(),
			div {
				div {
					class: SHARED_STYLES.topline(),
					div {
						p {
							class: SHARED_STYLES.kicker(),
							"Source Control"
						}
						h1 {
							class: SHARED_STYLES.title(),
							"GitHub Repositories"
						}
						p {
							class: SHARED_STYLES.muted() + STYLES.page_intro(),
							"Import GitHub App repositories into Reinhardt Cloud deployments."
						}
					}
				}
				div {
					class: STYLES.page_layout(),
					section {
						class: STYLES.content_stack(),
						section {
							class: SHARED_STYLES.panel(),
							div {
								class: SHARED_STYLES.panel_head(),
								"Imported Projects"
							}
							div {
								class: STYLES.panel_body(),
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
									match render_imported_projects_initial_state(snapshot.status) {
										Some(initial_state) => initial_state,
										None => match snapshot.data {
											Some(items) if items.is_empty() => page!({
												div {
													class: SHARED_STYLES.empty(),
													"No imported projects yet"
												}
											}),
											Some(items) => page!({
											div {
												class: STYLES.project_grid(),
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
							class: SHARED_STYLES.panel(),
							div {
								class: SHARED_STYLES.panel_head() + STYLES.inventory_head(),
								span { "Repository Inventory" }
								span {
									class: STYLES.github_badge(),
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
								class: STYLES.inventory_scroll(),
								table {
									class: SHARED_STYLES.table(),
									thead {
										class: STYLES.inventory_header(),
										tr {
											th {
												class: SHARED_STYLES.table_header(),
												"ID"
											}
											th {
												class: SHARED_STYLES.table_header(),
												"Repository"
											}
											th {
												class: SHARED_STYLES.table_header(),
												"Branch"
											}
											th {
												class: SHARED_STYLES.table_header(),
												"State"
											}
										}
									}
									tbody {
										class: STYLES.inventory_body(),
										{
											let snapshot = props.repositories_for_inventory.snapshot();
											match snapshot.status {
											QueryStatus::Idle | QueryStatus::Pending => page!({
												tr {
													td {
														class: SHARED_STYLES.empty(),
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
														class: STYLES.query_notice() + STYLES.query_error(),
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
														class: SHARED_STYLES.empty(),
														colspan: 4,
														{
															let snapshot = onboarding.snapshot();
															match snapshot.status {
																QueryStatus::Success if snapshot.data.as_ref().is_some_and(|info| !info.github_account_linked) => page!({
																	div {
																		class: STYLES.onboarding_action(),
																		span { "Link your GitHub account before installing the GitHub App." }
																		a {
																			class: SHARED_STYLES.button_secondary() + STYLES.onboarding_button(),
														href: "/api/auth/oauth/github/start/?intent=link",
																			"Link GitHub account"
																		}
																	}
																}),
																QueryStatus::Success => {
																	if let Some(url) = snapshot.data.and_then(|info| info.install_url) {
																		page!({
																	div {
																		class: STYLES.onboarding_action(),
																				span { "No GitHub App repositories are available." }
																		a {
																			class: SHARED_STYLES.button_secondary() + STYLES.onboarding_button(),
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
															class: STYLES.inventory_row(),
															td {
																class: SHARED_STYLES.table_cell() + STYLES.inventory_id(),
																{
																	repo.id.to_string()
																}
															}
															td {
																class: SHARED_STYLES.table_cell(),
																div {
																	class: STYLES.repository_name(),
																	{
																		repo.full_name.clone()
																	}
																}
																div {
																	class: STYLES.repository_visibility(),
																	{
																		if repo.private { "private" } else { "public" }
																	}
																}
															}
															td {
																class: SHARED_STYLES.table_cell() + STYLES.inventory_id(),
																{
																	repo.default_branch.clone()
																}
															}
															td {
																class: SHARED_STYLES.table_cell(),
																span {
																	class: if repo.selected {
																		STYLES.repository_state() + STYLES.repository_state_imported()
																	} else {
																		STYLES.repository_state() + STYLES.repository_state_available()
																	},
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
						class: SHARED_STYLES.stack(),
						section {
							class: SHARED_STYLES.panel_pad(),
							h2 {
								class: STYLES.aside_title(),
								"Import"
							}
							div {
								class: STYLES.import_selection(),
								div {
									class: STYLES.import_selection_row(),
									span {
										class: STYLES.import_selection_label(),
										"Repository"
									}
									span {
										class: STYLES.import_selection_value(),
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
									class: STYLES.import_selection_row(),
									span {
										class: STYLES.import_selection_label(),
										"Cluster"
									}
									span {
										class: STYLES.import_selection_value(),
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
									class: STYLES.import_selection_row(),
									span {
										class: STYLES.import_selection_label(),
										"App"
									}
									span {
										class: STYLES.import_selection_name(),
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
										class: STYLES.import_pending(),
											"Loading repositories..."
										}
									}),
									QueryStatus::Error => {
										let error = self::query_error_message(snapshot.error);
										page!({
									p {
										class: STYLES.import_error(),
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
										class: STYLES.import_pending(),
											"Loading clusters..."
										}
									}),
									QueryStatus::Error => {
										let error = self::query_error_message(snapshot.error);
										page!({
									p {
										class: STYLES.import_error(),
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
									class: STYLES.action_status(),
									"Importing..."
								}
							}
						}
						section {
							class: SHARED_STYLES.panel_pad(),
								h2 {
									class: STYLES.aside_title(),
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
									class: STYLES.cluster_list(),
									{
										let snapshot = props.clusters_for_inventory.snapshot();
										match snapshot.status {
										QueryStatus::Idle | QueryStatus::Pending => page!({
											p {
												class: STYLES.cluster_empty(),
												"Loading clusters..."
											}
										}),
										QueryStatus::Error => {
											let err = self::query_error_message(snapshot.error);
											page!({
											p {
												class: STYLES.cluster_error(),
												{ err }
											}
											})
										},
											QueryStatus::Success if snapshot.data.as_ref().is_some_and(Vec::is_empty) => page!({
											p {
												class: STYLES.cluster_empty(),
												"No active clusters."
											}
											}),
											QueryStatus::Success => {
												let items = snapshot.data.unwrap_or_default();
												page!({ {
											items.clone().into_iter().map(|cluster| {
												page!({
													div {
														class: STYLES.cluster_card(),
														div {
															class: STYLES.cluster_card_head(),
															div {
																class: STYLES.cluster_card_body(),
																div {
																	class: STYLES.cluster_name(),
																	{
																		cluster.name.clone()
																	}
																}
																div {
																	class: STYLES.cluster_id(),
																	{
																		format!("id {}", cluster.id)
																	}
																}
															}
														}
														div {
															class: STYLES.cluster_url(),
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
	use reinhardt::pages::prelude::UseFormAsyncSubmitOutcome;
	#[cfg(native)]
	use std::cell::Cell;
	#[cfg(native)]
	use std::rc::Rc;

	use reinhardt::pages::reactive::ReactiveScope;
	use rstest::rstest;

	use crate::apps::github::server_fn::GitHubRepositoryImportRequest;

	use super::*;

	#[cfg(native)]
	#[rstest]
	fn native_import_mutation_preserves_form_values_without_dispatch() {
		ReactiveScope::run(|| {
			// Arrange
			let form = GitHubRepositoryImportRequestClientForm::new();
			let runtime = use_form(&form).build();
			runtime.set_value(
				GitHubRepositoryImportRequestClientFormField::Registry,
				"ghcr.io/acme".to_owned(),
			);
			let mutation = form.server_mutation(&runtime).build();

			// Act
			let outcome = mutation.dispatch();

			// Assert
			assert_eq!(
				outcome,
				reinhardt::pages::MutationDispatchOutcome::UnsupportedTarget
			);
			assert_eq!(mutation.is_pending(), false);
			assert_eq!(runtime.form_state().is_submitting.get(), false);
			assert_eq!(runtime.form_state().field_errors.get().len(), 0);
			assert_eq!(
				GitHubRepositoryImportRequestClientForm::to_request(&runtime).registry,
				"ghcr.io/acme"
			);
		});
	}

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
		assert_eq!(
			html,
			format!(
				"<div class=\"{} {}\">Refresh failed: GitHub refresh timed out</div>",
				STYLES.refetch_notice().as_str(),
				STYLES.refetch_warning().as_str(),
			)
		);
	}

	#[rstest]
	fn refetch_notice_shows_cached_background_refresh() {
		// Act
		let html = refetch_notice(true, None, "repositories").render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<div class=\"{} {}\">Refreshing repositories...</div>",
				STYLES.refetch_notice().as_str(),
				STYLES.refetch_pending().as_str(),
			)
		);
	}

	#[rstest]
	#[case::idle(QueryStatus::Idle)]
	#[case::pending(QueryStatus::Pending)]
	fn imported_projects_initial_state_renders_loading(#[case] status: QueryStatus) {
		// Act
		let html = render_imported_projects_initial_state(status)
			.expect("initial state should render")
			.render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<div class=\"{}\">Loading imported projects...</div>",
				SHARED_STYLES.empty().as_str(),
			)
		);
	}

	#[rstest]
	fn imported_projects_initial_state_renders_error() {
		// Act
		let html = render_imported_projects_initial_state(QueryStatus::Error)
			.expect("initial state should render")
			.render_to_string();

		// Assert
		assert_eq!(
			html,
			format!(
				"<div class=\"{} {}\">Imported projects are temporarily unavailable</div>",
				STYLES.query_notice().as_str(),
				STYLES.query_warning().as_str(),
			)
		);
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
