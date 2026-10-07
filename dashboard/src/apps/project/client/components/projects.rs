//! Project list presentation with explicit access and failure states.

use reinhardt::pages::i18n::{I18nContext, use_i18n_context};
use reinhardt::pages::{Page, component, page};
use serde::{Deserialize, Serialize};

use crate::apps::deployment::services::OperationState;
use crate::apps::organization::services::OrganizationSummary;
use crate::apps::project::services::{
	EnvironmentKind, EnvironmentSummary, ProjectDetail, ProjectSummary,
};
use crate::client::components::shell::{STYLES, shell, translations};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectListState {
	AuthenticationRequired,
	SignInFailed,
	OrganizationRequired(Vec<OrganizationSummary>),
	Forbidden,
	Unavailable,
	Ready(Vec<ProjectSummary>),
	Detail(Box<ProjectDetail>),
}

#[component("/", name = "projects")]
pub fn projects() -> Page {
	initial_project_page()
}

pub fn initial_project_page() -> Page {
	let context = use_i18n_context().unwrap_or_else(|| translations("en"));
	#[cfg(wasm)]
	let (state, csrf) = reinhardt::pages::hydration::HydrationContext::from_window()
		.ok()
		.map(|hydration| {
			let state = hydration
				.get_metadata("cloud-project-state")
				.cloned()
				.and_then(|state| serde_json::from_value(state).ok())
				.unwrap_or(ProjectListState::AuthenticationRequired);
			let csrf = hydration
				.get_metadata("cloud-csrf")
				.and_then(|value| value.as_str())
				.unwrap_or("")
				.to_owned();
			(state, csrf)
		})
		.unwrap_or((ProjectListState::AuthenticationRequired, String::new()));
	#[cfg(not(wasm))]
	let (state, csrf) = (ProjectListState::AuthenticationRequired, String::new());
	project_surface(state, context, csrf)
}

pub fn project_list(state: ProjectListState, locale: &str) -> Page {
	let context = translations(locale);
	project_list_with_context(state, context)
}

pub fn project_list_with_context(state: ProjectListState, context: I18nContext) -> Page {
	project_surface(state, context, String::new())
}

pub fn project_surface(state: ProjectListState, context: I18nContext, csrf: String) -> Page {
	let signed_in = !matches!(
		state,
		ProjectListState::AuthenticationRequired | ProjectListState::SignInFailed
	);
	let failed = state == ProjectListState::SignInFailed;
	let heading = match &state {
		ProjectListState::Detail(detail) => {
			let name = detail.name.clone();
			page!({ h1 { class: STYLES.title(), { name } } })
		}
		_ => page!({ h1 { class: STYLES.title(), { context.translate("Projects") } } }),
	};
	let login_csrf = csrf.clone();
	let content = match state {
		ProjectListState::AuthenticationRequired | ProjectListState::SignInFailed => page!({
			p {
				class: STYLES.status(),
				{ context.translate("Authentication required") }
			}
			h2 {
				class: STYLES.panel_title(),
				{ context.translate("Sign in to your organization") }
			}
			p {
				class: STYLES.copy(),
				{ context.translate(
					"Your projects and environments are visible after your organization membership is verified.",
				) }
			}
			if failed {
				p {
					role: "alert",
					class: STYLES.copy(),
					{ context.translate("The email or password is incorrect.") }
				}
			}
			form {
				action: "/login/",
				method: "post",
				class: STYLES.form(),
				input {
					type: "hidden",
					name: "csrf_token",
					value: login_csrf
				}
				label {
					for: "email",
					class: STYLES.label(),
					{ context.translate("Email") }
				}
				input {
					id: "email",
					name: "email",
					type: "email",
					autocomplete: "username",
					required: true,
					class: STYLES.input()
				}
				label {
					for: "password",
					class: STYLES.label(),
					{ context.translate("Password") }
				}
				input {
					id: "password",
					name: "password",
					type: "password",
					autocomplete: "current-password",
					required: true,
					class: STYLES.input()
				}
				button {
					type: "submit",
					class: STYLES.primary(),
					{ context.translate("Sign in") }
				}
			}
		}),
		ProjectListState::OrganizationRequired(organizations) => page!({
			h2 {
				class: STYLES.panel_title(),
				{ context.translate("Choose an organization") }
			}
			ul {
				for organization in organizations @key(organization.id) {
					li {
						a {
							class: STYLES.link(),
							href: format!("/?organization={}", organization.id),
							{ organization.name }
						}
					}
				}
			}
		}),
		ProjectListState::Forbidden => page!({
			h2 {
				class: STYLES.panel_title(),
				{ context.translate("Access denied") }
			}
			p {
				class: STYLES.copy(),
				{ context.translate("Your current membership does not allow access to this organization.") }
			}
		}),
		ProjectListState::Unavailable => page!({
			h2 {
				class: STYLES.panel_title(),
				{ context.translate("Unable to load projects") }
			}
			p {
				class: STYLES.copy(),
				{ context.translate("Try again when the service is available.") }
			}
		}),
		ProjectListState::Ready(items) if items.is_empty() => page!({
			h2 {
				class: STYLES.panel_title(),
				{ context.translate("No projects yet") }
			}
			p {
				class: STYLES.copy(),
				{ context.translate("Connect a repository to publish your first application.") }
			}
		}),
		ProjectListState::Ready(items) => page!({
			table {
				class: STYLES.table(),
				thead {
					tr {
						th {
							scope: "col",
							{ context.translate("Project") }
						}
						th {
							scope: "col",
							{ context.translate("Repository") }
						}
						th {
							scope: "col",
							{ context.translate("Environments") }
						}
					}
				}
				tbody {
					for item in items @key(item.id) {
						tr {
							td {
								a {
									class: STYLES.link(),
									href: format!("/?organization={}&project={}", item.organization_id, item.id),
									{ item.name }
									}
							}
							td { { item.repository } }
							td { { format!("{}", item.environments) } }
						}
					}
				}
			}
		}),
		ProjectListState::Detail(detail) => project_detail_view(*detail, context.clone()),
	};
	let body = page!({
		p {
			class: STYLES.eyebrow(),
			{ context.translate("Workspace") }
		}
		{ heading }
		p {
			class: STYLES.description(),
			{ context.translate("Your applications, from source to production.") }
		}
		section {
			class: STYLES.panel(),
			{ content }
		}
		if signed_in {
			form {
				action: "/logout/",
				method: "post",
				class: STYLES.footer(),
				input {
					type: "hidden",
					name: "csrf_token",
					value: csrf
				}
				button {
					type: "submit",
					class: STYLES.language(),
					{ context.translate("Sign out") }
				}
			}
		}
	});
	shell(body, context)
}

fn project_detail_view(detail: ProjectDetail, context: I18nContext) -> Page {
	let ProjectDetail {
		organization_id,
		repository,
		environments: rows,
		..
	} = detail;
	let empty = rows.is_empty();
	let environments = rows
		.into_iter()
		.map(|environment| {
			(
				environment.id,
				environment_card(environment, context.clone()),
			)
		})
		.collect::<Vec<_>>();
	page!({
		nav {
			aria_label: "Project navigation",
			a {
				class: STYLES.link(),
				href: format!("/?organization={}", organization_id),
				{ context.translate("All projects") }
			}
		}
		p { class: STYLES.copy(), { repository } }
		h2 { class: STYLES.panel_title(), { context.translate("Environments") } }
		p { class: STYLES.copy(), { context.translate("Desired inputs. Readiness is reported separately.") } }
		if empty {
			p { class: STYLES.copy(), { context.translate("No environments yet") } }
		}
		div {
			class: STYLES.environment_grid(),
			for environment in environments @key(environment.0) {
				{ environment.1 }
			}
		}
	})
}

fn environment_card(environment: EnvironmentSummary, context: I18nContext) -> Page {
	let EnvironmentSummary {
		id,
		kind: environment_kind,
		version,
		desired_runtime,
		latest_operation,
	} = environment;
	let kind = match environment_kind {
		EnvironmentKind::Production => "Production",
		EnvironmentKind::Staging => "Staging",
		EnvironmentKind::Preview => "Preview",
	};
	let operation = match latest_operation {
		None => page!({ p { class: STYLES.copy(), { context.translate("No operations yet") } } }),
		Some(snapshot) => {
			let state = match snapshot.progress.state {
				OperationState::Queued => "Queued",
				OperationState::Building => "Building",
				OperationState::Migrating => "Migrating",
				OperationState::Applying => "Applying",
				OperationState::Verifying => "Verifying",
				OperationState::Uncertain => "Outcome uncertain",
				OperationState::Succeeded => "Succeeded",
				OperationState::Failed => "Failed",
				OperationState::Cancelled => "Cancelled",
			};
			let kind = match snapshot.kind.as_str() {
				"restart" => "Restart",
				"scale" => "Scale",
				"autoscale" => "CPU autoscaling",
				_ => "Operation",
			};
			page!({
				h4 { class: STYLES.operation_title(), { context.translate("Latest operation") } }
				p { class: STYLES.status(), { context.translate(kind) } ": " { context.translate(state) } }
				p { class: STYLES.identity(), { format!("{}", snapshot.progress.id) } }
				time { datetime: snapshot.created_at.to_rfc3339(), { snapshot.created_at.format("%Y-%m-%d %H:%M UTC").to_string() } }
				if snapshot.progress.state == OperationState::Uncertain {
					p { role: "status", class: STYLES.copy(), { context.translate("Further changes wait for cluster reconciliation.") } }
				}
			})
		}
	};
	let replicas = desired_runtime.replicas;
	let autoscaling = desired_runtime.autoscaling;
	let autoscaling_enabled = autoscaling.is_some();
	let range = autoscaling
		.as_ref()
		.map(|configuration| {
			format!(
				"{}–{} · {}% CPU",
				configuration.min_replicas,
				configuration.max_replicas,
				configuration.target_cpu_percent
			)
		})
		.unwrap_or_default();
	page!({
		article {
			class: STYLES.environment_card(),
			h3 { class: STYLES.panel_title(), { context.translate(kind) } }
			p { class: STYLES.identity(), { format!("{}", id) } }
			dl {
				class: STYLES.facts(),
				dt { { context.translate("Configuration version") } }
				dd { { format!("{}", version) } }
				dt { { context.translate("Desired replicas") } }
				dd { { format!("{}", replicas) } }
				if autoscaling_enabled {
					dt { { context.translate("CPU autoscaling") } }
					dd { { range } }
				}
			}
			{ operation }
		}
	})
}
