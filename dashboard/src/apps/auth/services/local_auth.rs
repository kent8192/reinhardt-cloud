//! Local authentication service implementation.
//!
//! Wraps existing auth functions behind the `AuthService` trait for
//! DI-based usage across both REST and gRPC handlers.

use async_trait::async_trait;
use reinhardt::BaseUser;
use reinhardt::db::orm::Model;
use reinhardt::di::FactoryOutput;
use reinhardt_cloud_core::auth::{self, Claims};
use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::traits::AuthService;
use reinhardt_cloud_types::User as DomainUser;
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::config::settings::get_jwt_secret;

/// Local authentication service backed by the ORM and JWT utilities.
///
/// Uses the database for credential verification and user lookups,
/// and `reinhardt_cloud_core::auth` for token operations (used by
/// the gRPC layer).
pub struct LocalAuthService;

#[reinhardt::di::injectable_key]
pub struct LocalAuthServiceKey;

impl LocalAuthService {
	/// Create a new `LocalAuthService`.
	pub fn new() -> Self {
		Self
	}
}

impl Default for LocalAuthService {
	fn default() -> Self {
		Self::new()
	}
}

/// DI factory — auto-registers `LocalAuthService` as a singleton.
/// Tests can override via `SingletonScope::set()` before resolution.
#[reinhardt::di::injectable(scope = "singleton")]
async fn create_local_auth_service() -> FactoryOutput<LocalAuthServiceKey, LocalAuthService> {
	FactoryOutput::new(LocalAuthService::new())
}

impl LocalAuthService {
	/// Get the JWT secret for gRPC token operations.
	///
	/// Resolves via `get_jwt_secret()` (TOML key
	/// `jwt_secret` with `REINHARDT_CLOUD_JWT_SECRET` env-var fallback).
	/// This is only used by the gRPC `AuthService` trait implementation,
	/// not by the dashboard's HTTP cookie-based auth.
	///
	/// Issue: kent8192/reinhardt-cloud#494
	fn secret(&self) -> Result<String, ApiError> {
		get_jwt_secret().ok_or_else(|| {
			ApiError::Internal(
				"JWT secret not configured: set jwt_secret in TOML or REINHARDT_CLOUD_JWT_SECRET env var"
					.to_string(),
			)
		})
	}
}

#[async_trait]
impl AuthService for LocalAuthService {
	async fn authenticate(&self, username: &str, password: &str) -> Result<Claims, ApiError> {
		let user = User::objects()
			.filter(User::field_username().eq(username.trim().to_string()))
			.first()
			.await
			.map_err(|e| {
				error!("Failed to query user during authentication: {e}");
				ApiError::Internal("Internal server error".to_string())
			})?
			.ok_or_else(|| ApiError::Unauthorized("Invalid credentials".to_string()))?;

		let valid = user.check_password(password).map_err(|e| {
			error!("Password verification failed: {e}");
			ApiError::Internal("Internal server error".to_string())
		})?;
		if !valid {
			return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
		}

		if !user.is_active() {
			return Err(ApiError::Unauthorized("Invalid credentials".to_string()));
		}

		self.create_token(&user.id().to_string(), user.get_username())
			.await
			.and_then(|token| self.validate_token_sync(&token))
	}

	async fn create_token(&self, user_id: &str, username: &str) -> Result<String, ApiError> {
		let secret = self.secret()?;
		let uid = Uuid::parse_str(user_id)
			.map_err(|e| ApiError::BadRequest(format!("Invalid user ID: {e}")))?;
		auth::create_token(uid, username, secret.as_bytes(), 24)
			.map_err(|e| ApiError::Internal(format!("Token creation failed: {e}")))
	}

	async fn verify_token(&self, token: &str) -> Result<Claims, ApiError> {
		Ok(self.validate_token_sync(token)?)
	}

	async fn get_user_info(&self, user_id: &str) -> Result<DomainUser, ApiError> {
		// Validate UUID format
		let _uid = Uuid::parse_str(user_id)
			.map_err(|e| ApiError::BadRequest(format!("Invalid user ID: {e}")))?;

		let user = User::objects()
			.filter(User::field_id().eq(user_id.to_string()))
			.first()
			.await
			.map_err(|e| {
				error!("Failed to query user: {e}");
				ApiError::Internal("Internal server error".to_string())
			})?
			.ok_or_else(|| ApiError::NotFound(format!("User {user_id} not found")))?;

		Ok(DomainUser {
			id: user.id,
			username: user.username.clone(),
			email: user.email.clone(),
			password_hash: user.password_hash.clone().unwrap_or_default(),
		})
	}
}

impl LocalAuthService {
	/// Synchronous token validation helper.
	fn validate_token_sync(&self, token: &str) -> Result<Claims, ApiError> {
		let secret = self.secret()?;
		auth::verify_token(token, secret.as_bytes())
			.map_err(|e| ApiError::Unauthorized(format!("Invalid token: {e}")))
	}
}
