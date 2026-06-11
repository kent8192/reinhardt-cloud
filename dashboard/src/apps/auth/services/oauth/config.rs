//! Provider credentials sourced from environment variables.
//!
//! Per `CM-2`, OAuth client credentials are environment-driven so that
//! deployments never bake provider secrets into TOML or container images.
//! A provider is considered enabled iff its `CLIENT_ID` and `CLIENT_SECRET`
//! are both set and the dashboard can encrypt persisted OAuth tokens; partial
//! runtime configuration disables the provider entirely so that the login UI
//! does not present a button that cannot complete the flow.
//!
//! [`OAuthSettings`] is exposed via `#[injectable_factory]` so handlers
//! and the OAuth backend factory resolve a single shared snapshot of the
//! environment per process. The legacy [`OAuthSettings::from_env`]
//! constructor is retained as an adapter during the
//! kent8192/reinhardt-cloud#599 caller migration.

use std::env;

use reinhardt::di::injectable_factory;

use crate::apps::auth::services::oauth::token_crypto::token_encryption_key_is_configured;
/// Credentials for a single OAuth provider, populated from env vars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCredentials {
	pub client_id: String,
	pub client_secret: String,
	pub redirect_uri: String,
}

/// All OAuth provider credentials known to the dashboard.
///
/// Today only `github` is shipped (see #428). Additional providers (GitLab,
/// etc.) will land via separate feature flags / follow-up issues — see
/// `kent8192/reinhardt-cloud#440`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OAuthSettings {
	pub github: Option<ProviderCredentials>,
}

impl OAuthSettings {
	/// Reads provider credentials from the process environment.
	///
	/// For each provider, all three env vars (`CLIENT_ID`, `CLIENT_SECRET`,
	/// `REDIRECT_URI`) and a valid OAuth token encryption key must be present
	/// for the provider to be enabled. If any of them is missing, the provider
	/// entry is `None` and provider discovery omits that provider.
	pub fn from_env() -> Self {
		let can_store_tokens = token_encryption_key_is_configured();
		Self {
			github: can_store_tokens.then(|| read_provider("GITHUB")).flatten(),
		}
	}

	/// Lookup credentials by provider id (lowercase, e.g. `"github"`).
	pub fn get(&self, provider: &str) -> Option<&ProviderCredentials> {
		match provider {
			"github" => self.github.as_ref(),
			_ => None,
		}
	}

	/// List the ids of providers that are currently enabled.
	pub fn enabled_provider_ids(&self) -> Vec<&'static str> {
		let mut out = Vec::new();
		if self.github.is_some() {
			out.push("github");
		}
		out
	}
}

/// DI factory — resolves [`OAuthSettings`] from the process environment
/// once at first resolve. Singleton-scoped so the snapshot is shared
/// across all handlers and the OAuth backend factory.
///
/// Tests should construct [`OAuthSettings`] directly and override the
/// scope entry with `scope.set::<OAuthSettings>(...)` rather than going
/// through this factory.
#[injectable_factory(scope = "singleton")]
async fn create_oauth_settings() -> OAuthSettings {
	OAuthSettings::from_env()
}

fn read_provider(suffix: &str) -> Option<ProviderCredentials> {
	let client_id = non_empty_env(&format!("REINHARDT_CLOUD_OAUTH_{suffix}_CLIENT_ID"))?;
	let client_secret = non_empty_env(&format!("REINHARDT_CLOUD_OAUTH_{suffix}_CLIENT_SECRET"))?;
	let redirect_uri = non_empty_env(&format!("REINHARDT_CLOUD_OAUTH_{suffix}_REDIRECT_URI"))?;
	Some(ProviderCredentials {
		client_id,
		client_secret,
		redirect_uri,
	})
}

fn non_empty_env(key: &str) -> Option<String> {
	env::var(key).ok().filter(|v| !v.is_empty())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::test_helpers::make_test_di_context;
	use rstest::rstest;
	use std::sync::Arc;

	#[rstest]
	#[tokio::test]
	async fn test_oauth_settings_factory_resolves_with_overridden_value() {
		// Arrange — register a hand-built OAuthSettings so the factory
		// path does not touch process environment variables.
		let expected = OAuthSettings {
			github: Some(ProviderCredentials {
				client_id: "test-client-id".to_string(),
				client_secret: "test-client-secret".to_string(),
				redirect_uri: "https://example.test/oauth/github/callback".to_string(),
			}),
		};
		let ctx = make_test_di_context(|scope| {
			scope.set(expected.clone());
		});

		// Act
		let resolved: Arc<OAuthSettings> = ctx
			.resolve::<OAuthSettings>()
			.await
			.expect("OAuthSettings factory should resolve when value is registered");

		// Assert
		assert_eq!(*resolved, expected);
		assert_eq!(resolved.enabled_provider_ids(), vec!["github"]);
	}
}
