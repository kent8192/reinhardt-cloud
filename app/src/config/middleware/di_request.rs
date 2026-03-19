//! Per-request DI context middleware.
//!
//! Rebuilds the DI context for each request so that `AuthInfo` and other
//! request-aware injectables can access the HTTP request via
//! `InjectionContext::get_http_request()`.

use std::sync::Arc;

use reinhardt::async_trait::async_trait;
use reinhardt::di::{InjectionContext, ParamContext};
use reinhardt::http::AuthState;
use reinhardt::{Handler, Middleware, Request, Response};

// Workaround: rebuilds DI context per-request with a minimal request carrying AuthState
// This is a workaround for reinhardt-web
// Without this, AuthInfo::inject() fails with "No HTTP request available in InjectionContext"
// See: https://github.com/kent8192/reinhardt-web/issues/2483

/// Middleware that rebuilds the DI context per-request.
///
/// Must run AFTER `JwtAuthMiddleware` (which injects `AuthState` into
/// extensions) so that the rebuilt context carries the auth state.
pub struct DiRequestMiddleware;

#[async_trait]
impl Middleware for DiRequestMiddleware {
	async fn process(
		&self,
		mut request: Request,
		next: Arc<dyn Handler>,
	) -> reinhardt::core::exception::Result<Response> {
		// Retrieve the shared DI context previously set by the router
		if let Some(shared_ctx) = request.get_di_context::<Arc<InjectionContext>>() {
			// Build a minimal request carrying only the AuthState from
			// the original request's extensions
			let di_request = Request::builder()
				.method(request.method.clone())
				.uri(request.uri.clone())
				.build()
				.map_err(|e| {
					reinhardt::core::exception::Error::Internal(format!(
						"Failed to build DI request: {e}"
					))
				})?;

			if let Some(auth_state) = request.extensions.get::<AuthState>() {
				di_request.extensions.insert(auth_state.clone());
			}

			// Build a per-request context that shares the singleton scope
			let singleton = Arc::clone(shared_ctx.singleton_scope());
			let per_request_ctx = InjectionContext::builder(singleton)
				.with_request(di_request)
				.with_param_context(ParamContext::new())
				.build();
			request.set_di_context(Arc::new(per_request_ctx));
		}

		next.handle(request).await
	}
}
