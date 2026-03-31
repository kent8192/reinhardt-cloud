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
///   permissive for admin (CDN fonts/scripts/styles), moderate for pages.
///   Admin routes override reinhardt-web's built-in CSP which blocks its
///   own CDN resources; other routes use if-absent to preserve handler CSP.
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
		let is_admin = path.starts_with("/admin/");
		let response = next.handle(request).await?;

		// Use with_header_if_absent for CSP so that handler-set CSP headers
		// (e.g. from admin_routes()) are preserved instead of overwritten.
		// API routes get a restrictive CSP; admin routes need CDN access for
		// fonts, stylesheets, UnoCSS runtime, and WASM execution; page routes
		// allow WASM, scripts, and the UnoCSS CDN runtime.
		let csp = if is_api {
			"default-src 'none'"
		} else if is_admin {
			"default-src 'self'; \
			 script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval' https://cdn.jsdelivr.net; \
			 style-src 'self' 'unsafe-inline' https://fonts.googleapis.com https://cdn.jsdelivr.net; \
			 font-src 'self' https://fonts.gstatic.com; \
			 connect-src 'self' wss: ws:; \
			 img-src 'self' data:"
		} else {
			"default-src 'self'; \
			 script-src 'self' 'wasm-unsafe-eval' https://cdn.jsdelivr.net; \
			 style-src 'self' 'unsafe-inline'; \
			 connect-src 'self' wss: ws:; \
			 img-src 'self' data:"
		};

		// Workaround: Override reinhardt-web's built-in admin CSP which is too
		// restrictive for the CDN resources its own HTML template loads
		// (Google Fonts, jsdelivr CSS/JS). Remove when reinhardt-web#3207 is
		// fixed. See reinhardt-cloud#223.
		let response = if is_admin {
			response.with_header("Content-Security-Policy", csp)
		} else {
			response.with_header_if_absent("Content-Security-Policy", csp)
		};

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
