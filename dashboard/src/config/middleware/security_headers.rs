//! Path-based CSP middleware for Reinhardt Cloud.
//!
//! Adds a `Content-Security-Policy` header with different policies for
//! API routes (restrictive) and page routes (WASM-aware).
//! Admin routes manage their own CSP via `AdminSettings`, so this
//! middleware defers with `with_header_if_absent`.
//!
//! General security headers are handled by the built-in
//! `SecurityMiddleware` (reinhardt-web).

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::{Handler, Middleware, Request, Response};

/// Middleware that adds a path-based Content-Security-Policy header.
pub struct CspPathMiddleware;

#[async_trait]
impl Middleware for CspPathMiddleware {
	async fn process(
		&self,
		request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		let path = request.uri.path();
		let is_api = path.starts_with("/api/");
		let is_admin = path.starts_with("/admin/");
		let response = next.handle(request).await?;

		if is_admin {
			// Admin SPA uses an inline <script type="module"> to boot WASM.
			// Override SecurityMiddleware's CSP to allow 'unsafe-inline'.
			let csp = "default-src 'self'; \
				 script-src 'self' 'unsafe-inline' 'wasm-unsafe-eval'; \
				 style-src 'self' 'unsafe-inline'; \
				 connect-src 'self'; \
				 img-src 'self' data:; \
				 font-src 'self'";
			return Ok(response.with_header("Content-Security-Policy", csp));
		}

		let csp = if is_api {
			"default-src 'none'"
		} else {
			"default-src 'self'; \
			 script-src 'self' 'wasm-unsafe-eval'; \
			 style-src 'self' 'unsafe-inline'; \
			 connect-src 'self' wss: ws:; \
			 img-src 'self' data:"
		};

		Ok(response.with_header_if_absent("Content-Security-Policy", csp))
	}
}
