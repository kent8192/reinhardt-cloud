//! Security response headers (SR-13).
//!
//! - **API responses** (`/api/`) carry a content security policy that allows
//!   nothing, plus `Cache-Control: no-store`: an API answer is data for one
//!   caller.
//! - **Page responses** carry a policy that restricts scripts to the
//!   application's own origin plus WebAssembly evaluation.
//! - **API documentation** (`/api/docs`, `/api/redoc`) exists only in the
//!   `local` and `ci` profiles (SR-11) and loads its viewer from a CDN, which a
//!   policy that allows nothing would blank; it gets no policy from here.
//! - **The admin site** sends its own, documented policy (inline module
//!   script, inline styles); this middleware leaves it in place.
//! - **Transport security** (`Strict-Transport-Security`) is added by the
//!   framework's `SecurityMiddleware` only when the request is known to have
//!   arrived over HTTPS. The framework believes `X-Forwarded-Proto` only from
//!   a proxy declared in the request's `TrustedProxies`, which `ProxyTrust`
//!   fills in from `REINHARDT_CLOUD_TRUSTED_PROXIES`. HTTPS redirection is the proxy's job, so it is
//!   switched off here: behind a proxy that has not been declared trusted,
//!   every request would look like plain HTTP and loop.
//!
//! The headers are added to errors too, so a failing handler cannot produce a
//! response without them.

use std::sync::Arc;

use async_trait::async_trait;
use reinhardt::conf::SecuritySettings;
use reinhardt::core::exception::Result;
use reinhardt::{Handler, Middleware, Request, Response, SecurityMiddleware};

/// Content security policy of API responses: nothing may load or run.
pub const API_CSP: &str = "default-src 'none'; frame-ancestors 'none'";

/// Content security policy of page responses.
pub const PAGE_CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; \
	style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; \
	font-src 'self'; base-uri 'self'; form-action 'self'; frame-ancestors 'none'";

/// Which policy a path gets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Surface {
	Api,
	Documentation,
	Admin,
	Page,
}

fn surface_of(path: &str) -> Surface {
	if path == "/admin" || path.starts_with("/admin/") || path.starts_with("/static/admin/") {
		Surface::Admin
	} else if matches!(path, "/api/docs" | "/api/redoc") {
		Surface::Documentation
	} else if path.starts_with("/api/") {
		Surface::Api
	} else {
		Surface::Page
	}
}

/// Adds the content security policy for the response's surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct ContentSecurityPolicy;

#[async_trait]
impl Middleware for ContentSecurityPolicy {
	async fn process(&self, request: Request, next: Arc<dyn Handler>) -> Result<Response> {
		let surface = surface_of(request.uri.path());
		// Convert errors first so they carry the headers as well.
		let response = match next.handle(request).await {
			Ok(response) => response,
			Err(error) => Response::from(error),
		};
		Ok(match surface {
			Surface::Api => response
				.with_header("Content-Security-Policy", API_CSP)
				.with_header_if_absent("Cache-Control", "no-store"),
			Surface::Documentation | Surface::Admin => response,
			Surface::Page => response.with_header_if_absent("Content-Security-Policy", PAGE_CSP),
		})
	}
}

/// The framework's header middleware, configured from the profile's security
/// settings but never redirecting (see the module documentation).
#[must_use]
pub fn transport_headers(settings: &SecuritySettings) -> SecurityMiddleware {
	SecurityMiddleware::from_security_settings(settings).with_ssl_redirect(false)
}
