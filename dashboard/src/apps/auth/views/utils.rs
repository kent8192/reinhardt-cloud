//! Utility functions for auth views.

use reinhardt::core::exception::{Error, Result};

/// Get JWT secret from settings or environment.
///
/// Reads the JWT secret with the following priority:
/// 1. `REINHARDT_CLOUD_JWT_SECRET` environment variable
/// 2. `jwt_secret` key in the active TOML settings file (e.g., `local.toml`)
///
/// In production, this MUST be set to a cryptographically
/// random string of at least 32 bytes.
pub(crate) fn jwt_secret() -> Result<String> {
	crate::config::settings::get_jwt_secret().ok_or_else(|| {
		Error::Internal(
			"JWT secret not configured: set REINHARDT_CLOUD_JWT_SECRET env var \
			 or jwt_secret in settings TOML"
				.to_string(),
		)
	})
}
