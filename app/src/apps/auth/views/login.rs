//! Login view for auth app.

use reinhardt::core::exception::Error as AppError;
use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{Json, JwtAuth, Response, StatusCode};
use tracing::error;

use super::utils::jwt_secret;
use crate::apps::auth::serializers::{LoginRequest, TokenResponse};
use crate::apps::auth::services;

/// Authenticate user against database and return JWT token.
#[post("/auth/login/", name = "auth_login", pre_validate = true)]
pub async fn login(body: Json<LoginRequest>) -> ViewResult<Response> {
	// Delegate credential verification to shared service
	let user = services::verify_credentials(&body.username, &body.password).await?;

	// Generate JWT with UUID as sub claim (REST API specific)
	let auth = JwtAuth::new(jwt_secret().expect("REINHARDT_CLOUD_JWT_SECRET must be set").as_bytes());
	let token = auth
		.generate_token(user.id().to_string(), user.username().to_string())
		.map_err(|e| {
			error!("JWT token generation failed during login: {e}");
			AppError::Internal("Internal server error".to_string())
		})?;

	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
