//! OAuth provider discovery server function for frontend rendering.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[cfg(native)]
use reinhardt::di::Depends;
#[cfg(native)]
use reinhardt::urls::prelude::UnifiedRouter;

#[cfg(native)]
use crate::apps::auth::services::oauth::{OAuthSettings, OAuthSettingsKey};
#[cfg(native)]
use crate::apps::auth::urls as auth_urls;

/// Public OAuth provider metadata safe to expose to the browser.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OAuthProviderInfo {
	pub id: String,
	pub label: String,
	pub start_url: String,
}

#[cfg(native)]
pub(crate) fn label_for_provider(id: &str) -> &'static str {
	match id {
		"github" => "GitHub",
		_ => "OAuth",
	}
}

#[cfg(native)]
pub(crate) fn oauth_start_url(provider_id: &str) -> Result<String, ServerFnError> {
	const ROUTE_NAME: &str = "oauth-start";
	const PROVIDER_PARAM: &str = "provider_id";

	UnifiedRouter::new()
		.with_prefix("/api/")
		.mount_unified("/auth/", auth_urls::url_patterns())
		.into_server()
		.reverse(ROUTE_NAME, &[(PROVIDER_PARAM, provider_id)])
		.ok_or_else(|| {
			ServerFnError::application(format!(
				"failed to reverse `{ROUTE_NAME}` with `{PROVIDER_PARAM}` parameter"
			))
		})
}

/// Return the currently enabled OAuth providers.
#[server_fn]
pub async fn list_oauth_providers(
	#[inject] settings: Depends<OAuthSettingsKey, OAuthSettings>,
) -> Result<Vec<OAuthProviderInfo>, ServerFnError> {
	settings
		.enabled_provider_ids()
		.into_iter()
		.map(|id| {
			Ok(OAuthProviderInfo {
				id: id.to_string(),
				label: label_for_provider(id).to_string(),
				start_url: oauth_start_url(id)?,
			})
		})
		.collect::<Result<Vec<_>, ServerFnError>>()
}
