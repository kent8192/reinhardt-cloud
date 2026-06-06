//! OAuth provider discovery server function for frontend rendering.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

/// Public OAuth provider metadata safe to expose to the browser.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct OAuthProviderInfo {
	pub id: String,
	pub label: String,
}

pub(crate) fn label_for_provider(id: &str) -> &'static str {
	match id {
		"github" => "GitHub",
		_ => "OAuth",
	}
}

/// Return the currently enabled OAuth providers.
#[server_fn]
pub async fn list_oauth_providers(
	#[inject] settings: reinhardt::di::Depends<
		crate::apps::auth::services::oauth::config::OAuthSettings,
	>,
) -> Result<Vec<OAuthProviderInfo>, ServerFnError> {
	#[cfg(native)]
	{
		Ok(settings
			.enabled_provider_ids()
			.into_iter()
			.map(|id| OAuthProviderInfo {
				id: id.to_string(),
				label: label_for_provider(id).to_string(),
			})
			.collect())
	}
	#[cfg(wasm)]
	{
		let _ = settings;
		unreachable!("server_fn body is replaced on wasm")
	}
}
