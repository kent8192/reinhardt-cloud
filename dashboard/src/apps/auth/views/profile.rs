//! Profile views for auth app.

use chrono::{Duration, Utc};
use rand::TryRngCore;
use rand::rngs::OsRng;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Response, StatusCode};
use reinhardt::{get, patch};
use sha2::{Digest, Sha256};
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::{EmailVerificationToken, User};
use crate::apps::auth::serializers::{ProfileResponse, UpdateProfileRequest};
use crate::apps::auth::services::mailer;

/// Return the authenticated user's profile.
#[get("/profile/", name = "profile")]
pub async fn profile(#[inject] AuthInfo(state): AuthInfo) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query user profile: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

	let resp = ProfileResponse::from(user);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}

/// Update the authenticated user's profile fields.
///
/// Email changes are NOT applied immediately. Instead, a verification token
/// is generated, stored hashed (SHA-256), and a confirmation link is emailed
/// to the new address. Only when the user clicks that link is `user.email`
/// updated. This prevents unauthorized email takeover via a hijacked session.
#[patch("/profile/", name = "profile_update", pre_validate = true)]
pub async fn profile_update(
	body: Json<UpdateProfileRequest>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID in token: {e}")))?;

	let mut user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query user for profile update: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

	// Trim values before applying — reject empty-after-trim values to prevent
	// whitespace-only strings from bypassing length(min=1) validation.
	if let Some(ref first_name) = body.first_name {
		let trimmed = first_name.trim();
		if trimmed.is_empty() {
			return Err(AppError::Validation(
				"first_name must not be blank".to_string(),
			));
		}
		user.first_name = trimmed.to_string();
	}
	if let Some(ref last_name) = body.last_name {
		let trimmed = last_name.trim();
		if trimmed.is_empty() {
			return Err(AppError::Validation(
				"last_name must not be blank".to_string(),
			));
		}
		user.last_name = trimmed.to_string();
	}

	// Handle email change request separately: generate + email a verification
	// token instead of mutating `user.email`.
	let mut email_change_requested: Option<String> = None;
	if let Some(ref email) = body.email {
		let trimmed = email.trim();
		if trimmed.is_empty() {
			return Err(AppError::Validation("email must not be blank".to_string()));
		}
		if trimmed != user.email {
			email_change_requested = Some(trimmed.to_string());
		}
	}

	// Persist non-email changes immediately.
	let updated = User::objects().update(&user).await.map_err(|e| {
		error!("Failed to update user profile: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	if let Some(pending_email) = email_change_requested {
		issue_email_verification(&updated, &pending_email).await?;
		let resp = serde_json::json!({ "status": "verification_sent" });
		return Ok(Response::new(StatusCode::ACCEPTED)
			.with_header("Content-Type", "application/json")
			.with_body(json::to_vec(&resp)?));
	}

	let resp = ProfileResponse::from(updated);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}

/// Generate and dispatch an email-verification token for a pending email
/// change. Invalidates any prior unconsumed tokens for the user before
/// inserting the new row.
async fn issue_email_verification(user: &User, pending_email: &str) -> Result<(), AppError> {
	// Invalidate existing unconsumed tokens for this user by deleting them.
	// A single-row-at-a-time delete loop keeps us compatible with the ORM's
	// primary-key-based delete API.
	let prior = EmailVerificationToken::objects()
		.filter(
			EmailVerificationToken::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user.id.to_string()),
		)
		.all()
		.await
		.map_err(|e| {
			error!("Failed to list existing verification tokens: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
	for t in prior {
		if t.consumed_at.is_some() {
			continue;
		}
		if let Some(id) = t.id {
			if let Err(e) = EmailVerificationToken::objects().delete(id).await {
				error!("Failed to delete stale verification token {id}: {e}");
			}
		}
	}

	// 32 bytes of cryptographic randomness from the OS.
	let mut token_bytes = [0u8; 32];
	OsRng.try_fill_bytes(&mut token_bytes).map_err(|e| {
		error!("OS RNG failure while generating verification token: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;
	let token_plain = hex::encode(token_bytes);
	let mut hasher = Sha256::new();
	hasher.update(token_plain.as_bytes());
	let digest = hasher.finalize();
	let token_hash = hex::encode(digest);

	let row = EmailVerificationToken {
		id: None,
		user_id: user.id,
		pending_email: pending_email.to_string(),
		token_hash,
		expires_at: Utc::now() + Duration::hours(24),
		consumed_at: None,
		created_at: Utc::now(),
	};
	EmailVerificationToken::objects()
		.create(&row)
		.await
		.map_err(|e| {
			error!("Failed to persist verification token: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let base_url = std::env::var("PUBLIC_BASE_URL").unwrap_or_default();
	let url = format!(
		"{base_url}/auth/verify-email/?user_id={}&token={}",
		user.id, token_plain
	);
	let body = format!(
		"A request was made to change your Reinhardt Cloud account email to \
		 {pending_email}.\n\nTo confirm this change, visit:\n\n{url}\n\n\
		 If you did not request this change, you can safely ignore this email; \
		 your account is unaffected."
	);

	let sender = mailer::default_sender();
	if let Err(e) = sender
		.send(pending_email, "Confirm your email change", &body)
		.await
	{
		// Token remains in the DB even if delivery fails; the user may retry.
		error!("Failed to send email verification message: {e}");
		return Err(AppError::Internal(
			"Failed to send verification email".to_string(),
		));
	}
	Ok(())
}
