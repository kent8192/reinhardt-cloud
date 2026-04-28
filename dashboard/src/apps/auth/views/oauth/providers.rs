//! `GET /oauth/providers/` — discovery endpoint.
//!
//! Returns a JSON list of currently-enabled OAuth providers so the WASM
//! client can render only the buttons that have credentials configured.
//! Crucially, this endpoint MUST NOT leak any provider secrets, redirect
//! URIs, or client IDs into the response — it ships only `id` (lowercase
//! provider slug) and `label` (the human-readable button text).

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::{Response, StatusCode, get};
use serde::Serialize;

use crate::apps::auth::services::oauth::config::OAuthSettings;

#[derive(Debug, Serialize)]
pub struct ProviderEntry {
	pub id: &'static str,
	pub label: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ProvidersResponse {
	pub providers: Vec<ProviderEntry>,
}

/// Map a provider id to its human label. Lowercase ids only — the slug is
/// the canonical identifier used in URLs.
pub fn label_for(id: &str) -> &'static str {
	match id {
		"github" => "GitHub",
		_ => "OAuth",
	}
}

#[get("/oauth/providers/", name = "oauth_providers")]
pub async fn oauth_providers() -> ViewResult<Response> {
	let settings = OAuthSettings::from_env();
	let providers: Vec<ProviderEntry> = settings
		.enabled_provider_ids()
		.into_iter()
		.map(|id| ProviderEntry {
			id,
			label: label_for(id),
		})
		.collect();
	let resp = ProvidersResponse { providers };
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp).map_err(AppError::from)?))
}
