//! Shared immutable document publication and initial Pages hydration.

use std::sync::OnceLock;

use reinhardt::commands::StaticAssetSettings;
use reinhardt::pages::reactive::ReactiveScope;
use reinhardt::pages::ssr::{SsrOptions, SsrRenderer};
use reinhardt::utils::staticfiles::publication::{
	ManifestSnapshot, SnapshotOptions, render_entry_document,
};
use reinhardt::{Request, Response};

use crate::apps::identity::middleware::{CsrfToken, request_cookie};
use crate::client::components::shell::{STYLES, translations};
use crate::client::screens::{DashboardState, surface};
use crate::config::settings::get_scoped_settings;

const HEAD_SLOT: &str = "<!--cloud-dashboard-head-->";
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
	state: DashboardState,
	locale: &str,
) -> Result<String, &'static str> {
	render_document_with_csrf(template, state, locale, String::new()).await
}

async fn render_document_with_csrf(
	template: &str,
	state: DashboardState,
	locale: &str,
	csrf: String,
) -> Result<String, &'static str> {
	if template.matches(HEAD_SLOT).count() != 1
		|| template.matches(BODY_SLOT).count() != 1
		|| template.matches(STATE_SLOT).count() != 1
	{
		return Err("The Pages document must contain exactly one head, body and state slot");
	}
	let scope = ReactiveScope::new();
	let page = scope.enter(|| surface(state.clone(), translations(locale), csrf.clone()));
	let mut renderer =
		SsrRenderer::with_options(SsrOptions::new().i18n_context(translations(locale)));
	let body = renderer.render_view(&page).await;
	renderer
		.state_mut()
		.add_metadata("cloud-project-state", state);
	renderer
		.state_mut()
		.add_metadata("cloud-csrf", csrf.clone());
	let csrf_meta = format!(
		"<meta name=\"csrf-token\" content=\"{}\">",
		reinhardt::utils::escape_attr(&csrf)
	);
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
		.replacen(HEAD_SLOT, &csrf_meta, 1)
		.replacen(BODY_SLOT, &body, 1)
		.replacen(STATE_SLOT, &renderer.state().to_script_tag(), 1))
}

/// Render one authenticated App projection into the published Pages document.
pub async fn respond(request: Request, state: DashboardState) -> reinhardt::http::Result<Response> {
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
