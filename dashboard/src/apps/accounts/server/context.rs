//! The services the accounts application's handlers share.
//!
//! [`AccountsServices`] is built once, when the router is assembled, and
//! registered as a DI singleton; handlers receive it with `#[inject]`.

use std::sync::Arc;

use async_trait::async_trait;
use reinhardt::di::{DiError, DiResult, Injectable, InjectionContext};

use crate::apps::accounts::server::cookies::CookieSecurity;
use crate::apps::accounts::services::server::github::GithubSignIn;
use crate::apps::accounts::services::server::provider_token_refresh::{
	ProviderTokenRefresher, ProviderTokenService,
};
use crate::apps::accounts::services::server::provider_tokens::OrmSocialAccountStorage;
use crate::apps::accounts::services::server::redis_handle::RedisHandle;
use crate::apps::accounts::services::server::sessions::SessionService;
use crate::apps::accounts::services::server::sign_in::SignInService;
use crate::apps::accounts::services::server::sign_in_notices::NoticeStore;
use crate::config::settings::ProjectSettings;

/// Why the services could not be assembled.
#[derive(Debug, thiserror::Error)]
pub enum ServicesError {
	/// The Redis URL is not valid.
	#[error("the Redis URL is not valid")]
	Redis,
	/// The provider-token keyring is not valid.
	#[error("the provider-token key is not valid")]
	TokenKey,
	/// The sign-up policy is not valid.
	#[error("the sign-up policy is not valid")]
	Policy,
	/// The GitHub App configuration could not be used.
	#[error("the GitHub App configuration is not usable")]
	Github,
}

/// Everything the sign-in handlers and server functions need.
#[derive(Clone, Debug)]
pub struct AccountsServices {
	/// The GitHub callback logic; `None` when no GitHub App is configured.
	pub sign_in: Option<SignInService>,
	/// Browser sessions.
	pub sessions: SessionService,
	/// One-shot sign-in notices.
	pub notices: NoticeStore,
	/// Provider tokens, refreshed on use; `None` when no GitHub App is configured.
	pub provider_tokens: Option<ProviderTokenService>,
	/// Cookie attributes of the profile.
	pub cookies: CookieSecurity,
}

impl AccountsServices {
	/// Assemble the services from the validated settings.
	///
	/// Nothing here reaches the network or Redis, so building the router never
	/// needs either to be up.
	///
	/// # Errors
	///
	/// Returns an error when a setting that passed startup validation still
	/// cannot be turned into a service.
	pub fn build(settings: &ProjectSettings) -> Result<Self, ServicesError> {
		let accounts = &settings.accounts;
		let redis = RedisHandle::new(&settings.redis.url).map_err(|_| ServicesError::Redis)?;
		let sessions = SessionService::new(redis.clone());
		let notices = NoticeStore::new(redis);
		let cookies = CookieSecurity {
			secure: settings.core.security.session_cookie_secure,
		};

		let (sign_in, provider_tokens) = match accounts.github_app() {
			Some(app) => {
				let keyring = accounts
					.token_keyring(&settings.core.secret_key)
					.map_err(|_| ServicesError::TokenKey)?;
				let storage = Arc::new(OrmSocialAccountStorage::new(Arc::new(keyring)));
				let policy = accounts
					.sign_up_policy()
					.map_err(|_| ServicesError::Policy)?;
				let github = GithubSignIn::new(&app, &settings.redis.url)
					.map_err(|_| ServicesError::Github)?;
				let sign_in = SignInService::new(github, sessions.clone(), storage.clone(), policy);
				let tokens = ProviderTokenService::new(storage, ProviderTokenRefresher::new(&app));
				(Some(sign_in), Some(tokens))
			}
			None => (None, None),
		};

		Ok(Self {
			sign_in,
			sessions,
			notices,
			provider_tokens,
			cookies,
		})
	}
}

#[async_trait]
impl Injectable for AccountsServices {
	async fn inject(ctx: &InjectionContext) -> DiResult<Self> {
		ctx.get_singleton::<AccountsServices>()
			.map(|services| (*services).clone())
			.ok_or_else(|| DiError::NotRegistered {
				type_name: "AccountsServices".into(),
				hint: "The project router registers AccountsServices when it is assembled.".into(),
			})
	}
}
