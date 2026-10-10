//! Talking to GitHub for sign-in.
//!
//! Sign-in uses the GitHub App's user authorization (authorization code with
//! PKCE). The upstream `SocialAuthBackend` runs the flow; this module supplies
//! what it does not:
//!
//! - the provider configuration for a GitHub App (no scopes: a GitHub App's
//!   permissions come from the App, and a `scope` parameter is ignored);
//! - a shared, atomic, browser-bound state store (`AsyncSessionStateStore` over
//!   the Redis session backend: the state is read and deleted in one `GETDEL`,
//!   and carries the provider and the digest of the browser binding);
//! - the signing-in account's profile, read from GitHub's `/user` response;
//! - the organizations the account belongs to, for the `allowlist` policy.
//!
//! GitHub's responses and error bodies never leave this module: callers get a
//! small enum, so provider text cannot reach a client or an audit field
//! (SR-14).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::Utc;
use rand::Rng;
use reinhardt::RedisSessionBackend;
use reinhardt::auth::StateStore;
use reinhardt::auth::social::backend::SocialAuthBackend;
use reinhardt::auth::social::core::{OAuth2Config, ProviderConfig, SocialAuthError};
use reinhardt::auth::social::flow::pkce::PkceFlow;
use reinhardt::auth::social::providers::github::GitHubProvider;
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::middleware::session::AsyncSessionStateStore;
use serde::Deserialize;
use tokio::sync::OnceCell;

use crate::apps::accounts::server::settings::GithubAppConfig;
use crate::apps::accounts::services::server::provider_tokens::{GITHUB_PROVIDER, ProviderTokens};
use crate::apps::accounts::services::server::sign_up_policy::{
	MembershipError, OrganizationMembership,
};
use crate::apps::accounts::services::server::users::GithubProfile;

/// Request timeout for every call to GitHub.
const GITHUB_TIMEOUT: Duration = Duration::from_secs(10);

/// Lifetime GitHub documents for the refresh token of an expiring GitHub App
/// user token (`refresh_token_expires_in`, 15,897,600 seconds: 184 days).
///
/// Workaround for kent8192/reinhardt-web#6709 (tracked in
/// kent8192/reinhardt-cloud#936): upstream's `TokenResponse` drops
/// `refresh_token_expires_in`, so the code exchange cannot report the real
/// value. Remove this constant when the upstream issue is resolved.
///
/// Ideal implementation (without workaround):
///   `let refresh_expires_in = result.callback.token_response.refresh_token_expires_in;`
///   // `TokenResponse` carries the field GitHub sent.
pub(crate) const GITHUB_REFRESH_TOKEN_LIFETIME_SECONDS: u64 = 15_897_600;

/// Redis key prefix of the pending sign-in states.
pub(crate) const STATE_KEY_PREFIX: &str = "cloud:oauth-state:";

/// Binding used when the browser presented no binding cookie.
///
/// Workaround for kent8192/reinhardt-web#6723 (tracked in
/// kent8192/reinhardt-cloud#953): `handle_callback_with_context` returns
/// *before* consuming the state when the binding is empty, which would let a
/// cookie-less callback leave the single-use state alive (SR-04). A fixed value
/// that no issued binding can equal makes the state be consumed and the digest
/// comparison fail instead. Remove this when the upstream issue is resolved.
///
/// Ideal implementation (without workaround):
///   `backend.handle_callback_with_context(provider, code, state, binding)`
///   // with an empty `binding`: consumes the state, then fails with
///   // `InvalidState`, exactly like a non-matching binding.
const MISSING_BINDING: &[u8] = b"\0no-binding-cookie";

/// Why a sign-in could not be completed. Carries no provider text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompleteError {
	/// The state was unknown, expired, already used, issued for another
	/// provider, or presented by a different browser.
	#[error("sign-in state is not valid")]
	InvalidState,
	/// GitHub refused the code exchange or could not be reached.
	#[error("GitHub did not complete the code exchange")]
	ExchangeFailed,
	/// GitHub did not return a usable profile for the account.
	#[error("GitHub did not return a usable profile")]
	ProfileUnavailable,
	/// The state store or the configuration failed.
	#[error("sign-in could not be processed")]
	Internal,
}

impl CompleteError {
	/// Short code recorded in the audit log.
	#[must_use]
	pub const fn code(self) -> &'static str {
		match self {
			Self::InvalidState => "invalid_state",
			Self::ExchangeFailed => "exchange_failed",
			Self::ProfileUnavailable => "profile_unavailable",
			Self::Internal => "internal",
		}
	}
}

/// What starting a sign-in produced.
#[derive(Debug)]
pub struct StartedSignIn {
	/// Where to send the browser.
	pub authorization_url: String,
	/// The value for the binding cookie. Not the state: it never appears in
	/// a URL, so only the browser that started the flow can present it.
	pub binding: String,
}

/// The account GitHub authenticated, with the tokens it issued.
pub struct GithubIdentity {
	/// Numeric ID, login, name, and avatar.
	pub profile: GithubProfile,
	/// The issued user tokens.
	pub tokens: ProviderTokens,
}

/// Runs the GitHub sign-in flow.
///
/// Building it touches neither the network nor Redis: the upstream backend is
/// assembled on first use, because creating the GitHub provider is `async` and
/// the router (which owns this value) is built by a synchronous function.
#[derive(Clone)]
pub struct GithubSignIn {
	config: GithubAppConfig,
	backend: Arc<OnceCell<Arc<SocialAuthBackend>>>,
	states: Arc<AsyncSessionStateStore<RedisSessionBackend>>,
	http: reqwest::Client,
	api_url: String,
}

impl std::fmt::Debug for GithubSignIn {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("GithubSignIn")
			.field("api_url", &self.api_url)
			.finish_non_exhaustive()
	}
}

/// The HTTP client every GitHub call uses: bounded, and never following a
/// redirect (a redirect could carry the bearer token to another host).
pub(crate) fn github_http_client() -> reqwest::Client {
	reqwest::Client::builder()
		.timeout(GITHUB_TIMEOUT)
		.redirect(reqwest::redirect::Policy::none())
		.user_agent("reinhardt-cloud-control-plane")
		.build()
		.expect("a client with only a timeout, a redirect policy, and a user agent is valid")
}

impl GithubSignIn {
	/// Build the flow for `config`, with state kept in the Redis at `redis_url`.
	///
	/// # Errors
	///
	/// Returns [`CompleteError::Internal`] when the Redis URL is unusable.
	pub fn new(config: &GithubAppConfig, redis_url: &SecretString) -> Result<Self, CompleteError> {
		let sessions = RedisSessionBackend::new_from_url(redis_url.expose_secret())
			.map_err(|_| CompleteError::Internal)?
			.with_key_prefix(STATE_KEY_PREFIX.to_owned());
		Ok(Self {
			config: config.clone(),
			backend: Arc::new(OnceCell::new()),
			states: Arc::new(AsyncSessionStateStore::new(sessions)),
			http: github_http_client(),
			api_url: config.api_url.clone(),
		})
	}

	/// The upstream backend, assembled on first use.
	async fn backend(&self) -> Result<&Arc<SocialAuthBackend>, CompleteError> {
		self.backend
			.get_or_try_init(|| async {
				let provider = GitHubProvider::new(provider_config(&self.config))
					.await
					.map_err(|_| CompleteError::Internal)?;
				let mut backend = SocialAuthBackend::with_state_store(self.states.clone());
				backend.register_provider(Arc::new(provider));
				Ok(Arc::new(backend))
			})
			.await
	}

	/// Start a sign-in: create single-use, PKCE-protected state bound to a fresh
	/// browser binding.
	///
	/// # Errors
	///
	/// Returns [`CompleteError::Internal`] when the state cannot be stored.
	pub async fn begin(&self) -> Result<StartedSignIn, CompleteError> {
		let mut bytes = [0_u8; 32];
		rand::rng().fill(&mut bytes);
		let binding = URL_SAFE_NO_PAD.encode(bytes);
		let (verifier, challenge) = PkceFlow::generate();
		let started = self
			.backend()
			.await?
			.begin_auth_with_context(
				GITHUB_PROVIDER,
				Some(challenge.as_str()),
				Some(verifier.as_str().to_owned()),
				binding.as_bytes(),
				Vec::new(),
			)
			.await
			.map_err(|_| CompleteError::Internal)?;
		Ok(StartedSignIn {
			authorization_url: started.authorization_url,
			binding,
		})
	}

	/// Consume `state` without exchanging anything.
	///
	/// A callback that carries no code (the visitor declined, or GitHub
	/// reported an error) must still use up its state, so it cannot be replayed.
	pub async fn discard_state(&self, state: &str) {
		let _ = self.states.consume_contextual(state).await;
	}

	/// Finish a sign-in: consume the state, verify the browser binding, exchange
	/// the code, and read the account's profile.
	///
	/// `binding` is the value of the binding cookie, or `None` when the browser
	/// sent none; the state is consumed either way.
	///
	/// # Errors
	///
	/// See [`CompleteError`].
	pub async fn complete(
		&self,
		code: &str,
		state: &str,
		binding: Option<&str>,
	) -> Result<GithubIdentity, CompleteError> {
		let binding = binding
			.filter(|value| !value.is_empty())
			.map_or(MISSING_BINDING, str::as_bytes);
		let result = self
			.backend()
			.await?
			.handle_callback_with_context(GITHUB_PROVIDER, code, state, binding)
			.await
			.map_err(map_social_error)?
			.callback;

		// A failed `/user` call inside the backend is logged and swallowed,
		// leaving `claims` empty; that is a failed sign-in, not an anonymous one
		// (reinhardt-web#6710, tracked in kent8192/reinhardt-cloud#937).
		let claims = result.claims.ok_or(CompleteError::ProfileUnavailable)?;
		let access_token = SecretString::new(result.token_response.access_token.clone());
		let user = self.fetch_user(&access_token).await?;
		if claims.sub != user.id.to_string() {
			return Err(CompleteError::ProfileUnavailable);
		}

		let now = Utc::now();
		let refresh_token = result
			.token_response
			.refresh_token
			.clone()
			.map(SecretString::new);
		// A missing `expires_in` means GitHub issued a token that does not
		// expire (the App opted out of user-token expiration); such a token
		// comes without a refresh token and is stored with no expiry.
		let access_token_expires_at = result
			.token_response
			.expires_in
			.map(|seconds| after(now, seconds).ok_or(CompleteError::ExchangeFailed))
			.transpose()?;
		let refresh_token_expires_at = match refresh_token {
			Some(_) => Some(
				after(now, GITHUB_REFRESH_TOKEN_LIFETIME_SECONDS).ok_or(CompleteError::Internal)?,
			),
			None => None,
		};
		let tokens = ProviderTokens {
			access_token,
			access_token_expires_at,
			refresh_token_expires_at,
			refresh_token,
		};
		Ok(GithubIdentity {
			profile: GithubProfile {
				github_user_id: user.id,
				login: user.login,
				name: user.name,
				avatar_url: user.avatar_url,
				// The public-profile email is unverified, and the verified one
				// needs a permission this App does not request, so none is kept.
				verified_email: None,
			},
			tokens,
		})
	}

	/// Read the account behind `access_token` from `GET /user`.
	///
	/// Workaround for kent8192/reinhardt-web#6710 (tracked in
	/// kent8192/reinhardt-cloud#937): `GitHubProvider::get_user_info` keeps the
	/// numeric `id` but drops `login` (it is only a fallback for `name`), so the
	/// login is read from the raw response here. Remove this call when the
	/// upstream issue is resolved.
	///
	/// Ideal implementation (without workaround):
	///   `let login = claims.additional_claims["login"];`
	///   // `StandardClaims` carries the GitHub login; no second `/user` call.
	async fn fetch_user(&self, access_token: &SecretString) -> Result<GithubUser, CompleteError> {
		let response = self
			.http
			.get(format!("{}/user", self.api_url))
			.bearer_auth(access_token.expose_secret())
			.header("Accept", "application/vnd.github+json")
			.send()
			.await
			.map_err(|_| CompleteError::ProfileUnavailable)?;
		if !response.status().is_success() {
			return Err(CompleteError::ProfileUnavailable);
		}
		let user: GithubUser = response
			.json()
			.await
			.map_err(|_| CompleteError::ProfileUnavailable)?;
		if user.id <= 0 || user.login.is_empty() {
			return Err(CompleteError::ProfileUnavailable);
		}
		Ok(user)
	}

	/// The organization lookup for an account that just signed in, using the
	/// token GitHub issued for that sign-in (SR-19: asked of GitHub on the
	/// server, never taken from the browser).
	#[must_use]
	pub fn memberships(&self, access_token: SecretString) -> GithubOrganizationMembership {
		GithubOrganizationMembership {
			http: self.http.clone(),
			api_url: self.api_url.clone(),
			access_token,
		}
	}
}

#[derive(Deserialize)]
struct GithubUser {
	id: i64,
	login: String,
	#[serde(default)]
	name: Option<String>,
	#[serde(default)]
	avatar_url: Option<String>,
}

/// `now` plus `seconds`, or `None` when the sum is not a representable time.
/// A lifetime comes from GitHub, so it is never trusted to be in range.
pub(crate) fn after(now: chrono::DateTime<Utc>, seconds: u64) -> Option<chrono::DateTime<Utc>> {
	let seconds = i64::try_from(seconds).ok()?;
	now.checked_add_signed(chrono::Duration::try_seconds(seconds)?)
}

fn map_social_error(error: SocialAuthError) -> CompleteError {
	match error {
		SocialAuthError::InvalidState
		| SocialAuthError::StateValidation(_)
		| SocialAuthError::PkceValidation(_) => CompleteError::InvalidState,
		SocialAuthError::Network(_)
		| SocialAuthError::InvalidResponse(_)
		| SocialAuthError::TokenExchangeError(_)
		| SocialAuthError::UserInfoError(_) => CompleteError::ExchangeFailed,
		_ => CompleteError::Internal,
	}
}

/// The GitHub App's provider configuration: GitHub's OAuth endpoints and no
/// scopes.
fn provider_config(config: &GithubAppConfig) -> ProviderConfig {
	ProviderConfig {
		name: GITHUB_PROVIDER.to_owned(),
		client_id: config.client_id.clone(),
		client_secret: config.client_secret.expose_secret().to_owned(),
		redirect_uri: config.redirect_uri.clone(),
		// A GitHub App's permissions are set on the App; `ProviderConfig::github`
		// would request the classic `user` and `user:email` scopes.
		scopes: Vec::new(),
		oidc: None,
		oauth2: Some(OAuth2Config {
			authorization_endpoint: config.authorize_url.clone(),
			token_endpoint: config.token_url.clone(),
			userinfo_endpoint: Some(format!("{}/user", config.api_url)),
		}),
	}
}

/// Pages of organizations read before giving up (100 per page).
const MAX_ORGANIZATION_PAGES: u32 = 5;

/// `OrganizationMembership` backed by GitHub, using the signing-in account's
/// own token.
///
/// `GET /user/memberships/orgs?state=active` lists the account's active
/// organization memberships for a GitHub App user access token (the
/// `GET /user/orgs` listing comes back empty for such tokens). It needs the
/// App's "Members" organization permission, so an allowlisted organization has
/// to have installed the App with it. Only memberships GitHub reports as
/// `active` count; a pending invitation is not membership. Anything GitHub
/// cannot confirm denies the sign-up.
pub struct GithubOrganizationMembership {
	http: reqwest::Client,
	api_url: String,
	access_token: SecretString,
}

/// One entry of `GET /user/memberships/orgs`.
#[derive(Deserialize)]
struct MembershipEntry {
	state: String,
	organization: OrganizationEntry,
}

#[derive(Deserialize)]
struct OrganizationEntry {
	id: i64,
}

#[async_trait]
impl OrganizationMembership for GithubOrganizationMembership {
	async fn organization_ids(&self, _github_user_id: i64) -> Result<Vec<i64>, MembershipError> {
		let mut ids = Vec::new();
		for page in 1..=MAX_ORGANIZATION_PAGES {
			let response = self
				.http
				.get(format!("{}/user/memberships/orgs", self.api_url))
				.query(&[
					("state", "active"),
					("per_page", "100"),
					("page", &page.to_string()),
				])
				.bearer_auth(self.access_token.expose_secret())
				.header("Accept", "application/vnd.github+json")
				.send()
				.await
				.map_err(|_| MembershipError)?;
			if !response.status().is_success() {
				return Err(MembershipError);
			}
			let entries: Vec<MembershipEntry> =
				response.json().await.map_err(|_| MembershipError)?;
			let full_page = entries.len() >= 100;
			ids.extend(
				entries
					.into_iter()
					.filter(|entry| entry.state == "active")
					.map(|entry| entry.organization.id),
			);
			if !full_page {
				return Ok(ids);
			}
		}
		// More organizations than were read: the answer would be incomplete.
		Err(MembershipError)
	}
}
