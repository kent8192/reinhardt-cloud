//! Linked OAuth account server function.

use reinhardt::pages::server_fn::{ServerFnError, server_fn};
use serde::{Deserialize, Serialize};

#[cfg(native)]
use reinhardt::CurrentUser;
#[cfg(native)]
use reinhardt::db::orm::Model;

#[cfg(native)]
use crate::apps::auth::models::{SocialAccount, User};
#[cfg(native)]
use crate::apps::auth::server_fn::oauth_providers::label_for_provider;

/// Linked OAuth account metadata safe to expose to the browser.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LinkedOAuthAccountInfo {
	pub provider: String,
	pub label: String,
	pub provider_username: Option<String>,
}

/// Return OAuth accounts linked to the current user.
#[server_fn]
pub async fn list_linked_oauth_accounts(
	#[inject] CurrentUser(user): CurrentUser<User>,
) -> Result<Vec<LinkedOAuthAccountInfo>, ServerFnError> {
	let rows = SocialAccount::objects()
		.filter(SocialAccount::field_user_id().eq(user.id.to_string()))
		.all()
		.await
		.map_err(|err| {
			tracing::error!("Failed to load linked OAuth accounts: {err}");
			ServerFnError::application("Internal server error")
		})?;

	Ok(rows
		.into_iter()
		.map(|account| LinkedOAuthAccountInfo {
			label: label_for_provider(&account.provider).to_string(),
			provider: account.provider,
			provider_username: account.provider_username,
		})
		.collect())
}
