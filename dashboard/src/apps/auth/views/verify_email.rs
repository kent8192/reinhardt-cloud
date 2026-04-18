//! Email change verification view.
//!
//! Consumes a single-use token emailed during a profile email-change
//! request. Updates `user.email` only when the hashed token matches a
//! pending (non-consumed, non-expired) record.

use chrono::Utc;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::get;
use reinhardt::http::ViewResult;
use reinhardt::{Query, Response, StatusCode};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::{EmailVerificationToken, User};

/// Query parameters for the verification link.
#[derive(Debug, Deserialize)]
pub struct VerifyEmailQuery {
	pub user_id: String,
	pub token: String,
}

/// Confirm a pending email change.
///
/// Security properties:
/// - Plaintext token is hashed with SHA-256 before any comparison.
/// - Comparison uses `subtle::ConstantTimeEq` on fixed-size 32-byte digests
///   to avoid timing side channels.
/// - Rejects tokens that are expired or already consumed.
/// - Updates `user.email` and marks the token consumed in the same ORM
///   update sequence; if the second update fails after the email was
///   already changed, the token remains active until it expires — this is
///   an acceptable trade-off given the ORM's transaction surface is not
///   exposed through the high-level `Model` API used in this module.
#[get("/verify-email/", name = "verify_email")]
pub async fn verify_email(Query(q): Query<VerifyEmailQuery>) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(&q.user_id)
		.map_err(|_| AppError::Validation("invalid user_id".to_string()))?;

	// Hash the submitted plaintext token.
	let mut hasher = Sha256::new();
	hasher.update(q.token.as_bytes());
	let submitted_digest = hasher.finalize();
	let submitted_bytes: [u8; 32] = submitted_digest.into();

	// Load all tokens for this user, then constant-time compare against each
	// candidate. In practice there is at most one active token per user
	// (prior tokens are invalidated on issue), so the loop is O(1) in the
	// common case.
	let candidates = EmailVerificationToken::objects()
		.filter(
			EmailVerificationToken::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.all()
		.await
		.map_err(|e| {
			error!("Failed to load verification tokens: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let now = Utc::now();
	let mut matched: Option<EmailVerificationToken> = None;
	for row in candidates {
		// Decode the stored hex hash to bytes; skip malformed rows.
		let stored = match hex::decode(&row.token_hash) {
			Ok(v) if v.len() == 32 => v,
			_ => continue,
		};
		let stored_arr: [u8; 32] = match stored.try_into() {
			Ok(a) => a,
			Err(_) => continue,
		};
		// Constant-time equality on fixed-size digests.
		if stored_arr.ct_eq(&submitted_bytes).unwrap_u8() == 1 {
			matched = Some(row);
			break;
		}
	}

	let token = matched
		.ok_or_else(|| AppError::Authentication("invalid or unknown token".to_string()))?;

	if token.consumed_at.is_some() {
		return Err(AppError::Authentication(
			"token already used".to_string(),
		));
	}
	if token.expires_at <= now {
		return Err(AppError::Authentication("token expired".to_string()));
	}

	// Load the user and update the email.
	let mut user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
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
		error!(
			"Email updated but failed to mark verification token consumed: {e}"
		);
	}

	let resp = serde_json::json!({
		"status": "email_updated",
		"email": updated.email,
	});
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
