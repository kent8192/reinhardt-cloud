//! Login view for auth app.

use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{JwtAuth, Json, Response, StatusCode};

use crate::apps::auth::serializers::{LoginRequest, TokenResponse};

/// Authenticate user and return JWT token.
#[post("/auth/login/", name = "auth_login")]
pub async fn login(Json(body): Json<LoginRequest>) -> ViewResult<Response> {
	let secret = std::env::var("NUAGES_JWT_SECRET")
		.unwrap_or_else(|_| "change-me-in-production-minimum-32-bytes!".to_string());
	let auth = JwtAuth::new(secret.as_bytes());
	let token = auth
		.generate_token(body.username.clone(), body.username)
		.map_err(|e| format!("Token generation failed: {e}"))?;
	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::OK)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
