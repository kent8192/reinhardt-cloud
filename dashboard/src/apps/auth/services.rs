//! Auth business logic services.
//!
//! Shared by auth server functions.

pub mod credentials;
pub mod email;
pub mod local_auth;
pub mod mailer;
pub mod oauth;
pub mod registration;
pub mod session;
pub mod token;

pub use credentials::verify_credentials;
pub use email::{EmailService, EmailServiceKey};
pub use local_auth::{LocalAuthService, LocalAuthServiceKey};
pub use mailer::{EmailSender, LettreSmtpSender, MailerError, NullEmailSender};
pub use registration::register_inactive_user;
pub use session::{
	RedisUrl, RedisUrlKey, SessionService, SessionServiceKey, session_cookie_header,
	session_id_from_cookie_header, validate_session,
};
