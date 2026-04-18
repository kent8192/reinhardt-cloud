//! Auth business logic services.
//!
//! Shared between REST API views and frontend server functions.

pub mod credentials;
pub mod local_auth;
pub mod mailer;
pub mod session;

pub use credentials::verify_credentials;
pub use local_auth::LocalAuthService;
pub use mailer::{EmailSender, LettreSmtpSender, MailerError, NullEmailSender};
pub use session::{create_session, destroy_session, validate_session};
