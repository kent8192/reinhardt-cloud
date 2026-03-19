//! Utility functions for auth views.

use reinhardt::core::exception::{Error, Result};

/// Get JWT secret from environment.
///
/// Returns an internal error if `NUAGES_JWT_SECRET` is not set.
/// In production, this MUST be set to a cryptographically random
/// string of at least 32 bytes.
pub(crate) fn jwt_secret() -> Result<String> {
	std::env::var("NUAGES_JWT_SECRET").map_err(|_| {
		Error::Internal("NUAGES_JWT_SECRET environment variable must be set".to_string())
	})
}
