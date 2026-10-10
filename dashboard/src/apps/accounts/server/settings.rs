//! The `[accounts]` settings fragment.
//!
//! Every value is a string so that deployed profiles can feed it from the
//! environment through `${VAR:-default}` interpolation (the settings loader
//! coerces interpolated text only into string fields without further help).
//! List-valued settings are comma-separated.

use reinhardt::conf::settings::fragment::SettingsValidation;
use reinhardt::conf::settings::profile::Profile;
use reinhardt::conf::settings::secret_types::SecretString;
use reinhardt::conf::settings::validation::{ValidationError, ValidationResult};
use reinhardt::settings;
use serde::{Deserialize, Serialize};

use crate::apps::accounts::services::server::sign_up_policy::{SignUpPolicy, SignUpPolicyError};
use crate::apps::accounts::services::server::token_crypto::{
	TokenCryptoError, TokenKeyring, TokenKeyringSettings,
};

/// Settings of the accounts application.
///
/// | Key | Environment variable (committed profiles) |
/// |-----|-------------------------------------------|
/// | `sign_up_policy` | `REINHARDT_CLOUD_SIGN_UP_POLICY` |
/// | `sign_up_allowed_github_user_ids` | `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_USER_IDS` |
/// | `sign_up_allowed_github_organization_ids` | `REINHARDT_CLOUD_SIGN_UP_ALLOWED_GITHUB_ORGANIZATION_IDS` |
/// | `token_encryption_key` | `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY` |
/// | `token_encryption_key_id` | `REINHARDT_CLOUD_TOKEN_ENCRYPTION_KEY_ID` |
/// | `token_encryption_retired_keys` | `REINHARDT_CLOUD_TOKEN_ENCRYPTION_RETIRED_KEYS` |
/// | `public_url` | `REINHARDT_CLOUD_PUBLIC_URL` |
/// | `allowed_origins` | `REINHARDT_CLOUD_ALLOWED_ORIGINS` |
/// | `trusted_proxies` | `REINHARDT_CLOUD_TRUSTED_PROXIES` |
/// | `github_sign_in` | `REINHARDT_CLOUD_GITHUB_SIGN_IN` |
/// | `github_client_id` | `REINHARDT_CLOUD_GITHUB_CLIENT_ID` |
/// | `github_client_secret` | `REINHARDT_CLOUD_GITHUB_CLIENT_SECRET` |
/// | `github_authorize_url`, `github_token_url`, `github_api_url` | `REINHARDT_CLOUD_GITHUB_{AUTHORIZE,TOKEN,API}_URL` |
#[settings(fragment = true, section = "accounts", validate = false)]
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct AccountsSettings {
	/// Sign-up policy name: `open`, `allowlist`, or `invite_only`. Anything
	/// else, including an empty value, resolves to `invite_only` (SR-19).
	#[serde(default)]
	pub sign_up_policy: String,

	/// Comma-separated numeric GitHub user IDs admitted under `allowlist`.
	#[serde(default)]
	pub sign_up_allowed_github_user_ids: String,

	/// Comma-separated numeric GitHub organization IDs whose members are
	/// admitted under `allowlist`. IDs rather than logins, because a renamed
	/// organization's old login can be claimed by someone else (SR-19).
	#[serde(default)]
	pub sign_up_allowed_github_organization_ids: String,

	/// Base64 of a dedicated 32-byte provider-token encryption key. When empty,
	/// the key is derived from `core.secret_key` with a domain-separated KDF.
	#[serde(default)]
	pub token_encryption_key: Option<SecretString>,

	/// Identifier stored beside every ciphertext produced with the key above.
	#[serde(default)]
	pub token_encryption_key_id: String,

	/// Comma-separated `id:base64` pairs of retired keys, kept to decrypt
	/// tokens written before a rotation.
	#[serde(default)]
	pub token_encryption_retired_keys: Option<SecretString>,

	/// Origin the Dashboard is served from (`scheme://host[:port]`, no path).
	/// The GitHub callback URL and the first cross-site request origin
	/// (SR-12) derive from it.
	#[serde(default)]
	pub public_url: String,

	/// Comma-separated extra origins allowed to send state-changing
	/// cookie-authenticated requests (SR-12). A wildcard entry is ignored.
	#[serde(default)]
	pub allowed_origins: String,

	/// Comma-separated IP addresses of the TLS-terminating proxies in front of
	/// the Control Plane (the framework matches exact addresses, not ranges).
	/// Only requests from these addresses have `X-Forwarded-Proto` honored, which
	/// is what lets `Strict-Transport-Security` be sent behind a proxy (SR-13).
	#[serde(default)]
	pub trusted_proxies: String,

	/// Set to `disabled` to run without GitHub sign-in (only Login Links can then
	/// sign in). `staging` and `production` refuse to start without a GitHub App
	/// unless this explicit opt-out is set.
	#[serde(default)]
	pub github_sign_in: String,

	/// Client ID of the GitHub App that backs sign-in.
	#[serde(default)]
	pub github_client_id: String,

	/// Client secret of the GitHub App. Never logged.
	#[serde(default)]
	pub github_client_secret: Option<SecretString>,

	/// Overrides the GitHub authorization endpoint. Empty means GitHub.
	/// Exists so integration tests can point the flow at a local mock server.
	#[serde(default)]
	pub github_authorize_url: String,

	/// Overrides the GitHub token endpoint. Empty means GitHub.
	#[serde(default)]
	pub github_token_url: String,

	/// Overrides the GitHub REST API base URL. Empty means GitHub.
	#[serde(default)]
	pub github_api_url: String,
}

impl AccountsSettings {
	/// Build the sign-up policy these settings describe.
	///
	/// # Errors
	///
	/// Returns an error when an allowlist entry is not a numeric ID.
	pub fn sign_up_policy(&self) -> Result<SignUpPolicy, SignUpPolicyError> {
		SignUpPolicy::from_settings(
			&self.sign_up_policy,
			&self.sign_up_allowed_github_user_ids,
			&self.sign_up_allowed_github_organization_ids,
		)
	}

	/// Build the provider-token keyring.
	///
	/// `secret_key` is `core.secret_key`, the fallback key material.
	///
	/// # Errors
	///
	/// Returns an error when a configured key is malformed (SR-06).
	pub fn token_keyring(&self, secret_key: &str) -> Result<TokenKeyring, TokenCryptoError> {
		TokenKeyring::from_settings(
			TokenKeyringSettings {
				key: self.token_encryption_key.as_ref(),
				key_id: &self.token_encryption_key_id,
				retired_keys: self.token_encryption_retired_keys.as_ref(),
			},
			secret_key,
		)
	}
}

impl AccountsSettings {
	/// Whether GitHub sign-in was explicitly switched off.
	#[must_use]
	pub fn github_sign_in_disabled(&self) -> bool {
		self.github_sign_in.trim().eq_ignore_ascii_case("disabled")
	}

	/// The public URL as a bare origin (`scheme://host[:port]`, no path), or
	/// `None` when it is empty or has anything else in it.
	#[must_use]
	pub fn public_origin(&self) -> Option<String> {
		let candidate = self.public_url.trim().trim_end_matches('/');
		normalize_origin(candidate).filter(|origin| origin == candidate)
	}

	/// The trusted proxy addresses.
	///
	/// # Errors
	///
	/// Returns the first entry that is not an IP address.
	pub fn trusted_proxy_addresses(&self) -> Result<Vec<std::net::IpAddr>, String> {
		self.trusted_proxies
			.split(',')
			.map(str::trim)
			.filter(|entry| !entry.is_empty())
			.map(|entry| entry.parse().map_err(|_| entry.to_owned()))
			.collect()
	}

	/// The GitHub App configuration, or `None` when sign-in is not configured
	/// (no client ID or no client secret).
	#[must_use]
	pub fn github_app(&self) -> Option<GithubAppConfig> {
		if self.github_sign_in_disabled() {
			return None;
		}
		let client_secret = self.github_client_secret.as_ref()?;
		let client_id = self.github_client_id.trim();
		if client_id.is_empty() || client_secret.is_empty() {
			return None;
		}
		let public_url = self.public_url.trim().trim_end_matches('/');
		Some(GithubAppConfig {
			client_id: client_id.to_owned(),
			client_secret: client_secret.clone(),
			redirect_uri: format!("{public_url}{GITHUB_CALLBACK_PATH}"),
			authorize_url: non_empty_or(&self.github_authorize_url, GITHUB_AUTHORIZE_URL),
			token_url: non_empty_or(&self.github_token_url, GITHUB_TOKEN_URL),
			api_url: non_empty_or(&self.github_api_url, GITHUB_API_URL)
				.trim_end_matches('/')
				.to_owned(),
		})
	}

	/// Origins allowed to send state-changing cookie-authenticated requests
	/// (SR-12): the public URL's origin plus the configured list. A wildcard or
	/// a malformed entry is ignored. In a debug profile the loopback origins of
	/// `port` are added; no deployed profile may enable them.
	#[must_use]
	pub fn request_origins(&self, debug: bool, port: u16) -> Vec<String> {
		let mut origins: Vec<String> = Vec::new();
		let configured = self.allowed_origins.split(',');
		for candidate in std::iter::once(self.public_url.as_str()).chain(configured) {
			if let Some(origin) = normalize_origin(candidate)
				&& !origins.contains(&origin)
			{
				origins.push(origin);
			}
		}
		if debug {
			for loopback in [
				format!("http://localhost:{port}"),
				format!("http://127.0.0.1:{port}"),
			] {
				if !origins.contains(&loopback) {
					origins.push(loopback);
				}
			}
		}
		origins
	}
}

/// Path of the GitHub callback served by this application.
pub const GITHUB_CALLBACK_PATH: &str = "/api/auth/github/callback/";

const GITHUB_AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GITHUB_API_URL: &str = "https://api.github.com";

fn non_empty_or(value: &str, default: &str) -> String {
	let value = value.trim();
	if value.is_empty() { default } else { value }.to_owned()
}

/// Reduce `candidate` to `scheme://host[:port]`, or `None` when it is empty, a
/// wildcard, or not an http(s) origin.
fn normalize_origin(candidate: &str) -> Option<String> {
	let candidate = candidate.trim();
	if candidate.is_empty() || candidate.contains('*') {
		return None;
	}
	let (scheme, rest) = candidate.split_once("://")?;
	if !matches!(scheme, "http" | "https") {
		return None;
	}
	let authority = rest.split(['/', '?', '#']).next()?;
	if authority.is_empty() || authority.contains('@') {
		return None;
	}
	Some(format!("{scheme}://{authority}"))
}

/// The GitHub App used for sign-in, as the application consumes it.
#[derive(Clone, Debug)]
pub struct GithubAppConfig {
	/// App client ID.
	pub client_id: String,
	/// App client secret (redacts itself in `Debug`).
	pub client_secret: SecretString,
	/// Callback URL registered on the App.
	pub redirect_uri: String,
	/// Authorization endpoint.
	pub authorize_url: String,
	/// Token endpoint, used for code exchange and refresh.
	pub token_url: String,
	/// REST API base URL without a trailing slash.
	pub api_url: String,
}

impl SettingsValidation for AccountsSettings {
	fn validate(&self, profile: &Profile) -> ValidationResult {
		self.sign_up_policy()
			.map_err(|error| ValidationError::InvalidValue {
				key: "accounts.sign_up_allowed_*".to_owned(),
				message: error.to_string(),
			})?;
		self.trusted_proxy_addresses()
			.map_err(|entry| ValidationError::InvalidValue {
				key: "accounts.trusted_proxies".to_owned(),
				message: format!("{entry:?} is not an IP address"),
			})?;
		let setting = self.github_sign_in.trim();
		if !setting.is_empty() && !self.github_sign_in_disabled() {
			return Err(ValidationError::InvalidValue {
				key: "accounts.github_sign_in".to_owned(),
				message: "must be empty or `disabled`".to_owned(),
			});
		}
		// A half-configured App is a mistake, never a choice.
		let has_id = !self.github_client_id.trim().is_empty();
		let has_secret = self
			.github_client_secret
			.as_ref()
			.is_some_and(|secret| !secret.is_empty());
		if has_id != has_secret {
			return Err(ValidationError::InvalidValue {
				key: "accounts.github_client_*".to_owned(),
				message: "the GitHub App client ID and secret must be set together".to_owned(),
			});
		}
		let deployed = matches!(profile, Profile::Staging | Profile::Production);
		if self.github_sign_in_disabled() {
			return Ok(());
		}
		// Sign-in is the only way in (SR-01): a deployed profile without a GitHub
		// App must say so explicitly instead of starting with no way to sign in.
		if deployed && !has_id {
			return Err(ValidationError::InvalidValue {
				key: "accounts.github_client_*".to_owned(),
				message: "the GitHub App client ID and secret are required in a deployed profile; set `accounts.github_sign_in = \"disabled\"` to run without GitHub sign-in".to_owned(),
			});
		}
		if has_id {
			if self.public_origin().is_none() {
				return Err(ValidationError::InvalidValue {
					key: "accounts.public_url".to_owned(),
					message: "must be a non-empty origin without a path (for example `https://host`) when the GitHub App is configured".to_owned(),
				});
			}
			if deployed && !self.public_url.trim().starts_with("https://") {
				return Err(ValidationError::InvalidValue {
					key: "accounts.public_url".to_owned(),
					message: "must be an https origin in a deployed profile".to_owned(),
				});
			}
		}
		Ok(())
	}
}
