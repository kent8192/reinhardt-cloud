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
/// - `Content-Security-Policy` — restrictive for API (`default-src 'none'`),
///   moderate for page routes. Admin routes manage their own CSP via
///   `AdminSettings` (reinhardt-web), so this middleware defers with
///   `with_header_if_absent`.
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
		let path = request.uri.path().to_string();
		let is_api = path.starts_with("/api/");
		let response = next.handle(request).await?;

		// API routes get a restrictive CSP; page routes allow WASM and
		// inline styles. Admin routes manage their own CSP via AdminSettings
		// (reinhardt-web), so with_header_if_absent defers to the built-in
		// admin CSP when present.
		let csp = if is_api {
			"default-src 'none'"
		} else {
			"default-src 'self'; \
			 script-src 'self' 'wasm-unsafe-eval'; \
			 style-src 'self' 'unsafe-inline'; \
			 connect-src 'self' wss: ws:; \
			 img-src 'self' data:"
		};

		let response = response.with_header_if_absent("Content-Security-Policy", csp);

		Ok(response
			.with_header("X-Content-Type-Options", "nosniff")
			.with_header("X-Frame-Options", "DENY")
			.with_header("X-XSS-Protection", "0")
			.with_header(
				"Strict-Transport-Security",
				"max-age=63072000; includeSubDomains",
			)
			.with_header("Cache-Control", "no-store")
			.with_header("Referrer-Policy", "no-referrer"))
	}
}
