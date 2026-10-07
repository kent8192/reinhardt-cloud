//! Native Pages document serving for the project App.

use std::sync::OnceLock;

use reinhardt::commands::StaticAssetSettings;
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::ssr::{SsrOptions, SsrRenderer};
use reinhardt::utils::staticfiles::publication::{
	ManifestSnapshot, SnapshotOptions, render_entry_document,
};
use reinhardt::{Request, Response, get};

use crate::apps::identity::{
	middleware::{CsrfToken, request_cookie},
	persistence::Actor,
};
use crate::apps::organization::persistence::{OrganizationError, organizations_for};
use crate::apps::project::client::components::projects::ProjectListState;
use crate::apps::project::client::components::projects::project_surface;
use crate::apps::project::persistence::{project_detail, projects_for};
use crate::client::components::shell::{STYLES, translations};
use crate::config::settings::get_scoped_settings;

const BODY_SLOT: &str = "<!--cloud-dashboard-body-->";
const STATE_SLOT: &str = "<!--cloud-dashboard-state-->";
static DOCUMENT: OnceLock<Result<String, String>> = OnceLock::new();

fn published_document() -> Result<&'static str, &'static str> {
	DOCUMENT
		.get_or_init(|| {
			let scoped = get_scoped_settings().map_err(|error| error.to_string())?;
			let assets =
				StaticAssetSettings::from_scoped(&scoped).map_err(|error| error.to_string())?;
			let snapshot =
				ManifestSnapshot::load(&assets.static_root, &SnapshotOptions::production())
					.map_err(|error| error.to_string())?;
			let entries = &snapshot.manifest().entrypoints;
			if entries.len() != 1 {
				return Err("Exactly one Dashboard entrypoint is required".into());
			}
			let (name, entry) = entries.first_key_value().expect("one entrypoint");
			let document = entry
				.document
				.as_ref()
				.ok_or("A Pages document is required")?;
			let template = String::from_utf8(
				snapshot
					.read_asset(document)
					.map_err(|error| error.to_string())?,
			)
			.map_err(|error| error.to_string())?;
			render_entry_document(&snapshot, &assets.static_url, name, &template)
				.map_err(|error| error.to_string())
		})
		.as_ref()
		.map(String::as_str)
		.map_err(String::as_str)
}

/// Insert escaped Pages output and framework hydration state into a published shell.
pub async fn render_document(
	template: &str,
	state: ProjectListState,
	locale: &str,
) -> Result<String, &'static str> {
	render_document_with_csrf(template, state, locale, String::new()).await
}

async fn render_document_with_csrf(
	template: &str,
	state: ProjectListState,
	locale: &str,
	csrf: String,
) -> Result<String, &'static str> {
	if template.matches(BODY_SLOT).count() != 1 || template.matches(STATE_SLOT).count() != 1 {
		return Err("The Pages document must contain exactly one body and state slot");
	}
	let scope = ReactiveScope::new();
	let page = scope.enter(|| project_surface(state.clone(), translations(locale), csrf.clone()));
	let mut renderer =
		SsrRenderer::with_options(SsrOptions::new().i18n_context(translations(locale)));
	let body = renderer.render_view(&page).await;
	renderer
		.state_mut()
		.add_metadata("cloud-project-state", state);
	renderer.state_mut().add_metadata("cloud-csrf", csrf);
	Ok(template
		.replacen(
			"<html lang=\"en\">",
			if locale == "ja" {
				"<html lang=\"ja\">"
			} else {
				"<html lang=\"en\">"
			},
			1,
		)
		.replacen(
			"class=\"cloud-document\"",
			&format!("class=\"{}\"", STYLES.document().as_str()),
			1,
		)
		.replacen(BODY_SLOT, &body, 1)
		.replacen(STATE_SLOT, &renderer.state().to_script_tag(), 1))
}

#[get("/", name = "project-index")]
pub async fn index(request: Request) -> reinhardt::http::Result<Response> {
	#[derive(serde::Deserialize)]
	struct Query {
		organization: Option<uuid::Uuid>,
		project: Option<uuid::Uuid>,
		signin: Option<String>,
	}
	let Ok(query) = request.query_as::<Query>() else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	let state = match request.extensions.get::<Actor>() {
		None if query.signin.as_deref() == Some("failed") => ProjectListState::SignInFailed,
		None => ProjectListState::AuthenticationRequired,
		Some(actor) => {
			if let Some(organization) = query.organization {
				if let Some(project) = query.project {
					match project_detail(actor.id, organization, project).await {
						Ok(detail) => ProjectListState::Detail(Box::new(detail)),
						Err(OrganizationError::Forbidden) => ProjectListState::Forbidden,
						Err(_) => ProjectListState::Unavailable,
					}
				} else {
					match projects_for(actor.id, organization).await {
						Ok(items) => ProjectListState::Ready(items),
						Err(OrganizationError::Forbidden) => ProjectListState::Forbidden,
						Err(_) => ProjectListState::Unavailable,
					}
				}
			} else {
				match organizations_for(actor.id).await {
					Ok(items) => ProjectListState::OrganizationRequired(items),
					Err(_) => ProjectListState::Unavailable,
				}
			}
		}
	};
	let csrf = request
		.extensions
		.get::<CsrfToken>()
		.map(|token| token.0)
		.unwrap_or_default();
	let locale = request_cookie(&request, "cloud_locale")
		.filter(|locale| matches!(locale.as_str(), "en" | "ja"))
		.unwrap_or_else(|| "en".into());
	// Pages reactive handles are local to the rendering thread. Only the completed
	// HTML crosses back into the HTTP server's Send future.
	let rendered = tokio::task::spawn_blocking(move || {
		let template = published_document()?;
		let runtime = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.map_err(|_| "Cannot initialize the Pages renderer")?;
		runtime.block_on(render_document_with_csrf(template, state, &locale, csrf))
	})
	.await;
	let response = match rendered {
		Ok(Ok(html)) => Response::ok()
			.with_header("Content-Type", "text/html; charset=utf-8")
			.with_header("Cache-Control", "no-store")
			.with_body(html),
		_ => Response::new(http::StatusCode::SERVICE_UNAVAILABLE).with_body(
			"The Dashboard assets are unavailable. Build static assets and restart the server.",
		),
	};
	Ok(response)
}

#[get("/api/v1/project/", name = "project-detail")]
pub async fn detail(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::UNAUTHORIZED));
	};
	#[derive(serde::Deserialize)]
	struct Query {
		organization: uuid::Uuid,
		project: uuid::Uuid,
	}
	let Ok(query) = request.query_as::<Query>() else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	match project_detail(actor.id, query.organization, query.project).await {
		Ok(detail) => Ok(Response::ok().with_json(&detail)?),
		Err(OrganizationError::Forbidden) => Ok(Response::new(http::StatusCode::FORBIDDEN)),
		Err(_) => Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)),
	}
}

#[get("/api/v1/projects/", name = "project-list")]
pub async fn list(request: Request) -> reinhardt::http::Result<Response> {
	let Some(actor) = request.extensions.get::<Actor>() else {
		return Ok(Response::new(http::StatusCode::UNAUTHORIZED));
	};
	#[derive(serde::Deserialize)]
	struct Query {
		organization: uuid::Uuid,
	}
	let Ok(query) = request.query_as::<Query>() else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	match projects_for(actor.id, query.organization).await {
		Ok(projects) => Ok(Response::ok()
			.with_header("Cache-Control", "no-store")
			.with_json(&projects)?),
		Err(OrganizationError::Forbidden) => Ok(Response::new(http::StatusCode::FORBIDDEN)),
		Err(_) => Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)),
	}
}
