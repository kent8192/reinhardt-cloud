//! Initial SSR projections and shared workspace framing.
use crate::apps::organization::services::OrganizationSummary;
use crate::apps::project::services::{ProjectDetail, ProjectSummary};
use crate::client::components::shell::{STYLES, shell, translations};
use reinhardt::pages::i18n::{I18nContext, use_i18n_context};
use reinhardt::pages::{Page, ServerFnError, page};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DashboardState {
	AuthenticationRequired,
	SignInFailed,
	OrganizationRequired(Vec<OrganizationSummary>),
	Forbidden,
	Unavailable,
	Ready(Vec<ProjectSummary>),
	Detail(Box<ProjectDetail>),
}
pub fn context() -> I18nContext {
	use_i18n_context().unwrap_or_else(|| translations("en"))
}
pub fn csrf_token() -> String {
	#[cfg(wasm)]
	{
		reinhardt::pages::hydration::HydrationContext::from_window()
			.ok()
			.and_then(|h| {
				h.get_metadata("cloud-csrf")
					.and_then(|v| v.as_str())
					.map(str::to_owned)
			})
			.unwrap_or_default()
	}
	#[cfg(server)]
	{
		String::new()
	}
}
pub fn initial_page() -> Page {
	#[cfg(wasm)]
	let state = reinhardt::pages::hydration::HydrationContext::from_window()
		.ok()
		.and_then(|h| h.get_metadata("cloud-project-state").cloned())
		.and_then(|v| serde_json::from_value(v).ok())
		.unwrap_or(DashboardState::AuthenticationRequired);
	#[cfg(server)]
	let state = DashboardState::AuthenticationRequired;
	surface(state, context(), csrf_token())
}
pub fn surface(state: DashboardState, context: I18nContext, csrf: String) -> Page {
	use crate::apps::{
		identity::client::components::sign_in::sign_in_form,
		organization::client::components::chooser::chooser,
		project::client::components::{detail::project_detail_view, projects::project_table},
	};
	let signed_in = !matches!(
		state,
		DashboardState::AuthenticationRequired | DashboardState::SignInFailed
	);
	let title = match &state {
		DashboardState::Detail(detail) => Some(detail.name.clone()),
		_ => None,
	};
	let content = match state {
		DashboardState::AuthenticationRequired => {
			sign_in_form(false, csrf.clone(), context.clone())
		}
		DashboardState::SignInFailed => sign_in_form(true, csrf.clone(), context.clone()),
		DashboardState::OrganizationRequired(items) => chooser(items, context.clone()),
		DashboardState::Ready(items) => project_table(items, context.clone()),
		DashboardState::Detail(detail) => project_detail_view(*detail, context.clone()),
		DashboardState::Forbidden => access_denied(context.clone()),
		DashboardState::Unavailable => unavailable(context.clone()),
	};
	shell(
		workspace(title, content, signed_in, context.clone(), csrf),
		context,
	)
}
pub fn workspace(
	title: Option<String>,
	content: Page,
	signed_in: bool,
	context: I18nContext,
	csrf: String,
) -> Page {
	page!({
		p {
			class: STYLES.eyebrow(),
			{ context.translate("Workspace") }
		}
		h1 {
			class: STYLES.title(),
			if let Some(title) = title.clone() { { title } } else { { context.translate("Projects") } }
		}
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
	})
}
pub fn access_denied(context: I18nContext) -> Page {
	page!({
		h2 {
			class: STYLES.panel_title(),
			{ context.translate("Access denied") }
		}
		p {
			class: STYLES.copy(),
			{ context.translate("Your current membership does not allow access to this organization.") }
		}
	})
}
pub fn unavailable(context: I18nContext) -> Page {
	page!({
		h2 {
			class: STYLES.panel_title(),
			{ context.translate("Unable to load projects") }
		}
		p {
			class: STYLES.copy(),
			{ context.translate("Try again when the service is available.") }
		}
	})
}
pub fn loading(context: I18nContext) -> Page {
	page!({
		p {
			role: "status",
			{ context.translate("Loading…") }
		}
	})
}
pub fn query_error(error: ServerFnError, context: I18nContext) -> Page {
	if error.status() == Some(401) {
		return crate::apps::identity::client::components::sign_in::sign_in_form(
			false,
			csrf_token(),
			context,
		);
	}
	if error.status() == Some(403) {
		return access_denied(context);
	}
	unavailable(context)
}
