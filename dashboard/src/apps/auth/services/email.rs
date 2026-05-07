//! Email sending service for auth flows (verification and password reset).

use reinhardt::conf::EmailSettings;
use reinhardt::mail::templates::{TemplateContext, TemplateEmailBuilder};
use reinhardt::mail::{EmailBackend, SmtpBackend, SmtpConfig, SmtpSecurity};

/// Apply `REINHARDT_EMAIL__*` environment variable overrides on top of an
/// `EmailSettings` value loaded from TOML.
///
/// The settings system's `EnvSource` does not yet map double-underscore env
/// keys to nested TOML sections, so we layer the overrides here. Each env
/// var is optional; only fields with a corresponding env var set are
/// touched. Parse failures are surfaced as `Err` to mirror the strictness
/// of TOML deserialization.
fn apply_email_env_overrides(mut email: EmailSettings) -> Result<EmailSettings, String> {
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__BACKEND") {
		email.backend = v;
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__HOST") {
		email.host = v;
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__PORT") {
		email.port = v
			.parse()
			.map_err(|e| format!("Invalid REINHARDT_EMAIL__PORT: {e}"))?;
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__USERNAME") {
		email.username = Some(v);
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__PASSWORD") {
		email.password = Some(v);
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__USE_TLS") {
		email.use_tls = v
			.parse()
			.map_err(|e| format!("Invalid REINHARDT_EMAIL__USE_TLS: {e}"))?;
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__USE_SSL") {
		email.use_ssl = v
			.parse()
			.map_err(|e| format!("Invalid REINHARDT_EMAIL__USE_SSL: {e}"))?;
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__FROM_EMAIL") {
		email.from_email = v;
	}
	if let Ok(v) = std::env::var("REINHARDT_EMAIL__TIMEOUT") {
		email.timeout = Some(
			v.parse()
				.map_err(|e| format!("Invalid REINHARDT_EMAIL__TIMEOUT: {e}"))?,
		);
	}
	Ok(email)
}

/// Resolve the active `EmailSettings`, layering `REINHARDT_EMAIL__*` env
/// var overrides on top of TOML-loaded settings.
///
/// Use this from any call site that needs an email-related field
/// (`from_email`, host, port, etc.) so the env-var override path is honored
/// consistently across the codebase.
pub fn resolved_email_settings() -> Result<EmailSettings, String> {
	let settings = crate::config::settings::get_settings();
	apply_email_env_overrides(settings.email.clone())
}

/// Pick the SMTP security mode for an `EmailSettings` value.
///
/// Implicit TLS (`use_ssl`) takes precedence over STARTTLS (`use_tls`) when
/// both flags are set — the two modes are mutually exclusive at the wire
/// level, and the historical dashboard behavior favors the stricter
/// implicit-TLS mode (port 465 typical).
fn select_smtp_security(email: &EmailSettings) -> SmtpSecurity {
	if email.use_ssl {
		SmtpSecurity::Tls
	} else if email.use_tls {
		SmtpSecurity::StartTls
	} else {
		SmtpSecurity::None
	}
}

/// Build an SMTP email backend from the resolved `EmailSettings`.
///
/// `REINHARDT_EMAIL__*` env vars override the TOML-loaded values; see
/// [`resolved_email_settings`] for the full list of supported keys.
pub fn get_email_backend() -> Result<Box<dyn EmailBackend>, String> {
	let email = resolved_email_settings()?;

	let mut config =
		SmtpConfig::new(&email.host, email.port).with_security(select_smtp_security(&email));

	if let (Some(username), Some(password)) = (&email.username, &email.password) {
		config = config.with_credentials(username.clone(), password.clone());
	}

	if let Some(timeout_secs) = email.timeout {
		config = config.with_timeout(std::time::Duration::from_secs(timeout_secs));
	}

	let backend = SmtpBackend::new(config).map_err(|e| format!("SMTP backend error: {e}"))?;
	Ok(Box::new(backend))
}

/// Send a verification email to a newly registered user.
pub async fn send_verification_email(
	to_email: &str,
	username: &str,
	verification_url: &str,
	backend: &dyn EmailBackend,
	from_email: &str,
) -> Result<(), String> {
	let mut ctx = TemplateContext::new();
	ctx.insert("username".to_string(), username.into());
	ctx.insert("verification_url".to_string(), verification_url.into());

	let email = TemplateEmailBuilder::new()
		.from(from_email)
		.to(vec![to_email.to_string()])
		.subject_template("Verify your email address")
		.body_template(
			"Hi {{username}},\n\n\
			 Please verify your email address by visiting the following URL:\n\n\
			 {{verification_url}}\n\n\
			 This link will expire in 24 hours.\n\n\
			 If you did not create an account, you can safely ignore this email.",
		)
		.html_template(
			"<h2>Welcome, {{username}}!</h2>\
			 <p>Please verify your email address by clicking the link below:</p>\
			 <p><a href=\"{{verification_url}}\">Verify Email</a></p>\
			 <p>This link will expire in 24 hours.</p>\
			 <p>If you did not create an account, you can safely ignore this email.</p>",
		)
		.context(ctx)
		.build()
		.map_err(|e| format!("Failed to build verification email: {e}"))?;

	email
		.send(backend)
		.await
		.map_err(|e| format!("Failed to send verification email: {e}"))
}

/// Send a password reset email.
pub async fn send_password_reset_email(
	to_email: &str,
	reset_url: &str,
	backend: &dyn EmailBackend,
	from_email: &str,
) -> Result<(), String> {
	let mut ctx = TemplateContext::new();
	ctx.insert("reset_url".to_string(), reset_url.into());

	let email = TemplateEmailBuilder::new()
		.from(from_email)
		.to(vec![to_email.to_string()])
		.subject_template("Reset your password")
		.body_template(
			"You requested a password reset.\n\n\
			 Please visit the following URL to set a new password:\n\n\
			 {{reset_url}}\n\n\
			 This link will expire in 1 hour.\n\n\
			 If you did not request a password reset, you can safely ignore this email.",
		)
		.html_template(
			"<h2>Password Reset</h2>\
			 <p>You requested a password reset. Click the link below to set a new password:</p>\
			 <p><a href=\"{{reset_url}}\">Reset Password</a></p>\
			 <p>This link will expire in 1 hour.</p>\
			 <p>If you did not request a password reset, you can safely ignore this email.</p>",
		)
		.context(ctx)
		.build()
		.map_err(|e| format!("Failed to build reset email: {e}"))?;

	email
		.send(backend)
		.await
		.map_err(|e| format!("Failed to send reset email: {e}"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;
	use serial_test::serial;

	const ENV_KEYS: &[&str] = &[
		"REINHARDT_EMAIL__BACKEND",
		"REINHARDT_EMAIL__HOST",
		"REINHARDT_EMAIL__PORT",
		"REINHARDT_EMAIL__USERNAME",
		"REINHARDT_EMAIL__PASSWORD",
		"REINHARDT_EMAIL__USE_TLS",
		"REINHARDT_EMAIL__USE_SSL",
		"REINHARDT_EMAIL__FROM_EMAIL",
		"REINHARDT_EMAIL__TIMEOUT",
	];

	/// RAII guard that snapshots `REINHARDT_EMAIL__*` env vars on
	/// construction, applies caller-supplied overrides, and restores the
	/// original values when dropped. Pairs with `#[serial(env)]` so the
	/// snapshot is taken without contention from other tests.
	struct EnvGuard {
		saved: Vec<(&'static str, Option<String>)>,
	}

	impl EnvGuard {
		fn set(vars: &[(&'static str, &str)]) -> Self {
			let mut saved = Vec::with_capacity(ENV_KEYS.len());
			for key in ENV_KEYS {
				saved.push((*key, std::env::var(key).ok()));
				// SAFETY: env mutation is serialized via `#[serial(env)]`.
				unsafe { std::env::remove_var(key) };
			}
			for (key, value) in vars {
				// SAFETY: env mutation is serialized via `#[serial(env)]`.
				unsafe { std::env::set_var(key, value) };
			}
			Self { saved }
		}
	}

	impl Drop for EnvGuard {
		fn drop(&mut self) {
			for (key, value) in &self.saved {
				// SAFETY: env mutation is serialized via `#[serial(env)]`.
				unsafe {
					match value {
						Some(v) => std::env::set_var(key, v),
						None => std::env::remove_var(key),
					}
				}
			}
		}
	}

	#[rstest]
	#[serial(env)]
	fn no_env_vars_preserves_input_settings() {
		// Arrange
		let _guard = EnvGuard::set(&[]);
		let input = EmailSettings::default();

		// Act
		let resolved = apply_email_env_overrides(input.clone()).unwrap();

		// Assert — every field unchanged from the TOML-derived input.
		assert_eq!(resolved.backend, input.backend);
		assert_eq!(resolved.host, input.host);
		assert_eq!(resolved.port, input.port);
		assert_eq!(resolved.username, input.username);
		assert_eq!(resolved.password, input.password);
		assert_eq!(resolved.use_tls, input.use_tls);
		assert_eq!(resolved.use_ssl, input.use_ssl);
		assert_eq!(resolved.from_email, input.from_email);
		assert_eq!(resolved.timeout, input.timeout);
	}

	#[rstest]
	#[serial(env)]
	fn host_and_port_env_overrides() {
		// Arrange
		let _guard = EnvGuard::set(&[
			("REINHARDT_EMAIL__HOST", "smtp.example.test"),
			("REINHARDT_EMAIL__PORT", "2525"),
		]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert_eq!(resolved.host, "smtp.example.test");
		assert_eq!(resolved.port, 2525);
	}

	#[rstest]
	#[serial(env)]
	fn username_password_env_overrides_credentials() {
		// Arrange
		let _guard = EnvGuard::set(&[
			("REINHARDT_EMAIL__USERNAME", "smtp-user"),
			("REINHARDT_EMAIL__PASSWORD", "smtp-pass"),
		]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert_eq!(resolved.username, Some("smtp-user".to_string()));
		assert_eq!(resolved.password, Some("smtp-pass".to_string()));
	}

	#[rstest]
	#[serial(env)]
	fn use_tls_env_sets_starttls_flag() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__USE_TLS", "true")]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert!(resolved.use_tls);
		assert!(!resolved.use_ssl);
	}

	#[rstest]
	#[serial(env)]
	fn use_ssl_env_sets_implicit_tls_flag() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__USE_SSL", "true")]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert!(resolved.use_ssl);
		assert!(!resolved.use_tls);
	}

	#[rstest]
	#[serial(env)]
	fn use_ssl_takes_precedence_over_use_tls() {
		// Arrange — both flags enabled simulates a misconfiguration; the
		// runtime must choose implicit TLS (the stricter mode).
		let _guard = EnvGuard::set(&[
			("REINHARDT_EMAIL__USE_TLS", "true"),
			("REINHARDT_EMAIL__USE_SSL", "true"),
		]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();
		let security = select_smtp_security(&resolved);

		// Assert — both flags survive the override pass; the backend
		// chooses implicit TLS via `select_smtp_security`.
		assert!(resolved.use_tls);
		assert!(resolved.use_ssl);
		assert!(
			matches!(security, SmtpSecurity::Tls),
			"expected SmtpSecurity::Tls, got {security:?}"
		);
	}

	#[rstest]
	#[serial(env)]
	fn select_smtp_security_falls_back_to_none() {
		// Arrange / Act
		let security = select_smtp_security(&EmailSettings::default());

		// Assert — defaults disable both flags, yielding plaintext.
		assert!(
			matches!(security, SmtpSecurity::None),
			"expected SmtpSecurity::None, got {security:?}"
		);
	}

	#[rstest]
	#[serial(env)]
	fn select_smtp_security_picks_starttls_for_use_tls_only() {
		// Arrange — `EmailSettings` is `#[non_exhaustive]`, so mutate a
		// `Default` instance rather than using struct-update syntax.
		let mut email = EmailSettings::default();
		email.use_tls = true;

		// Act
		let security = select_smtp_security(&email);

		// Assert
		assert!(
			matches!(security, SmtpSecurity::StartTls),
			"expected SmtpSecurity::StartTls, got {security:?}"
		);
	}

	#[rstest]
	#[serial(env)]
	fn from_email_env_overrides_outbound_address() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__FROM_EMAIL", "sender@example.test")]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert_eq!(resolved.from_email, "sender@example.test");
	}

	#[rstest]
	#[serial(env)]
	fn timeout_env_overrides_socket_timeout() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__TIMEOUT", "30")]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert_eq!(resolved.timeout, Some(30));
	}

	#[rstest]
	#[serial(env)]
	fn backend_env_overrides_backend_field() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__BACKEND", "smtp")]);

		// Act
		let resolved = apply_email_env_overrides(EmailSettings::default()).unwrap();

		// Assert
		assert_eq!(resolved.backend, "smtp");
	}

	#[rstest]
	#[serial(env)]
	fn invalid_port_returns_error() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__PORT", "not-a-port")]);

		// Act
		let result = apply_email_env_overrides(EmailSettings::default());

		// Assert
		let err = result.unwrap_err();
		assert!(
			err.contains("Invalid REINHARDT_EMAIL__PORT"),
			"unexpected error: {err}"
		);
	}

	#[rstest]
	#[serial(env)]
	fn invalid_timeout_returns_error() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__TIMEOUT", "abc")]);

		// Act
		let result = apply_email_env_overrides(EmailSettings::default());

		// Assert
		let err = result.unwrap_err();
		assert!(
			err.contains("Invalid REINHARDT_EMAIL__TIMEOUT"),
			"unexpected error: {err}"
		);
	}

	#[rstest]
	#[serial(env)]
	fn invalid_use_tls_returns_error() {
		// Arrange
		let _guard = EnvGuard::set(&[("REINHARDT_EMAIL__USE_TLS", "maybe")]);

		// Act
		let result = apply_email_env_overrides(EmailSettings::default());

		// Assert
		let err = result.unwrap_err();
		assert!(
			err.contains("Invalid REINHARDT_EMAIL__USE_TLS"),
			"unexpected error: {err}"
		);
	}
}
