//! Email sending service for auth flows (verification and password reset).
//!
//! Provides [`EmailService`] resolved via `#[injectable_factory]`,
//! capturing the SMTP backend and `from_email` once at factory time.

use std::sync::Arc;

use reinhardt::conf::EmailSettings;
use reinhardt::di::{Depends, injectable_factory};
use reinhardt::mail::templates::{TemplateContext, TemplateEmailBuilder};
use reinhardt::mail::{EmailBackend, backend_from_settings};

use crate::config::settings::ProjectSettings;

/// Construct an email backend from a resolved `EmailSettings` snapshot.
///
/// Extracted from the [`EmailService`] DI factory so backend construction
/// stays on the settings-first API exposed by `reinhardt-mail` and honors
/// non-SMTP development backends such as `console` and `memory`. Refs #666.
fn build_email_backend(email: &EmailSettings) -> Result<Box<dyn EmailBackend>, String> {
	let backend = backend_from_settings(email).map_err(|e| format!("email backend error: {e}"))?;
	Ok(backend)
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
async fn create_email_service(#[inject] settings: Depends<ProjectSettings>) -> EmailService {
	let backend = build_email_backend(&settings.email)
		.expect("Failed to build email backend: check Reinhardt email settings");
	EmailService {
		backend: Arc::from(backend),
		from_email: settings.email.from_email.clone(),
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
	use rstest::rstest;

	#[rstest]
	#[tokio::test]
	async fn test_email_service_uses_configured_from_email() {
		// Arrange
		let mut email = EmailSettings::default();
		email.from_email = "test@example.test".to_string();
		let backend = build_email_backend(&email).expect("email backend should build");

		// Act
		let svc = EmailService {
			backend: Arc::from(backend),
			from_email: email.from_email.clone(),
		};

		// Assert
		assert_eq!(svc.from_email(), "test@example.test");
	}

	#[rstest]
	#[tokio::test]
	async fn test_email_service_factory_honors_console_backend() {
		// Arrange
		let mut email = EmailSettings::default();
		email.backend = "console".to_string();
		email.from_email = "test@example.test".to_string();
		let backend = build_email_backend(&email).expect("console backend should build");
		let svc = EmailService {
			backend: Arc::from(backend),
			from_email: email.from_email.clone(),
		};

		// Act
		let result = svc
			.send_verification_email(
				"user@example.test",
				"dev-user",
				"http://localhost:8000/api/auth/verify-email/token/",
			)
			.await;

		// Assert
		assert_eq!(result, Ok(()));
	}
}
