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
		let is_admin = request.uri.path().starts_with("/admin/")
			|| request.uri.path().starts_with("/static/admin/");
		let response = next.handle(request).await?;

		// Admin SPA needs script/style/connect-src permissions.
		// API endpoints use restrictive default-src 'none'.
		let csp = if is_admin {
			"default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; font-src 'self'"
		} else {
			"default-src 'none'"
		};

		let response = response
			.with_header("X-Content-Type-Options", "nosniff")
			.with_header("X-Frame-Options", "DENY")
			.with_header("X-XSS-Protection", "0")
			.with_header(
				"Strict-Transport-Security",
				"max-age=63072000; includeSubDomains",
			)
			.with_header("Content-Security-Policy", csp)
			.with_header("Cache-Control", "no-store")
			.with_header("Referrer-Policy", "no-referrer")
			.with_header(
				"Permissions-Policy",
				"camera=(), microphone=(), geolocation=(), payment=()",
			);

		Ok(response)
	}
}
