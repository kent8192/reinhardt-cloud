//! View functions for auth endpoints.

pub mod login;
pub mod register;

pub use login::login;
pub use register::register;

/// Get JWT secret from environment.
///
/// Falls back to a default value suitable only for development.
/// In production, `NUAGES_JWT_SECRET` MUST be set to a cryptographically
/// random string of at least 32 bytes.
pub(crate) fn jwt_secret() -> String {
	std::env::var("NUAGES_JWT_SECRET")
		.unwrap_or_else(|_| "change-me-in-production-minimum-32-bytes!".to_string())
}
