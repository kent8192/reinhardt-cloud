//! Default-deny access control for the router's surface (SR-09, SR-10).
//!
//! Every request to the API (`/api/`) or the admin site (`/admin`) is refused
//! unless the caller is signed in, except for the short, explicit list in
//! [`UNAUTHENTICATED_SURFACE`]. A route added without an authentication
//! decision is therefore private, not public. The admin site additionally
//! requires Staff.
//!
//! The gate sits after session authentication, so "signed in" means a live
//! session whose User is active (SR-07). It never inspects who the caller
//! claims to be; it only reads the authentication state that middleware
//! published.

use std::sync::Arc;

use async_trait::async_trait;
use reinhardt::core::exception::Result;
use reinhardt::http::AuthState;
use reinhardt::{Handler, Middleware, Request, Response};

use crate::apps::accounts::urls::server_router::ANONYMOUS_PATHS as ACCOUNTS_ANONYMOUS_PATHS;

/// Where the browser is sent when it asks the admin site for a page without a
/// session.
const SIGN_IN_PAGE: &str = "/sign-in/";

/// Path prefixes answered without a session (SR-10), besides the exact
/// paths of the applications (see `ANONYMOUS_PATHS` of each application's
/// server router). Everything else under `/api/` or `/admin` needs a session.
///
/// | Entry | Why it is public |
/// |-------|------------------|
/// | accounts: sign-in start and callback | they create the session |
/// | accounts: sign-in notice, current viewer | the sign-in page runs before sign-in |
/// | `/static/admin/` | the admin site's own CSS and scripts |
///
/// Not listed because the router never sees them: the single-page application
/// shell and its static assets (served by the framework's static layer), and
/// the API documentation (served only in the `local` and `ci` profiles, SR-11).
/// Later milestones add the health endpoint (SR-15) and GitHub webhooks, which
/// authenticate by signature (SR-88).
pub const UNAUTHENTICATED_PREFIXES: &[&str] = &["/static/admin/"];

/// Paths whose only purpose is a password login, which this application does
/// not have (SR-01). They answer `404` to everyone.
///
/// Workaround for the admin site's built-in credential endpoints: the admin
/// router registers `admin_login`, `admin_login_with_header`, and
/// `admin_logout` unconditionally, and `AdminSite` offers no way to omit them
/// (tracked in kent8192/reinhardt-cloud#947, upstream
/// kent8192/reinhardt-web#6720). Remove this list when the upstream issue is
/// resolved.
///
/// Ideal implementation (without workaround):
///   `site.disable_password_login();`
///   // The admin router omits the credential endpoints for sites whose
///   // users authenticate elsewhere.
pub const ABSENT_CREDENTIAL_PATHS: &[&str] = &[
	"/admin/api/server_fn/admin_login",
	"/admin/api/server_fn/admin_login_with_header",
];

fn is_admin_path(path: &str) -> bool {
	path == "/admin" || path.starts_with("/admin/")
}

/// Whether `path` is spelled the way routes are: no percent-encoding, no empty
/// segment, no `.` or `..` segment. A path that is not canonical is never
/// treated as public, because the router might resolve it to a different route
/// than the one the gate matched.
fn is_canonical(path: &str) -> bool {
	!path.contains('%')
		&& !path.contains("//")
		&& !path
			.split('/')
			.any(|segment| segment == "." || segment == "..")
}

/// Whether `path` may be served to an anonymous caller.
#[must_use]
pub fn is_unauthenticated(path: &str) -> bool {
	is_canonical(path)
		&& (ACCOUNTS_ANONYMOUS_PATHS.contains(&path)
			|| UNAUTHENTICATED_PREFIXES
				.iter()
				.any(|prefix| path.starts_with(prefix)))
}

/// Refuses anonymous callers outside the enumerated surface.
#[derive(Clone, Copy, Debug, Default)]
pub struct AccessGate;

fn json_error(status: Response, message: &str) -> Response {
	status
		.with_header("Content-Type", "application/json")
		.with_body(format!(r#"{{"error":"{message}"}}"#))
}

#[async_trait]
impl Middleware for AccessGate {
	async fn process(&self, request: Request, next: Arc<dyn Handler>) -> Result<Response> {
		let path = request.uri.path().to_owned();
		if ABSENT_CREDENTIAL_PATHS.contains(&path.as_str()) {
			return Ok(json_error(Response::not_found(), "not found"));
		}
		let admin = is_admin_path(&path);
		let guarded = path.starts_with("/api/") || admin;
		if !guarded || is_unauthenticated(&path) {
			return next.handle(request).await;
		}

		let state = request.extensions.get::<AuthState>();
		let Some(state) = state.filter(AuthState::is_authenticated) else {
			let wants_page = admin
				&& request.method.as_str() == "GET"
				&& request
					.headers
					.get("Accept")
					.and_then(|value| value.to_str().ok())
					.is_some_and(|accept| accept.contains("text/html"));
			return Ok(if wants_page {
				Response::temporary_redirect(SIGN_IN_PAGE)
			} else {
				json_error(Response::unauthorized(), "authentication required")
			});
		};
		if admin && !state.is_admin() {
			return Ok(json_error(Response::forbidden(), "forbidden"));
		}
		next.handle(request).await
	}
}
