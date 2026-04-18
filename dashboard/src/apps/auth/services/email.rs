//! Email sending service for auth flows (verification and password reset).

use reinhardt::mail::templates::{TemplateContext, TemplateEmailBuilder};
use reinhardt::mail::{EmailBackend, SmtpBackend, SmtpConfig, SmtpSecurity};

/// Build an email backend from the application's `EmailSettings`.
///
/// Checks `REINHARDT_EMAIL__*` environment variables first to allow
/// tests to override the SMTP target (e.g., pointing to a Mailpit
/// container). The settings system's `EnvSource` does not support
/// nested keys via `__`, so this direct check is necessary.
pub fn get_email_backend() -> Result<Box<dyn EmailBackend>, String> {
	// Direct env var override for testing — the settings system cannot
	// map REINHARDT_EMAIL__* to nested email.* keys.
	if std::env::var("REINHARDT_EMAIL__BACKEND").as_deref() == Ok("smtp")
		&& let (Ok(host), Ok(port_str)) = (
			std::env::var("REINHARDT_EMAIL__HOST"),
			std::env::var("REINHARDT_EMAIL__PORT"),
		) {
		let port: u16 = port_str
			.parse()
			.map_err(|e| format!("Invalid REINHARDT_EMAIL__PORT: {e}"))?;
		return get_email_backend_with_config(&host, port);
	}

	let settings = crate::config::settings::get_settings();
	let email = &settings.email;

	let security = if email.use_ssl {
		SmtpSecurity::Tls
	} else if email.use_tls {
		SmtpSecurity::StartTls
	} else {
		SmtpSecurity::None
	};

	let mut config = SmtpConfig::new(&email.host, email.port).with_security(security);

	if let (Some(username), Some(password)) = (&email.username, &email.password) {
		config = config.with_credentials(username.clone(), password.clone());
	}

	if let Some(timeout_secs) = email.timeout {
		config = config.with_timeout(std::time::Duration::from_secs(timeout_secs));
	}

	let backend = SmtpBackend::new(config).map_err(|e| format!("SMTP backend error: {e}"))?;
	Ok(Box::new(backend))
}

/// Build an email backend from explicit SMTP parameters (for testing).
pub fn get_email_backend_with_config(
	host: &str,
	port: u16,
) -> Result<Box<dyn EmailBackend>, String> {
	let config = SmtpConfig::new(host, port).with_security(SmtpSecurity::None);
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
