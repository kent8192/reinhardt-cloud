//! Database-backed identities and revocable opaque sessions.

use chrono::{Duration, Utc};
use reinhardt::auth::{Argon2Hasher, PasswordHasher};
use reinhardt::db::orm::Model;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::models::{Session, UserAccount};
use super::services::SessionFacts;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Actor {
	pub id: Uuid,
	pub email: String,
}

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
	#[error("Invalid credentials")]
	InvalidCredentials,
	#[error("Identity storage is unavailable")]
	Storage(#[from] reinhardt::core::exception::Error),
	#[error("Password processing is unavailable")]
	PasswordWorker,
}

pub fn token_hash(token: &str) -> String {
	hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn new_token() -> String {
	format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Reload the session and account on every authorization boundary.
pub async fn session_actor(token: &str) -> Result<Option<Actor>, IdentityError> {
	if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
		return Ok(None);
	}
	let sessions = Session::objects()
		.filter(Session::field_token_hash().eq(token_hash(token)))
		.limit(1)
		.all()
		.await?;
	let Some(session) = sessions.first() else {
		return Ok(None);
	};
	let users = UserAccount::objects().get(session.user_id()).all().await?;
	let Some(user) = users.first() else {
		return Ok(None);
	};
	let facts = SessionFacts {
		user_id: user.id,
		expires_at: session.expires_at,
		revoked: session.revoked,
		user_active: user.active,
	};
	Ok(facts.authenticate(Utc::now()).ok().map(|id| Actor {
		id,
		email: user.email.clone(),
	}))
}

/// Verify credentials off the async executor and issue a fresh session identity.
pub async fn sign_in(email: &str, password: String) -> Result<String, IdentityError> {
	if password.is_empty() || password.len() > 1024 || email.len() > 254 {
		return Err(IdentityError::InvalidCredentials);
	}
	let users = UserAccount::objects()
		.filter(UserAccount::field_email().eq(email.trim().to_ascii_lowercase()))
		.limit(1)
		.all()
		.await?;
	let user = users.into_iter().next();
	let password_hash = user.as_ref().map(|user| user.password_hash.clone());
	let verified = tokio::task::spawn_blocking(move || {
		// Unknown accounts still perform Argon2 work before returning the same error.
		let hash = password_hash.unwrap_or_else(|| {
			static DUMMY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
			DUMMY
				.get_or_init(|| {
					Argon2Hasher
						.hash(&new_token())
						.expect("Argon2 parameters are valid")
				})
				.clone()
		});
		Argon2Hasher.verify(&password, &hash)
	})
	.await
	.map_err(|_| IdentityError::PasswordWorker)??;
	let user = user
		.filter(|user| user.active && verified)
		.ok_or(IdentityError::InvalidCredentials)?;
	let token = new_token();
	let session = Session::new()
		.user(user.id)
		.token_hash(token_hash(&token))
		.expires_at(Utc::now() + Duration::hours(24))
		.revoked(false)
		.finish();
	Session::objects().create(&session).await?;
	Ok(token)
}

/// Revocation remains authoritative after the browser discards its cookie.
pub async fn revoke_session(token: &str) -> Result<(), IdentityError> {
	Session::objects()
		.filter(Session::field_token_hash().eq(token_hash(token)))
		.update_fields([Session::field_revoked().assign(true)])
		.await?;
	Ok(())
}
