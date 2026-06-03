//! Email sending service for auth flows (verification and password reset).
//!
//! Provides [`EmailService`] resolved via `#[injectable_factory]`,
//! capturing the SMTP backend and `from_email` once at factory time.

use std::sync::Arc;

use reinhardt::conf::EmailSettings;
use reinhardt::di::{Depends, injectable_factory};
use reinhardt::mail::templates::{TemplateContext, TemplateEmailBuilder};
use reinhardt::mail::{EmailBackend, create_smtp_backend_from_settings};

use crate::config::settings::ProjectSettings;

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
/// Used by the [`ResolvedEmailSettings`] factory and by the inline helper
/// tests covering `apply_email_env_overrides`.
fn resolved_email_settings(settings: &ProjectSettings) -> Result<EmailSettings, String> {
	apply_email_env_overrides(settings.email.clone())
}

/// Construct an SMTP backend from a resolved `EmailSettings` snapshot.
///
/// Extracted from the [`EmailService`] DI factory so backend construction
/// stays on the settings-first API exposed by `reinhardt-mail`.
fn build_smtp_backend(email: &EmailSettings) -> Result<Box<dyn EmailBackend>, String> {
	let backend =
		create_smtp_backend_from_settings(email).map_err(|e| format!("SMTP backend error: {e}"))?;
	Ok(Box::new(backend))
}

/// Resolved email settings captured at DI resolution time.
///
/// Wrapper newtype to satisfy the DI pseudo-orphan rule
/// (kent8192/reinhardt-web#3468) — `EmailSettings` lives in the
/// framework crate and cannot be registered directly. Singleton-scoped
/// so settings + env-var overrides are read once at first resolve.
pub struct ResolvedEmailSettings(pub EmailSettings);

/// DI factory — resolves `EmailSettings` from the composed
/// [`ProjectSettings`] snapshot, applying the `REINHARDT_EMAIL__*`
/// env-var overrides on top. Panics on env-var parse errors, which are
/// treated as deploy-time configuration errors rather than recoverable
/// faults.
#[injectable_factory(scope = "singleton")]
async fn create_resolved_email_settings(
	#[inject] settings: Depends<ProjectSettings>,
) -> ResolvedEmailSettings {
	ResolvedEmailSettings(
		resolved_email_settings(settings.as_ref())
			.expect("Failed to resolve email settings: check REINHARDT_EMAIL__* env vars"),
	)
}

/// Email lifecycle service backed by an SMTP transport.
///
/// Constructs the SMTP backend once at factory time and shares it across
/// all requests; individual `send_*` calls reuse the same connection
/// pool instead of re-resolving settings on every call.
pub struct EmailService {
	backend: Arc<dyn EmailBackend>,
	from_email: String,
}

/// DI factory — `singleton` because the SMTP transport pool is reusable
/// across requests and connection setup is expensive.
#[injectable_factory(scope = "singleton")]
async fn create_email_service(#[inject] settings: Depends<ResolvedEmailSettings>) -> EmailService {
	let backend = build_smtp_backend(&settings.0)
		.expect("Failed to build SMTP email backend: check REINHARDT_EMAIL__* env vars");
	EmailService {
		backend: Arc::from(backend),
		from_email: settings.0.from_email.clone(),
	}
}

impl EmailService {
	/// Outbound `From:` address captured at factory time.
	pub fn from_email(&self) -> &str {
		&self.from_email
	}

	/// Send a verification email to a newly registered user.
	pub async fn send_verification_email(
		&self,
		to_email: &str,
		username: &str,
		verification_url: &str,
	) -> Result<(), String> {
		let mut ctx = TemplateContext::new();
		ctx.insert("username".to_string(), username.into());
		ctx.insert("verification_url".to_string(), verification_url.into());

		let email = TemplateEmailBuilder::new()
			.from(&self.from_email)
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
			.send(self.backend.as_ref())
			.await
			.map_err(|e| format!("Failed to send verification email: {e}"))
	}

	/// Send a password reset email.
	pub async fn send_password_reset_email(
		&self,
		to_email: &str,
		reset_url: &str,
	) -> Result<(), String> {
		let mut ctx = TemplateContext::new();
		ctx.insert("reset_url".to_string(), reset_url.into());

		let email = TemplateEmailBuilder::new()
			.from(&self.from_email)
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
			.send(self.backend.as_ref())
			.await
			.map_err(|e| format!("Failed to send reset email: {e}"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::config::test_helpers::make_test_di_context;
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
	#[tokio::test]
	async fn test_email_service_factory_resolves_with_overridden_settings() {
		// Arrange — override ResolvedEmailSettings so the factory wires
		// the hand-built dependency. SmtpBackend::new only validates the
		// configuration; it does not open a TCP connection until a send
		// is attempted, so this works without a live SMTP server.
		let mut email = EmailSettings::default();
		email.host = "smtp.test.invalid".to_string();
		email.port = 2525;
		email.from_email = "test@example.test".to_string();
		let ctx = make_test_di_context(|scope| {
			scope.set(ResolvedEmailSettings(email));
		});

		// Act
		let svc: Arc<EmailService> = ctx
			.resolve::<EmailService>()
			.await
			.expect("EmailService factory should resolve when settings are registered");

		// Assert — factory wired from_email through and the backend
		// is constructible (full round-trip is exercised by integration
		// tests against a live SMTP container).
		assert_eq!(svc.from_email(), "test@example.test");
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
