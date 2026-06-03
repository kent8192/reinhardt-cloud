//! Deprecation redirect patterns for the flat-URL API endpoints.
//!
//! The flat-URL endpoints `/api/clusters/` and `/api/deployments/` were
//! replaced by org-scoped endpoints `/api/orgs/{org}/clusters/` and
//! `/api/orgs/{org}/deployments/` in issue #418.
//!
//! These redirect handlers issue 307 Temporary Redirects to the caller's
//! Personal Organization (the first org by `created_at`), preserving the
//! original HTTP method. They will be removed after the next release window.
//!
//! # CHANGELOG note
//! Added in this release as a compatibility shim. **Will be removed in the
//! next release.** Clients should migrate to the `/api/orgs/{org}/...` form.

use reinhardt::ServerRouter;
use reinhardt::core::exception::Error as AppError;
use reinhardt::http::ViewResult;
use reinhardt::{AuthInfo, Path, Response, delete, get, patch, post, put};
use uuid::Uuid;

use crate::apps::organizations::helpers::current_organization_id_for_user;
use crate::apps::organizations::models::Organization;

/// Resolve the personal org slug for a user, used by the redirect handlers.
async fn personal_org_slug(user_id: Uuid) -> Result<String, AppError> {
	use reinhardt::Model;

	let org_id = current_organization_id_for_user(user_id).await?;
	let org = Organization::objects()
		.filter(Organization::field_id().eq(org_id))
		.first()
		.await
		.map_err(|e| AppError::Internal(format!("org lookup failed: {e}")))?
		.ok_or_else(|| AppError::Internal("personal org not found".to_string()))?;
	Ok(org.slug)
}

// ── Clusters redirect handlers ───────────────────────────────────────────────

#[get("/", name = "clusters-list-redirect")]
async fn clusters_list_redirect(#[inject] AuthInfo(state): AuthInfo) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/clusters/"
	)))
}

#[post("/", name = "clusters-create-redirect")]
async fn clusters_create_redirect(#[inject] AuthInfo(state): AuthInfo) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/clusters/"
	)))
}

#[get("/{id}/", name = "clusters-retrieve-redirect")]
async fn clusters_retrieve_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/clusters/{id}/"
	)))
}

#[patch("/{id}/", name = "clusters-update-redirect")]
async fn clusters_update_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/clusters/{id}/"
	)))
}

#[delete("/{id}/", name = "clusters-delete-redirect")]
async fn clusters_delete_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/clusters/{id}/"
	)))
}

#[post("/{id}/rotate-token/", name = "clusters-rotate-token-redirect")]
async fn clusters_rotate_token_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/clusters/{id}/rotate-token/"
	)))
}

// ── Deployments redirect handlers ────────────────────────────────────────────

#[get("/", name = "deployments-list-redirect")]
async fn deployments_list_redirect(#[inject] AuthInfo(state): AuthInfo) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/"
	)))
}

#[post("/", name = "deployments-create-redirect")]
async fn deployments_create_redirect(#[inject] AuthInfo(state): AuthInfo) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/"
	)))
}

#[get("/{id}/", name = "deployments-retrieve-redirect")]
async fn deployments_retrieve_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/{id}/"
	)))
}

#[put("/{id}/", name = "deployments-update-redirect")]
async fn deployments_update_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/{id}/"
	)))
}

#[delete("/{id}/", name = "deployments-delete-redirect")]
async fn deployments_delete_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/{id}/"
	)))
}

#[post("/{id}/status/", name = "deployments-status-redirect")]
async fn deployments_status_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/{id}/status/"
	)))
}

#[get("/{id}/logs/", name = "deployments-logs-redirect")]
async fn deployments_logs_redirect(
	Path(id): Path<i64>,
	#[inject] AuthInfo(state): AuthInfo,
) -> ViewResult<Response> {
	let user_id = Uuid::parse_str(state.user_id())
		.map_err(|e| AppError::Authentication(format!("Invalid user ID: {e}")))?;
	let slug = personal_org_slug(user_id).await?;
	Ok(Response::temporary_redirect_preserve_method(format!(
		"/api/orgs/{slug}/deployments/{id}/logs/"
	)))
}

// ── URL pattern registrations ────────────────────────────────────────────────

/// Returns 307 redirect URL patterns for the deprecated `/api/clusters/` prefix.
pub fn clusters_redirect_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(clusters_list_redirect)
		.endpoint(clusters_create_redirect)
		.endpoint(clusters_retrieve_redirect)
		.endpoint(clusters_update_redirect)
		.endpoint(clusters_delete_redirect)
		.endpoint(clusters_rotate_token_redirect)
}

/// Returns 307 redirect URL patterns for the deprecated `/api/deployments/` prefix.
pub fn deployments_redirect_patterns() -> ServerRouter {
	ServerRouter::new()
		.endpoint(deployments_list_redirect)
		.endpoint(deployments_create_redirect)
		.endpoint(deployments_retrieve_redirect)
		.endpoint(deployments_update_redirect)
		.endpoint(deployments_delete_redirect)
		.endpoint(deployments_status_redirect)
		.endpoint(deployments_logs_redirect)
}
