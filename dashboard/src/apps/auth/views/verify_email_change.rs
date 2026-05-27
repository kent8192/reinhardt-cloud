//! Email-change confirmation endpoint.
//!
//! Activates a pending email-change request issued by `profile_update`. The
//! plaintext token arrives in the request body (not the URL) so it cannot
//! leak via access logs, proxies, browser history, or `Referer` headers.
//! The intended UX is:
//!
//! 1. The verification email links to a frontend confirmation page,
//!    `<PUBLIC_BASE_URL>/auth/confirm-email`, with the token carried in the
//!    URL **fragment** (`#user_id=...&token=...`). Fragments are never sent
//!    to the server, so they stay out of access logs and `Referer` headers.
//! 2. That page reads the fragment in JavaScript and POSTs the values to
//!    this endpoint as a JSON body.
//!
//! Security properties:
//! - Plaintext token is hashed with SHA-256 before any DB lookup.
//! - The lookup filters `consumed_at IS NULL AND expires_at > now()` at the
//!   DB layer; no application-side scan, so timing depends on the (uniform)
//!   index lookup rather than on token position in a candidate list.
//! - `user_id` from the request body is verified against the row to prevent
//!   cross-account confusion if a token were ever leaked between users.

use chrono::Utc;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::FilterValue;
use reinhardt::db::orm::Model;
use reinhardt::http::ViewResult;
use reinhardt::{Json, Response, StatusCode, post};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::{EmailVerificationToken, User};

/// Request body for the email-change confirmation endpoint.
#[derive(Debug, Deserialize)]
pub struct VerifyEmailChangeRequest {
	pub user_id: String,
	pub token: String,
}

/// Confirm a pending email change.
#[post("/verify-email-change/", name = "verify_email_change")]
pub async fn verify_email_change(body: Json<VerifyEmailChangeRequest>) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(&body.user_id)
		.map_err(|_| AppError::Validation("invalid user_id".to_string()))?;

	// Hash the submitted plaintext token to match what is persisted.
	let mut hasher = Sha256::new();
	hasher.update(body.token.as_bytes());
	let submitted_hash = hex::encode(hasher.finalize());

	// Direct DB lookup: token_hash is UNIQUE, and we filter out already-
	// consumed and expired rows so the row we get back is guaranteed valid
	// (modulo the user_id check below). No application-side scan.
	let now = Utc::now();
	let token = EmailVerificationToken::objects()
		.filter(EmailVerificationToken::field_token_hash().eq(submitted_hash))
		.filter(EmailVerificationToken::field_consumed_at().eq(FilterValue::Null))
		.filter(EmailVerificationToken::field_expires_at().gt(now.to_rfc3339()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to query verification token: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::Authentication("invalid or unknown token".to_string()))?;

	// Bind the token to the claimed user_id. A mismatch is treated as an
	// authentication failure with the same generic message used above so we
	// do not leak whether the token exists for a different user.
	if token.user_id != user_id {
		return Err(AppError::Authentication(
			"invalid or unknown token".to_string(),
		));
	}

	// Load the user and apply the new email.
	let mut user = User::objects()
		.filter(User::field_id().eq(user_id.to_string()))
		.first()
		.await
		.map_err(|e| {
			error!("Failed to load user for verification: {e}");
			AppError::Internal("Internal server error".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("User not found".to_string()))?;

	user.email = token.pending_email.clone();
	let updated = User::objects().update(&user).await.map_err(|e| {
		let err_lower = e.to_string().to_lowercase();
		if err_lower.contains("unique") || err_lower.contains("duplicate") {
			return AppError::Conflict("Email already in use".to_string());
		}
		error!("Failed to apply verified email change: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	// Mark the token consumed. On failure, the email is already updated
	// (unavoidable without transactional support at this ORM level) — log
	// loudly so operators can clean up the residual record.
	let mut consumed = token;
	consumed.consumed_at = Some(now);
	if let Err(e) = EmailVerificationToken::objects().update(&consumed).await {
		error!("Email updated but failed to mark verification token consumed: {e}");
	}

	let resp = serde_json::json!({
		"status": "email_updated",
		"email": updated.email,
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
