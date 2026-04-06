//! Authentication service trait.

use async_trait::async_trait;

use crate::auth::Claims;
use crate::error::ApiError;
use reinhardt_cloud_types::User;

/// Trait for authentication and token management.
///
/// Implementations handle credential verification, JWT token
/// lifecycle, and user information retrieval. This trait is
/// object-safe and designed for DI registration as `Arc<dyn AuthService>`.
#[async_trait]
pub trait AuthService: Send + Sync + 'static {
	/// Authenticate a user by username and password.
	///
	/// Returns JWT claims on success.
	async fn authenticate(&self, username: &str, password: &str) -> Result<Claims, ApiError>;

	/// Create a signed JWT token for the given user ID and username.
	async fn create_token(&self, user_id: &str, username: &str) -> Result<String, ApiError>;

	/// Verify a JWT token and return the decoded claims.
	async fn verify_token(&self, token: &str) -> Result<Claims, ApiError>;

	/// Retrieve user information by user ID.
	async fn get_user_info(&self, user_id: &str) -> Result<User, ApiError>;
}
