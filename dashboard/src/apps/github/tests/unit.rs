//! Unit tests for GitHub App integration.

#[cfg(test)]
pub mod config_tests {
	use rstest::rstest;
	use serial_test::serial;

	use crate::apps::github::services::config::GitHubAppSettings;

	const APP_ID_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_ID";
	const PRIVATE_KEY_PEM_ENV: &str = "REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM";
	const WEBHOOK_SECRET_ENV: &str = "REINHARDT_CLOUD_GITHUB_WEBHOOK_SECRET";
	const API_BASE_URL_ENV: &str = "REINHARDT_CLOUD_GITHUB_API_BASE_URL";

	struct EnvGuard {
		saved: Vec<(String, Option<String>)>,
	}

	impl EnvGuard {
		fn set(vars: Vec<(&str, Option<&str>)>) -> Self {
			let mut saved = Vec::new();
			for (key, value) in &vars {
				saved.push((key.to_string(), std::env::var(key).ok()));
				// SAFETY: these tests are serialized and mutate env vars before the act phase.
				unsafe {
					match value {
						Some(value) => std::env::set_var(key, value),
						None => std::env::remove_var(key),
					}
				}
			}
			Self { saved }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (key, value) in &self.saved {
				// SAFETY: these tests are serialized and restore env vars during teardown.
				unsafe {
					match value {
						Some(value) => std::env::set_var(key, value),
						None => std::env::remove_var(key),
					}
				}
			}
		}
	}

	#[rstest]
	#[serial(env_github_app_settings)]
	fn test_github_app_settings_loads_required_env() {
		// Arrange
		let _env = EnvGuard::set(vec![
			(APP_ID_ENV, Some("12345")),
			(
				PRIVATE_KEY_PEM_ENV,
				Some("-----BEGIN PRIVATE KEY-----\\nabc\\n-----END PRIVATE KEY-----"),
			),
			(WEBHOOK_SECRET_ENV, Some("webhook-secret")),
			(API_BASE_URL_ENV, None),
		]);

		// Act
		let settings =
			GitHubAppSettings::from_env().expect("required GitHub App settings should load");

		// Assert
		assert_eq!(settings.app_id, 12345);
		assert_eq!(
			settings.private_key_pem,
			"-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----"
		);
		assert_eq!(settings.webhook_secret, "webhook-secret");
		assert_eq!(settings.api_base_url, "https://api.github.com");
	}

	#[rstest]
	#[serial(env_github_app_settings)]
	fn test_github_app_settings_rejects_missing_private_key() {
		// Arrange
		let _env = EnvGuard::set(vec![
			(APP_ID_ENV, Some("12345")),
			(PRIVATE_KEY_PEM_ENV, Some("   ")),
			(WEBHOOK_SECRET_ENV, Some("webhook-secret")),
			(API_BASE_URL_ENV, Some("https://github.example.test/api/v3")),
		]);

		// Act
		let err = GitHubAppSettings::from_env().expect_err("blank private key should be rejected");

		// Assert
		assert_eq!(
			err.to_string(),
			"REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM is required"
		);
	}

	#[rstest]
	#[serial(env_github_app_settings)]
	fn test_github_app_settings_rejects_escaped_blank_private_key() {
		// Arrange
		let _env = EnvGuard::set(vec![
			(APP_ID_ENV, Some("12345")),
			(PRIVATE_KEY_PEM_ENV, Some("\\n\\n")),
			(WEBHOOK_SECRET_ENV, Some("webhook-secret")),
			(API_BASE_URL_ENV, Some("https://github.example.test/api/v3")),
		]);

		// Act
		let err = GitHubAppSettings::from_env()
			.expect_err("escaped blank private key should be rejected");

		// Assert
		assert_eq!(
			err.to_string(),
			"REINHARDT_CLOUD_GITHUB_APP_PRIVATE_KEY_PEM is required"
		);
	}

	#[rstest]
	fn test_github_app_settings_debug_redacts_secrets() {
		// Arrange
		let settings = GitHubAppSettings {
			app_id: 12345,
			private_key_pem: "secret-private-key".to_string(),
			webhook_secret: "secret-webhook-token".to_string(),
			api_base_url: "https://api.github.com".to_string(),
		};

		// Act
		let debug = format!("{settings:?}");

		// Assert
		assert!(!debug.contains("secret-private-key"));
		assert!(!debug.contains("secret-webhook-token"));
		assert!(debug.contains("[redacted]"));
		assert!(debug.contains("GitHubAppSettings"));
		assert!(debug.contains("https://api.github.com"));
	}
}
