//! Utility functions for auth views.

/// Get JWT secret from environment.
///
/// Returns an error if `REINHARDT_CLOUD_JWT_SECRET` environment variable
/// is not set. In production, this MUST be set to a cryptographically
/// random string of at least 32 bytes.
pub(crate) fn jwt_secret() -> Result<String, std::env::VarError> {
	std::env::var("REINHARDT_CLOUD_JWT_SECRET")
}
