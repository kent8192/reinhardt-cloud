//! Password sign-in and session revocation for native Pages forms.

use reinhardt::{Request, Response, post};
use serde::Deserialize;

use super::middleware::{BrowserSettings, request_cookie};
use super::persistence::{IdentityError, revoke_session, sign_in};

#[derive(Deserialize)]
struct Credentials {
	email: String,
	password: String,
}

#[post("/login/", name = "sign-in")]
pub async fn login(request: Request) -> reinhardt::http::Result<Response> {
	if request.body().len() > 65536 {
		return Ok(Response::new(http::StatusCode::PAYLOAD_TOO_LARGE));
	}
	let Ok(credentials) = serde_urlencoded::from_bytes::<Credentials>(request.body()) else {
		return Ok(Response::new(http::StatusCode::BAD_REQUEST));
	};
	match sign_in(&credentials.email, credentials.password).await {
		Ok(token) => {
			let Ok(settings) = BrowserSettings::load() else {
				return Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE));
			};
			// Rotating the opaque token prevents a pre-login session from being adopted.
			if let Some(previous) = request_cookie(&request, settings.session_cookie_name()) {
				revoke_session(&previous).await.map_err(|_| {
					reinhardt::core::exception::Error::Internal("Session rotation failed".into())
				})?;
			}
			Ok(Response::new(http::StatusCode::SEE_OTHER)
				.with_header("Location", "/organizations/")
				.with_header("Cache-Control", "no-store")
				.append_header(
					"Set-Cookie",
					&settings.cookie(settings.session_cookie_name(), &token, 86400),
				))
		}
		Err(IdentityError::InvalidCredentials) => Ok(Response::new(http::StatusCode::SEE_OTHER)
			.with_header("Location", "/login/?signin=failed")
			.with_header("Cache-Control", "no-store")),
		Err(_) => Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)
			.with_body("Sign-in is unavailable. Try again later.")),
	}
}

#[post("/logout/", name = "sign-out")]
pub async fn logout(request: Request) -> reinhardt::http::Result<Response> {
	let Ok(settings) = BrowserSettings::load() else {
		return Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE));
	};
	if let Some(token) = request_cookie(&request, settings.session_cookie_name()) {
		revoke_session(&token).await.map_err(|_| {
			reinhardt::core::exception::Error::Internal("Session revocation failed".into())
		})?;
	}
	Ok(Response::new(http::StatusCode::SEE_OTHER)
		.with_header("Location", "/login/")
		.with_header("Cache-Control", "no-store")
		.append_header(
			"Set-Cookie",
			&settings.cookie(settings.session_cookie_name(), "", 0),
		))
}

#[reinhardt::get("/login/", name = "sign-in-page")]
pub async fn sign_in_page(request: Request) -> reinhardt::http::Result<Response> {
	if request
		.extensions
		.get::<super::persistence::Actor>()
		.is_some()
	{
		return Ok(
			Response::new(http::StatusCode::SEE_OTHER).with_header("Location", "/organizations/")
		);
	}
	#[derive(Deserialize)]
	struct Query {
		signin: Option<String>,
	}
	let failed = request
		.query_as::<Query>()
		.ok()
		.is_some_and(|q| q.signin.as_deref() == Some("failed"));
	crate::client::document::respond(
		request,
		if failed {
			crate::client::screens::DashboardState::SignInFailed
		} else {
			crate::client::screens::DashboardState::AuthenticationRequired
		},
	)
	.await
}
