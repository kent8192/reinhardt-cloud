//! Outbound email service abstraction.
//!
//! Defines the [`EmailSender`] trait plus two implementations:
//! - [`LettreSmtpSender`] — production SMTP transport backed by `lettre`,
//!   configured via `MAIL_FROM` and `MAIL_SMTP_URL` environment variables.
//! - [`NullEmailSender`] — test double that logs to `tracing` and returns
//!   `Ok(())`. Use in tests and local development where no SMTP is present.

use async_trait::async_trait;
use lettre::message::Mailbox;
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncTransport, Message, Tokio1Executor};
use std::sync::OnceLock;
use thiserror::Error;
use tracing::info;

/// Mailer failure surface.
#[derive(Debug, Error)]
pub enum MailerError {
	#[error("invalid mail configuration: {0}")]
	Config(String),
	#[error("failed to build message: {0}")]
	Build(String),
	#[error("failed to send message: {0}")]
	Transport(String),
}

/// Abstract outbound email transport.
#[async_trait]
pub trait EmailSender: Send + Sync {
	async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), MailerError>;
}

/// SMTP-backed sender using `lettre` with rustls.
///
/// Configuration is read lazily from environment:
/// - `MAIL_FROM` — the envelope `From:` mailbox (e.g. `Reinhardt <noreply@example.com>`).
/// - `MAIL_SMTP_URL` — connection string. Supported schemes:
///   - `smtps://user:pass@host:465` — implicit TLS (recommended).
///   - `smtp+insecure://user:pass@host:25` — cleartext, NO TLS. Intended
///     **only** for local development against MailHog/MailCatcher and
///     similar; never use against a remote relay or in production.
///
/// The bare `smtp://` scheme is rejected because it tends to be picked
/// accidentally for a "default" connection and silently transports
/// credentials in cleartext.
pub struct LettreSmtpSender {
	transport: OnceLock<AsyncSmtpTransport<Tokio1Executor>>,
	from: OnceLock<Mailbox>,
}

impl LettreSmtpSender {
	pub fn new() -> Self {
		Self {
			transport: OnceLock::new(),
			from: OnceLock::new(),
		}
	}

	fn resolve_from(&self) -> Result<&Mailbox, MailerError> {
		if let Some(m) = self.from.get() {
			return Ok(m);
		}
		let raw = std::env::var("MAIL_FROM")
			.map_err(|_| MailerError::Config("MAIL_FROM not set".to_string()))?;
		let parsed: Mailbox = raw
			.parse()
			.map_err(|e| MailerError::Config(format!("invalid MAIL_FROM: {e}")))?;
		let _ = self.from.set(parsed);
		Ok(self.from.get().expect("from just set"))
	}

	fn resolve_transport(&self) -> Result<&AsyncSmtpTransport<Tokio1Executor>, MailerError> {
		if let Some(t) = self.transport.get() {
			return Ok(t);
		}
		let url = std::env::var("MAIL_SMTP_URL")
			.map_err(|_| MailerError::Config("MAIL_SMTP_URL not set".to_string()))?;
		let parsed = ParsedSmtpUrl::parse(&url)?;
		let mut builder = match parsed.scheme.as_str() {
			"smtps" => AsyncSmtpTransport::<Tokio1Executor>::relay(&parsed.host)
				.map_err(|e| MailerError::Config(format!("smtps relay: {e}")))?,
			// Bare `smtp://` is intentionally rejected: defaulting to it
			// makes it trivial to transport credentials in cleartext by
			// accident. Operators that genuinely need an unencrypted
			// transport (local MailHog/MailCatcher in dev) must opt in via
			// the explicit `smtp+insecure://` scheme below.
			"smtp" => {
				return Err(MailerError::Config(
					"`smtp://` is not allowed; use `smtps://` for production or \
					 `smtp+insecure://` for local development against MailHog/MailCatcher"
						.to_string(),
				));
			}
			"smtp+insecure" => {
				AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&parsed.host)
			}
			other => {
				return Err(MailerError::Config(format!("unsupported scheme: {other}")));
			}
		};
		if let Some(port) = parsed.port {
			builder = builder.port(port);
		}
		if let Some((user, pass)) = parsed.credentials {
			builder = builder.credentials(Credentials::new(user, pass));
		}
		let transport = builder.build();
		let _ = self.transport.set(transport);
		Ok(self.transport.get().expect("transport just set"))
	}
}

impl Default for LettreSmtpSender {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl EmailSender for LettreSmtpSender {
	async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), MailerError> {
		let from = self.resolve_from()?.clone();
		let to: Mailbox = to
			.parse()
			.map_err(|e| MailerError::Build(format!("invalid recipient: {e}")))?;
		let msg = Message::builder()
			.from(from)
			.to(to)
			.subject(subject)
			.body(body.to_string())
			.map_err(|e| MailerError::Build(e.to_string()))?;
		let transport = self.resolve_transport()?;
		transport
			.send(msg)
			.await
			.map_err(|e| MailerError::Transport(e.to_string()))?;
		Ok(())
	}
}

/// No-op sender used in tests and local development.
///
/// Logs the outgoing message via `tracing` at `info` level and returns
/// success. Never contacts the network.
pub struct NullEmailSender;

impl NullEmailSender {
	pub fn new() -> Self {
		Self
	}
}

impl Default for NullEmailSender {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl EmailSender for NullEmailSender {
	async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), MailerError> {
		info!(target: "mailer.null", to, subject, "suppressed outbound email");
		let _ = body;
		Ok(())
	}
}

/// Process-wide default sender. Tests override with [`set_default_sender`].
///
/// We use a `OnceLock<Arc<dyn EmailSender>>` so the handler can resolve a
/// sender without threading it through every call site. The DI framework
/// in this project primarily supports concrete singletons via
/// `#[injectable_factory]`; a static override is the pragmatic way to
/// support a trait object with a test double today.
static DEFAULT_SENDER: std::sync::OnceLock<std::sync::Arc<dyn EmailSender>> =
	std::sync::OnceLock::new();

/// Install a process-wide default [`EmailSender`]. Returns `Err` with the
/// supplied sender if one has already been set (tests should set exactly
/// once per process).
pub fn set_default_sender(
	sender: std::sync::Arc<dyn EmailSender>,
) -> Result<(), std::sync::Arc<dyn EmailSender>> {
	DEFAULT_SENDER.set(sender)
}

/// Resolve the configured [`EmailSender`], falling back to the SMTP sender.
pub fn default_sender() -> std::sync::Arc<dyn EmailSender> {
	DEFAULT_SENDER
		.get_or_init(|| std::sync::Arc::new(LettreSmtpSender::new()))
		.clone()
}

/// Parsed SMTP URL components used by the transport builder.
///
/// Backed by the `url` crate's RFC 3986 parser so we correctly handle:
/// - bracketed IPv6 hosts (`[::1]:587`),
/// - percent-encoded credentials (`user%40example.com:p%40ss`),
/// - missing port / missing credentials.
struct ParsedSmtpUrl {
	scheme: String,
	host: String,
	port: Option<u16>,
	credentials: Option<(String, String)>,
}

impl ParsedSmtpUrl {
	fn parse(input: &str) -> Result<Self, MailerError> {
		// `url::Url` requires that custom schemes follow the same authority
		// rules as known ones; the SMTP-style schemes we accept always do.
		let parsed = url::Url::parse(input)
			.map_err(|e| MailerError::Config(format!("invalid MAIL_SMTP_URL: {e}")))?;

		let scheme = parsed.scheme().to_string();

		// `Url::host_str` strips the IPv6 brackets, which is the format
		// `lettre`'s `relay`/`builder_dangerous` expect for the hostname
		// portion.
		let host = parsed
			.host_str()
			.ok_or_else(|| MailerError::Config("MAIL_SMTP_URL missing host".to_string()))?
			.to_string();
		if host.is_empty() {
			return Err(MailerError::Config(
				"MAIL_SMTP_URL missing host".to_string(),
			));
		}

		let port = parsed.port();

		let credentials = match parsed.username() {
			"" => None,
			user => {
				let user = percent_decode(user)?;
				let pass = percent_decode(parsed.password().unwrap_or(""))?;
				Some((user, pass))
			}
		};

		Ok(Self {
			scheme,
			host,
			port,
			credentials,
		})
	}
}

/// Percent-decode a URL-encoded credential field as UTF-8. Returns a config
/// error if the bytes are not valid UTF-8 (mailer credentials are always
/// text, so a decode failure indicates a malformed URL).
fn percent_decode(input: &str) -> Result<String, MailerError> {
	percent_encoding::percent_decode_str(input)
		.decode_utf8()
		.map(|c| c.into_owned())
		.map_err(|e| MailerError::Config(format!("invalid percent-encoding in credentials: {e}")))
}
