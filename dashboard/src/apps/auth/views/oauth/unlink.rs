//! `POST /oauth/{provider}/unlink/` — remove the link to a social provider.
//!
//! Authentication is required (caller must already be logged in via session
//! cookie or JWT). Lockout protection: if the user has no usable password
//! AND this is their last social link, the request is rejected with 422 so
//! the user cannot accidentally make their account unreachable.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::db::orm::{FilterOperator, FilterValue, Model};
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, StatusCode, post};
use reinhardt_auth::social::storage::SocialAccountStorage;
use tracing::error;
use uuid::Uuid;

use crate::apps::auth::models::User;
use crate::apps::auth::services::oauth::storage::OrmSocialAccountStorage;

#[post("/oauth/{provider}/unlink/", name = "oauth_unlink")]
pub async fn oauth_unlink(
	Path(provider): Path<String>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("invalid user_id in token: {e}")))?;

	let user = User::objects()
		.filter(
			User::field_id(),
			FilterOperator::Eq,
			FilterValue::String(user_id.to_string()),
		)
		.first()
		.await
		.map_err(|e| {
			error!("user lookup failed during unlink: {e}");
			AppError::Internal("user lookup failed".to_string())
		})?
		.ok_or_else(|| AppError::NotFound("user not found".to_string()))?;

	let storage = OrmSocialAccountStorage::new();
	let links = storage.find_by_user(user_id).await.map_err(|e| {
		error!("storage list failed during unlink: {e}");
		AppError::Internal("storage list failed".to_string())
	})?;

	let target = links
		.iter()
		.find(|l| l.provider == provider)
		.ok_or_else(|| AppError::NotFound(format!("no link to provider: {provider}")))?;

	// Lockout protection: if the user has no usable password AND this is
	// their only social link, deleting it would lock them out of the
	// account. Refuse with a 422 so the client can prompt them to set a
	// password first.
	if user.password_hash.is_none() && links.len() == 1 {
		return Err(AppError::Validation(
			"cannot unlink the last sign-in method; set a password before unlinking"
				.to_string(),
		));
	}

	storage.delete(target.id).await.map_err(|e| {
		error!("storage delete failed during unlink: {e}");
		AppError::Internal("storage delete failed".to_string())
	})?;

	let body = json::to_vec(&serde_json::json!({"success": true, "provider": provider}))?;
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(body))
}
