//! Security headers middleware for Reinhardt Cloud.
//!
//! Adds recommended security headers to all HTTP responses to mitigate
//! common web vulnerabilities (clickjacking, MIME sniffing, XSS, etc.).
//!
//! Admin panel routes (`/admin/`, `/static/admin/`) are excluded from
//! the restrictive API CSP because `admin_routes()` sets its own
//! Content-Security-Policy headers appropriate for the admin SPA.

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
///   (skipped for admin routes where `admin_routes()` sets its own CSP)
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

		let response = response
			.with_header("X-Content-Type-Options", "nosniff")
			.with_header("X-Frame-Options", "DENY")
			.with_header("X-XSS-Protection", "0")
			.with_header(
				"Strict-Transport-Security",
				"max-age=63072000; includeSubDomains",
			)
			.with_header("Cache-Control", "no-store")
			.with_header("Referrer-Policy", "no-referrer");

		// Workaround for kent8192/reinhardt-web#2862 (tracked in reinhardt-cloud#118)
		// Remove this workaround when the upstream issue is resolved.
		//
		// Ideal implementation (without workaround):
		//   response.with_header("Content-Security-Policy", "default-src 'none'")
		//   // admin_routes() CSP survives middleware processing because the
		//   // framework preserves handler-set headers instead of overwriting them.
		let response = if is_admin {
			response
		} else {
			response.with_header("Content-Security-Policy", "default-src 'none'")
		};

		Ok(response)
	}
}
