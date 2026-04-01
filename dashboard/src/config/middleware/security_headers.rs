//! Security headers middleware for Reinhardt Cloud.
//!
//! Adds recommended security headers to all HTTP responses to mitigate
//! common web vulnerabilities (clickjacking, MIME sniffing, XSS, etc.).

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::{Handler, Middleware, Request, Response};

/// Middleware that adds security headers to all responses.
///
/// Applied headers:
/// - `X-Content-Type-Options: nosniff` — prevents MIME-type sniffing
/// - `X-Frame-Options: DENY` — prevents clickjacking via iframes
/// - `X-XSS-Protection: 0` — disables legacy XSS filter (modern CSP preferred)
/// - `Strict-Transport-Security` — enforces HTTPS connections
/// - `Content-Security-Policy: default-src 'none'` — restrictive CSP for API
/// - `Cache-Control: no-store` — prevents caching of sensitive responses
/// - `Referrer-Policy: no-referrer` — prevents referrer leakage
pub struct SecurityHeadersMiddleware;

#[async_trait]
impl Middleware for SecurityHeadersMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		// Detect HTTPS via X-Forwarded-Proto (set by reverse proxy / LB)
		let is_https = request
			.headers
			.get("X-Forwarded-Proto")
			.and_then(|v| v.to_str().ok())
			.is_some_and(|proto| proto == "https");

		let response = next.handle(request).await?;

		let mut response = response
			.with_header("X-Content-Type-Options", "nosniff")
			.with_header("X-Frame-Options", "DENY")
			.with_header("X-XSS-Protection", "0")
			.with_header("Content-Security-Policy", "default-src 'none'")
			.with_header("Cache-Control", "no-store")
			.with_header("Referrer-Policy", "no-referrer");

		// HSTS only applies over HTTPS; sending it over plain HTTP is
		// ignored by browsers and confusing in development environments.
		if is_https {
			response = response.with_header(
				"Strict-Transport-Security",
				"max-age=63072000; includeSubDomains",
			);
		}

		Ok(response)
	}
}
