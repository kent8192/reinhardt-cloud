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
pub use local_auth::LocalAuthService;
pub use mailer::{EmailSender, LettreSmtpSender, MailerError, NullEmailSender};
pub use session::validate_session;
