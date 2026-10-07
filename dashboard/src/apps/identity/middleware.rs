//! Browser session resolution and signed, origin-checked CSRF protection.

use std::sync::Arc;

use hmac::{Hmac, Mac};
use reinhardt::{Handler, Middleware, Request, Response};
use serde::Deserialize;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use super::persistence::{new_token, session_actor};
use crate::config::settings::get_scoped_settings;

#[derive(Clone)]
pub struct CsrfToken(pub String);

pub struct BrowserSettings {
	pub secure: bool,
	pub public_origin: String,
	secret: String,
}

impl BrowserSettings {
	pub fn load() -> Result<Self, &'static str> {
		let scoped = get_scoped_settings().map_err(|_| "Cannot load browser settings")?;
		let secure = scoped
			.require_path(&["core", "security", "session_cookie_secure"])
			.map_err(|_| "Cannot load browser cookie policy")?;
		let public_origin: String = scoped
			.require_path(&["identity", "public_origin"])
			.map_err(|_| "Cannot load the public browser origin")?;
		let secret: String = scoped
			.require_path(&["core", "secret_key"])
			.map_err(|_| "Cannot load the browser signing key")?;
		let origin: http::Uri = public_origin
			.parse()
			.map_err(|_| "Invalid browser origin")?;
		let scheme = origin
			.scheme_str()
			.ok_or("A browser origin requires a scheme")?;
		let authority = origin
			.authority()
			.ok_or("A browser origin requires a host")?;
		if secret.len() < 32
			|| !matches!(scheme, "http" | "https")
			|| authority.as_str().contains('@')
			|| public_origin != format!("{scheme}://{authority}")
			|| (secure && scheme != "https")
		{
			return Err("Invalid browser origin or signing key");
		}
		Ok(Self {
			secure,
			public_origin,
			secret,
		})
	}

	pub fn session_cookie_name(&self) -> &'static str {
		if self.secure {
			"__Host-cloud_session"
		} else {
			"cloud_session"
		}
	}

	fn csrf_cookie_name(&self) -> &'static str {
		if self.secure {
			"__Host-cloud_csrf"
		} else {
			"cloud_csrf"
		}
	}

	pub fn cookie(&self, name: &str, value: &str, max_age: u32) -> String {
		format!(
			"{name}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Strict{}",
			if self.secure { "; Secure" } else { "" }
		)
	}

	fn issue_csrf(&self) -> String {
		let payload = format!("{}.{}", chrono::Utc::now().timestamp(), new_token());
		let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
			.expect("HMAC accepts any key length");
		mac.update(payload.as_bytes());
		format!("{payload}.{}", hex::encode(mac.finalize().into_bytes()))
	}

	fn valid_csrf(&self, token: &str) -> bool {
		if token.len() > 160 {
			return false;
		}
		let Some((payload, signature)) = token.rsplit_once('.') else {
			return false;
		};
		let Some((issued, nonce)) = payload.split_once('.') else {
			return false;
		};
		let Ok(issued) = issued.parse::<i64>() else {
			return false;
		};
		let age = chrono::Utc::now().timestamp().saturating_sub(issued);
		if !(0..7200).contains(&age) || nonce.len() != 64 {
			return false;
		}
		let Ok(signature) = hex::decode(signature) else {
			return false;
		};
		let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
			.expect("HMAC accepts any key length");
		mac.update(payload.as_bytes());
		mac.verify_slice(&signature).is_ok()
	}
}

pub fn request_cookie(request: &Request, name: &str) -> Option<String> {
	request
		.headers
		.get("Cookie")
		.and_then(|value| value.to_str().ok())?
		.split(';')
		.find_map(|pair| {
			let (key, value) = pair.trim().split_once('=')?;
			(key == name).then(|| value.to_owned())
		})
}

pub struct IdentityMiddleware;

#[reinhardt::core::async_trait]
impl Middleware for IdentityMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::http::Result<Response> {
		if request.uri.path() == "/healthz/" || request.uri.path().starts_with("/static/") {
			return next.handle(request).await;
		}
		let Ok(settings) = BrowserSettings::load() else {
			return Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)
				.with_body("Browser configuration is unavailable."));
		};
		let cookie_token = request_cookie(&request, settings.csrf_cookie_name());
		let safe = matches!(
			request.method,
			http::Method::GET | http::Method::HEAD | http::Method::OPTIONS
		);
		if !safe {
			#[derive(Deserialize)]
			struct SubmittedCsrf {
				csrf_token: String,
			}
			let submitted = request
				.headers
				.get("X-CSRFToken")
				.or_else(|| request.headers.get("X-CSRF-Token"))
				.and_then(|value| value.to_str().ok())
				.map(str::to_owned)
				.or_else(|| {
					(request.body().len() <= 65536)
						.then(|| serde_urlencoded::from_bytes::<SubmittedCsrf>(request.body()).ok())
						.flatten()
						.map(|form| form.csrf_token)
				});
			let valid = request
				.headers
				.get("Origin")
				.and_then(|value| value.to_str().ok())
				== Some(settings.public_origin.as_str())
				&& cookie_token.as_ref().zip(submitted.as_ref()).is_some_and(
					|(cookie, submitted)| {
						settings.valid_csrf(cookie)
							&& bool::from(cookie.as_bytes().ct_eq(submitted.as_bytes()))
					},
				);
			if !valid {
				return Ok(Response::new(http::StatusCode::FORBIDDEN)
					.with_header("Cache-Control", "no-store")
					.with_body(
						"The form expired or its origin could not be verified. Reload the page and try again.",
					));
			}
		}
		if let Some(token) = request_cookie(&request, settings.session_cookie_name()) {
			match session_actor(&token).await {
				Ok(Some(actor)) => request.extensions.insert(actor),
				Ok(None) => {}
				Err(_) => {
					return Ok(Response::new(http::StatusCode::SERVICE_UNAVAILABLE)
						.with_header("Cache-Control", "no-store")
						.with_body("Identity storage is unavailable. Try again later."));
				}
			}
		}
		let fresh = !cookie_token
			.as_ref()
			.is_some_and(|token| settings.valid_csrf(token));
		let csrf = if fresh {
			settings.issue_csrf()
		} else {
			cookie_token.expect("a valid cookie is present")
		};
		request.extensions.insert(CsrfToken(csrf.clone()));
		let response = next
			.handle(request)
			.await?
			.with_header("Cache-Control", "no-store");
		Ok(if fresh {
			response.append_header(
				"Set-Cookie",
				&settings.cookie(settings.csrf_cookie_name(), &csrf, 7200),
			)
		} else {
			response
		})
	}
}
