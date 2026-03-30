//! Utility functions for auth views.

/// Get JWT secret from environment.
///
/// # Panics
///
/// Panics if `REINHARDT_CLOUD_JWT_SECRET` environment variable is not set.
/// In production, this MUST be set to a cryptographically random
/// string of at least 32 bytes.
pub(crate) fn jwt_secret() -> String {
	std::env::var("REINHARDT_CLOUD_JWT_SECRET")
		.expect("REINHARDT_CLOUD_JWT_SECRET environment variable must be set")
}
