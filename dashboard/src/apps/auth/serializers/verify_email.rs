//! Request serializer for email verification.

use serde::Deserialize;

/// Path parameter for email verification URL — currently unused as
/// the view extracts the token directly via `Path<String>`.
#[derive(Debug, Clone, Deserialize)]
pub struct VerifyEmailPath {
	/// HMAC-signed verification token from the email link.
	pub token: String,
}
