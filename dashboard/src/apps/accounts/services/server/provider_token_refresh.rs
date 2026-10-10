//! Using a User's GitHub token, refreshing it when it is about to expire.
//!
//! A GitHub App user token lasts hours; its refresh token lasts months and is
//! single-use: every refresh returns a new pair and invalidates the old one.
//! [`ProviderTokenService::access_token`] therefore persists the new pair in
//! one write ([`OrmSocialAccountStorage::store_tokens`], never the trait
//! `update`, which cannot carry the refresh-token expiry) before handing out
//! the new access token.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use reinhardt::conf::settings::secret_types::SecretString;
use serde::Deserialize;
use uuid::Uuid;

use crate::apps::accounts::server::settings::GithubAppConfig;
use crate::apps::accounts::services::server::github::{after, github_http_client};
use crate::apps::accounts::services::server::provider_tokens::{
	OrmSocialAccountStorage, ProviderTokens,
};

/// A token that expires within this window is refreshed before use.
const REFRESH_SKEW: Duration = Duration::seconds(60);

/// After GitHub rejects a refresh token, how often and how long to wait for a
/// concurrent request to store the replacement it obtained.
const RACE_ATTEMPTS: u32 = 4;
const RACE_WAIT: std::time::Duration = std::time::Duration::from_millis(150);

/// Why a token could not be provided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TokenAccessError {
	/// The User has no stored token (they have never signed in with GitHub).
	#[error("no GitHub token is stored for the user")]
	NoToken,
	/// The token cannot be renewed: GitHub rejected the refresh token or none
	/// is stored. The User has to sign in again.
	#[error("the GitHub token cannot be renewed; sign in again")]
	ReauthenticationRequired,
	/// Storage or GitHub failed; retrying later may work.
	#[error("the GitHub token is temporarily unavailable")]
	Unavailable,
}

/// How a refresh attempt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshError {
	/// GitHub answered that the refresh token is not valid.
	Rejected,
	/// GitHub could not be reached or answered in an unexpected shape.
	Unavailable,
}

/// Exchanges a refresh token for a new token pair.
#[derive(Clone)]
pub struct ProviderTokenRefresher {
	http: reqwest::Client,
	token_url: String,
	client_id: String,
	client_secret: SecretString,
}

#[derive(Deserialize)]
struct RefreshResponse {
	access_token: Option<String>,
	refresh_token: Option<String>,
	expires_in: Option<u64>,
	refresh_token_expires_in: Option<u64>,
	error: Option<String>,
}

impl ProviderTokenRefresher {
	/// Create a refresher for the GitHub App in `config`.
	#[must_use]
	pub fn new(config: &GithubAppConfig) -> Self {
		Self {
			http: github_http_client(),
			token_url: config.token_url.clone(),
			client_id: config.client_id.clone(),
			client_secret: config.client_secret.clone(),
		}
	}

	/// Exchange `refresh_token` for a new pair.
	///
	/// Workaround for kent8192/reinhardt-web#6709 (tracked in
	/// kent8192/reinhardt-cloud#936): upstream's `RefreshFlow` sends no
	/// `Accept: application/json`, so GitHub answers form-encoded and parsing
	/// fails; it also drops `refresh_token_expires_in` and mistakes GitHub's
	/// `200 {"error": ...}` for a token. Remove this request when the upstream
	/// issue is resolved.
	///
	/// Ideal implementation (without workaround):
	///   `let token = provider.refresh_token(refresh_token).await?;`
	///   // `RefreshFlow` asks for JSON, reports `refresh_token_expires_in`, and
	///   // turns an `error` body into a `SocialAuthError`.
	///
	/// # Errors
	///
	/// See [`RefreshError`].
	pub async fn refresh(
		&self,
		refresh_token: &SecretString,
		now: DateTime<Utc>,
	) -> Result<ProviderTokens, RefreshError> {
		let response = self
			.http
			.post(&self.token_url)
			.header("Accept", "application/json")
			.form(&[
				("grant_type", "refresh_token"),
				("refresh_token", refresh_token.expose_secret()),
				("client_id", self.client_id.as_str()),
				("client_secret", self.client_secret.expose_secret()),
			])
			.send()
			.await
			.map_err(|_| RefreshError::Unavailable)?;
		if !response.status().is_success() {
			return Err(RefreshError::Unavailable);
		}
		let body: RefreshResponse = response
			.json()
			.await
			.map_err(|_| RefreshError::Unavailable)?;
		if body.error.is_some() {
			return Err(RefreshError::Rejected);
		}
		let (Some(access_token), Some(expires_in)) = (body.access_token, body.expires_in) else {
			return Err(RefreshError::Unavailable);
		};
		let refresh_token = body.refresh_token.map(SecretString::new);
		// Lifetimes come from GitHub; one that is not a representable time
		// makes the whole answer unusable.
		let access_token_expires_at = after(now, expires_in).ok_or(RefreshError::Unavailable)?;
		let refresh_token_expires_at = match (&refresh_token, body.refresh_token_expires_in) {
			(Some(_), Some(seconds)) => Some(after(now, seconds).ok_or(RefreshError::Unavailable)?),
			_ => None,
		};
		Ok(ProviderTokens {
			access_token: SecretString::new(access_token),
			access_token_expires_at,
			refresh_token_expires_at,
			refresh_token,
		})
	}
}

/// Provides a usable GitHub access token for a User.
#[derive(Clone)]
pub struct ProviderTokenService {
	storage: Arc<OrmSocialAccountStorage>,
	refresher: ProviderTokenRefresher,
}

impl std::fmt::Debug for ProviderTokenService {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ProviderTokenService")
			.finish_non_exhaustive()
	}
}

impl ProviderTokenService {
	/// Create the service.
	#[must_use]
	pub fn new(storage: Arc<OrmSocialAccountStorage>, refresher: ProviderTokenRefresher) -> Self {
		Self { storage, refresher }
	}

	/// The access token of `user_id`, renewed first when it is about to expire.
	///
	/// # Errors
	///
	/// See [`TokenAccessError`].
	pub async fn access_token(&self, user_id: Uuid) -> Result<SecretString, TokenAccessError> {
		let now = Utc::now();
		let tokens = self.load(user_id).await?;
		if tokens.access_token_expires_at - now > REFRESH_SKEW {
			return Ok(tokens.access_token);
		}

		let usable_refresh = tokens.refresh_token.filter(|_| {
			tokens
				.refresh_token_expires_at
				.is_none_or(|expiry| expiry > now)
		});
		let Some(refresh_token) = usable_refresh else {
			return Err(TokenAccessError::ReauthenticationRequired);
		};
		match self.refresher.refresh(&refresh_token, now).await {
			Ok(renewed) => {
				self.storage
					.store_tokens(user_id, &renewed)
					.await
					.map_err(|_| TokenAccessError::Unavailable)?;
				Ok(renewed.access_token)
			}
			Err(RefreshError::Unavailable) => Err(TokenAccessError::Unavailable),
			Err(RefreshError::Rejected) => {
				// Refresh tokens are single-use: a concurrent request may have
				// spent this one and be about to store its replacement. Give it
				// a moment before telling the User to sign in again.
				for _ in 0..RACE_ATTEMPTS {
					tokio::time::sleep(RACE_WAIT).await;
					let current = self.load(user_id).await?;
					if current.access_token_expires_at - now > REFRESH_SKEW {
						return Ok(current.access_token);
					}
				}
				Err(TokenAccessError::ReauthenticationRequired)
			}
		}
	}

	async fn load(&self, user_id: Uuid) -> Result<ProviderTokens, TokenAccessError> {
		self.storage
			.load_tokens(user_id)
			.await
			.map_err(|_| TokenAccessError::Unavailable)?
			.ok_or(TokenAccessError::NoToken)
	}
}
