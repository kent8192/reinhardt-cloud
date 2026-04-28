//! Provider credentials sourced from environment variables.
//!
//! Per `CM-2`, OAuth client credentials are environment-driven so that
//! deployments never bake provider secrets into TOML or container images.
//! A provider is considered enabled iff its `CLIENT_ID` and `CLIENT_SECRET`
//! are both set; partial configuration disables the provider entirely so
//! that the login UI does not present a button that cannot complete the
//! flow.

use std::env;

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
	/// `REDIRECT_URI`) must be present and non-empty for the provider to be
	/// enabled. If any of them is missing, the provider entry is `None` and
	/// the corresponding REST endpoints will return 404 / the login button
	/// will not appear.
	pub fn from_env() -> Self {
		Self {
			github: read_provider("GITHUB"),
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
