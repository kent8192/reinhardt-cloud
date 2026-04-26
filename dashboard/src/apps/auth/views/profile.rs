//! Profile views for auth app.

use chrono::{Duration, Utc};
use rand::TryRngCore;
use rand::rngs::OsRng;
use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{Filter, FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Json, Response, StatusCode};
use reinhardt::{get, patch};
use sha2::{Digest, Sha256};
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::{EmailVerificationToken, User};
use crate::apps::auth::serializers::{ProfileResponse, UpdateProfileRequest};
use crate::apps::auth::services::mailer::{self, MailerError};

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

	// Determine whether an email change is pending before deciding the response shape.
	// The two operations — updating non-email fields and issuing the email verification
	// — are intentionally separate so that a mailer failure does not roll back name
	// changes that the user successfully submitted.  The response body communicates
	// both outcomes independently so the client can display targeted feedback:
	//   - no email field in request    → 200, `email_verification: null`
	//   - email change requested       → 202, `email_verification: { "status": "verification_sent" }`
	//   - verification email failed    → 200, `email_verification: { "status": "failed" }` (profile still updated)
	//   - no-op (same email submitted) → 200, `email_verification: null`
	//
	// Returning 202 when a verification email is dispatched signals to the client that
	// the action is accepted but not yet complete, consistent with the PR contract.
	if email_change_requested.is_none() {
		// Fast path: no email change, just persist and return.
		let updated = User::objects().update(&user).await.map_err(|e| {
			error!("Failed to update user profile: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
		let resp = serde_json::json!({
			"profile": ProfileResponse::from(updated),
			"email_verification": serde_json::Value::Null,
		});
		return Ok(Response::new(StatusCode::OK)
			.with_header("Content-Type", "application/json")
			.with_body(json::to_vec(&resp)?));
	}

	// Persist non-email fields first. A failure here aborts before any email
	// is sent, keeping the operation atomic from the caller's perspective.
	let updated = User::objects().update(&user).await.map_err(|e| {
		error!("Failed to update user profile: {e}");
		AppError::Internal("Internal server error".to_string())
	})?;

	let pending_email = email_change_requested.expect("checked is_none above");
	match issue_email_verification(&updated, &pending_email).await {
		Ok(()) => {
			let resp = serde_json::json!({
				"profile": ProfileResponse::from(updated),
				"email_verification": { "status": "verification_sent" },
			});
			Ok(Response::new(StatusCode::ACCEPTED)
				.with_header("Content-Type", "application/json")
				.with_body(json::to_vec(&resp)?))
		}
		Err(EmailVerificationError::Send(e)) => {
			// Profile fields were already persisted. Mail delivery failure is
			// logged but not surfaced as a 5xx so the client knows the profile
			// part succeeded and can offer a "resend" affordance.
			error!("Email verification dispatch failed: {e}");
			let resp = serde_json::json!({
				"profile": ProfileResponse::from(updated),
				"email_verification": { "status": "failed" },
			});
			Ok(Response::new(StatusCode::OK)
				.with_header("Content-Type", "application/json")
				.with_body(json::to_vec(&resp)?))
		}
		Err(EmailVerificationError::App(e)) => Err(e),
	}
}

/// Distinguishes "could not even create the verification token" from
/// "token created but mail send failed". The former propagates as a real
/// error (DB problems, RNG failure) and aborts the whole request; the
/// latter degrades to a partial-success response per the contract above.
enum EmailVerificationError {
	App(AppError),
	Send(MailerError),
}

impl From<AppError> for EmailVerificationError {
	fn from(value: AppError) -> Self {
		Self::App(value)
	}
}

/// Generate and dispatch an email-verification token for a pending email
/// change. Invalidates any prior unconsumed tokens for the user before
/// inserting the new row.
async fn issue_email_verification(
	user: &User,
	pending_email: &str,
) -> Result<(), EmailVerificationError> {
	// Invalidate existing unconsumed tokens for this user by deleting them.
	// The query filters consumed_at IS NULL at the DB layer so it only
	// touches rows that are still pending — consumed rows are left intact for
	// audit purposes. Filtering at the DB layer also allows the partial index
	// on (user_id) WHERE consumed_at IS NULL (from migration 0004) to be used,
	// avoiding a full table scan as the token table grows.
	let prior = EmailVerificationToken::objects()
		.filter(
			EmailVerificationToken::field_user_id(),
			FilterOperator::Eq,
			FilterValue::String(user.id.to_string()),
		)
		.filter(Filter::new(
			EmailVerificationToken::field_consumed_at(),
			FilterOperator::IsNull,
			FilterValue::Null,
		))
		.all()
		.await
		.map_err(|e| {
			error!("Failed to list existing verification tokens: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;
	// Delete all unconsumed prior tokens. The loop iterates every row returned
	// (no early exit) so that all stale tokens are always cleaned up regardless
	// of delete errors on earlier rows.
	for t in prior {
		if let Some(id) = t.id
			&& let Err(e) = EmailVerificationToken::objects().delete(id).await
		{
			error!("Failed to delete stale verification token {id}: {e}");
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

	// PUBLIC_BASE_URL must be set to a non-empty absolute origin. An empty
	// value would silently emit a relative path, which produces broken
	// verification links once delivered (clients have no base to resolve
	// against). Treat that as a configuration error so it surfaces in logs
	// rather than as silent failure in user inboxes.
	let base_url = std::env::var("PUBLIC_BASE_URL")
		.ok()
		.map(|v| v.trim().trim_end_matches('/').to_string())
		.filter(|v| !v.is_empty())
		.ok_or_else(|| {
			error!("PUBLIC_BASE_URL is not configured; cannot build verification link");
			AppError::Internal("Server is missing PUBLIC_BASE_URL configuration".to_string())
		})?;
	// Token is delivered in the URL fragment (`#...`). Fragments are NOT
	// transmitted to the server, so they do not appear in access logs,
	// proxies, browser history, or `Referer` headers, mitigating the
	// classic "token in query string" leak. The dashboard's
	// `/auth/confirm-email` page is responsible for parsing the fragment in
	// JavaScript and POSTing it to the `/api/auth/verify-email-change/`
	// endpoint.
	let url = format!(
		"{base_url}/auth/confirm-email#user_id={}&token={}",
		user.id, token_plain
	);
	let body = format!(
		"A request was made to change your Reinhardt Cloud account email to \
		 {pending_email}.\n\nTo confirm this change, visit:\n\n{url}\n\n\
		 If you did not request this change, you can safely ignore this email; \
		 your account is unaffected."
	);

	let sender = mailer::default_sender();
	sender
		.send(pending_email, "Confirm your email change", &body)
		.await
		.map_err(EmailVerificationError::Send)?;
	Ok(())
}
