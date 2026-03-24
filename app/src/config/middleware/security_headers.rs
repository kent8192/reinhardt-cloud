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
///   (skipped for routes that set their own CSP, e.g. admin panel)
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
		let response = next.handle(request).await?;

		// Use with_header_if_absent for CSP so that handler-set CSP headers
		// (e.g. from admin_routes()) are preserved instead of overwritten.
		Ok(response
			.with_header("X-Content-Type-Options", "nosniff")
			.with_header("X-Frame-Options", "DENY")
			.with_header("X-XSS-Protection", "0")
			.with_header(
				"Strict-Transport-Security",
				"max-age=63072000; includeSubDomains",
			)
			.with_header_if_absent("Content-Security-Policy", "default-src 'none'")
			.with_header("Cache-Control", "no-store")
			.with_header("Referrer-Policy", "no-referrer"))
	}
}
