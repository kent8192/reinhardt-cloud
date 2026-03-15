//! Register view for auth app.

use reinhardt::core::serde::json;
use reinhardt::http::ViewResult;
use reinhardt::post;
use reinhardt::{JwtAuth, Json, Response, StatusCode};

use crate::apps::auth::serializers::{RegisterRequest, TokenResponse};

/// Register new user and return JWT token.
#[post("/auth/register/", name = "auth_register", pre_validate = true)]
pub async fn register(body: Json<RegisterRequest>) -> ViewResult<Response> {
	let secret = std::env::var("NUAGES_JWT_SECRET")
		.unwrap_or_else(|_| "change-me-in-production-minimum-32-bytes!".to_string());
	let auth = JwtAuth::new(secret.as_bytes());
	let token = auth
		.generate_token(body.username.clone(), body.username.clone())
		.map_err(|e| format!("Token generation failed: {e}"))?;
	let resp = TokenResponse::bearer(token);
	Ok(Response::new(StatusCode::CREATED)
		.with_header("Content-Type", "application/json")
		.with_body(json::to_vec(&resp)?))
}
