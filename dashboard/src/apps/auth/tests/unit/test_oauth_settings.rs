//! Tests for `OAuthSettings::from_env`.
//!
//! Verifies that a provider is enabled iff all three of its credential env
//! vars (`CLIENT_ID`, `CLIENT_SECRET`, `REDIRECT_URI`) are present and
//! non-empty, and the token encryption key is valid. Partial configuration
//! disables the provider rather than half-enabling it, so the login UI never
//! offers a button that cannot complete the flow.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use serial_test::serial;

	use crate::apps::auth::services::oauth::config::OAuthSettings;

	const KEY_ID: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_CLIENT_ID";
	const KEY_SECRET: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_CLIENT_SECRET";
	const KEY_REDIRECT: &str = "REINHARDT_CLOUD_OAUTH_GITHUB_REDIRECT_URI";
	const KEY_TOKEN_ENCRYPTION: &str = "REINHARDT_CLOUD_OAUTH_TOKEN_ENCRYPTION_KEY";
	const VALID_TOKEN_ENCRYPTION_KEY: &str = "AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=";

	/// RAII guard that restores OAuth env vars on drop. Mirrors the pattern
	/// used by the e2e mailer tests so behavior under `serial` is identical.
	struct EnvGuard {
		saved: Vec<(String, Option<String>)>,
	}

	impl EnvGuard {
		fn set(vars: Vec<(&str, Option<&str>)>) -> Self {
			let mut saved = Vec::new();
			for (key, new_val) in &vars {
				saved.push((key.to_string(), std::env::var(key).ok()));
				// SAFETY: called in a serial test before any parallel tasks read these vars.
				unsafe {
					match new_val {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
			Self { saved }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (key, old_val) in &self.saved {
				// SAFETY: restoring env vars in serial test teardown.
				unsafe {
					match old_val {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
		}
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_github_enabled_when_all_three_env_vars_set() {
		// Arrange
		let _g = EnvGuard::set(vec![
			(KEY_ID, Some("github-client-id")),
			(KEY_SECRET, Some("github-client-secret")),
			(KEY_REDIRECT, Some("https://example.test/cb")),
			(KEY_TOKEN_ENCRYPTION, Some(VALID_TOKEN_ENCRYPTION_KEY)),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		let creds = settings.github.as_ref().expect("github should be enabled");
		assert_eq!(creds.client_id, "github-client-id");
		assert_eq!(creds.client_secret, "github-client-secret");
		assert_eq!(creds.redirect_uri, "https://example.test/cb");
		assert_eq!(settings.enabled_provider_ids(), vec!["github"]);
		assert!(settings.get("github").is_some());
		assert!(settings.get("gitlab").is_none());
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_github_disabled_when_secret_missing() {
		// Arrange
		let _g = EnvGuard::set(vec![
			(KEY_ID, Some("id-only")),
			(KEY_SECRET, None),
			(KEY_REDIRECT, Some("https://example.test/cb")),
			(KEY_TOKEN_ENCRYPTION, Some(VALID_TOKEN_ENCRYPTION_KEY)),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		assert!(settings.github.is_none());
		assert!(settings.enabled_provider_ids().is_empty());
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_github_disabled_when_client_id_missing() {
		// Arrange
		let _g = EnvGuard::set(vec![
			(KEY_ID, None),
			(KEY_SECRET, Some("secret-only")),
			(KEY_REDIRECT, Some("https://example.test/cb")),
			(KEY_TOKEN_ENCRYPTION, Some(VALID_TOKEN_ENCRYPTION_KEY)),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		assert!(settings.github.is_none());
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_github_disabled_when_all_missing() {
		// Arrange
		let _g = EnvGuard::set(vec![
			(KEY_ID, None),
			(KEY_SECRET, None),
			(KEY_REDIRECT, None),
			(KEY_TOKEN_ENCRYPTION, None),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		assert!(settings.github.is_none());
		assert!(settings.enabled_provider_ids().is_empty());
		assert!(settings.get("github").is_none());
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_empty_string_treated_as_unset() {
		// Arrange — partial configuration with empty values must disable the
		// provider just like absence does, otherwise an exported-but-empty
		// CLIENT_SECRET would silently fail at OAuth-exchange time.
		let _g = EnvGuard::set(vec![
			(KEY_ID, Some("")),
			(KEY_SECRET, Some("secret")),
			(KEY_REDIRECT, Some("https://example.test/cb")),
			(KEY_TOKEN_ENCRYPTION, Some(VALID_TOKEN_ENCRYPTION_KEY)),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		assert!(settings.github.is_none());
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_github_disabled_when_token_encryption_key_missing() {
		// Arrange
		let _g = EnvGuard::set(vec![
			(KEY_ID, Some("github-client-id")),
			(KEY_SECRET, Some("github-client-secret")),
			(KEY_REDIRECT, Some("https://example.test/cb")),
			(KEY_TOKEN_ENCRYPTION, None),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		assert!(settings.github.is_none());
		assert!(settings.enabled_provider_ids().is_empty());
	}

	#[rstest]
	#[serial(env_oauth)]
	fn test_github_disabled_when_token_encryption_key_invalid() {
		// Arrange
		let _g = EnvGuard::set(vec![
			(KEY_ID, Some("github-client-id")),
			(KEY_SECRET, Some("github-client-secret")),
			(KEY_REDIRECT, Some("https://example.test/cb")),
			(KEY_TOKEN_ENCRYPTION, Some("not-base64-32-bytes")),
		]);

		// Act
		let settings = OAuthSettings::from_env();

		// Assert
		assert!(settings.github.is_none());
		assert!(settings.enabled_provider_ids().is_empty());
	}
}
