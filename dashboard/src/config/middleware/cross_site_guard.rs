//! Cross-site request protection for cookie-authenticated requests (SR-12).
//!
//! A browser attaches the session cookie to any request, including one a
//! hostile page makes. A state-changing request that carries the session cookie
//! is therefore accepted only when it proves same-origin intent: its `Origin`
//! header (or, when absent, the origin of its `Referer`) must be on the
//! explicit allow-list. `SameSite=Lax` on the cookie is the first line; this is
//! the second.
//!
//! Not subject to the check:
//!
//! - safe methods (`GET`, `HEAD`, `OPTIONS`), which must not change state;
//! - requests without the session cookie: there is no ambient credential to
//!   abuse. The exception is a path that *starts* a session
//!   (`SESSION_ESTABLISHING_PATHS`, for example confirming a Login Link): there
//!   the request is the thing a hostile page would forge, so it always has to
//!   prove its origin.
//!
//! An `Authorization` header does not exempt a request that also carries the
//! cookie. Bearer CLI Sessions (M2) will be authenticated by the header alone;
//! the exemption belongs with that credential, not before it exists.

use std::sync::Arc;

use async_trait::async_trait;
use reinhardt::core::exception::Result;
use reinhardt::{Handler, Middleware, Request, Response};

use crate::apps::accounts::server::cookies::{SESSION_COOKIE, request_cookie};
use crate::apps::accounts::urls::server_router::SESSION_ESTABLISHING_PATHS;

/// Rejects cross-site state-changing requests that carry a session cookie.
#[derive(Clone, Debug)]
pub struct CrossSiteGuard {
	allowed_origins: Vec<String>,
}

impl CrossSiteGuard {
	/// Create the guard. `allowed_origins` are `scheme://host[:port]` strings;
	/// the settings layer has already dropped wildcards and malformed entries.
	#[must_use]
	pub fn new(allowed_origins: Vec<String>) -> Self {
		Self { allowed_origins }
	}

	fn requires_proof(request: &Request) -> bool {
		let safe = matches!(request.method.as_str(), "GET" | "HEAD" | "OPTIONS");
		!safe
			&& (request_cookie(request, SESSION_COOKIE).is_some()
				|| SESSION_ESTABLISHING_PATHS.contains(&request.uri.path()))
	}

	fn presented_origin(request: &Request) -> Option<String> {
		let header = |name: &str| {
			request
				.headers
				.get(name)
				.and_then(|value| value.to_str().ok())
		};
		match header("Origin") {
			Some(origin) => Some(origin.to_owned()),
			None => header("Referer").and_then(origin_of),
		}
	}
}

/// `scheme://authority` of `url`, or `None` when it has no authority.
fn origin_of(url: &str) -> Option<String> {
	let (scheme, rest) = url.split_once("://")?;
	let authority = rest.split(['/', '?', '#']).next()?;
	(!authority.is_empty()).then(|| format!("{scheme}://{authority}"))
}

#[async_trait]
impl Middleware for CrossSiteGuard {
	async fn process(&self, request: Request, next: Arc<dyn Handler>) -> Result<Response> {
		if Self::requires_proof(&request) {
			let allowed = Self::presented_origin(&request)
				.is_some_and(|origin| self.allowed_origins.contains(&origin));
			if !allowed {
				return Ok(Response::forbidden()
					.with_header("Content-Type", "application/json")
					.with_body(r#"{"error":"cross-site request rejected"}"#));
			}
		}
		next.handle(request).await
	}
}
