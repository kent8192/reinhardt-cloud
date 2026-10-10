//! Assembly of the request surface around the application routers.
//!
//! [`assemble`] takes the merged application routes and adds what is shared by
//! all of them: the admin site, the dependency-injection registrations the
//! handlers resolve, and the middleware stack (see [`crate::config::middleware`]).
//! Middleware added first is outermost, so a request passes through the
//! transport and content-security headers, the cross-site guard, session
//! authentication, and the access gate, in that order, before it reaches a
//! handler.

use std::sync::Arc;

use reinhardt::UnifiedRouter;
use reinhardt::admin::{admin_routes_with_di, admin_static_routes};
use reinhardt::di::DiRegistrationList;

use crate::apps::accounts::server::context::AccountsServices;
use crate::apps::accounts::server::session_auth::SessionAuthMiddleware;
use crate::config::admin::build_admin_site;
use crate::config::middleware::access_gate::AccessGate;
use crate::config::middleware::cross_site_guard::CrossSiteGuard;
use crate::config::middleware::proxy_trust::ProxyTrust;
use crate::config::middleware::security_headers::{ContentSecurityPolicy, transport_headers};
use crate::config::settings::{ProjectSettings, get_resolved_settings};

/// Port assumed for the loopback request origins of a debug profile.
const DEFAULT_PORT: u16 = 8000;

/// Wrap `routes` with the shared request surface, loading the settings and
/// building the services they describe.
///
/// # Panics
///
/// Panics when the settings cannot be loaded or a service cannot be built.
/// Startup validation has already accepted the same settings, so this is a
/// configuration defect that must stop the process before it serves anything.
pub fn assemble(routes: UnifiedRouter) -> UnifiedRouter {
	let settings = get_resolved_settings()
		.expect("settings were validated at startup")
		.into_parts()
		.0;
	let services = AccountsServices::build(&settings)
		.expect("the accounts services should build from validated settings");
	assemble_with(routes, &settings, &services)
}

/// Wrap `routes` with the shared request surface around already-built services.
///
/// # Panics
///
/// Panics when the admin site cannot be assembled (a duplicate registration is
/// a programming error).
pub fn assemble_with(
	routes: UnifiedRouter,
	settings: &ProjectSettings,
	services: &AccountsServices,
) -> UnifiedRouter {
	let site = build_admin_site().expect("admin registrations should be unique");
	let (admin_router, admin_registrations) = admin_routes_with_di(Arc::clone(&site));

	let mut registrations = DiRegistrationList::new();
	registrations.merge(admin_registrations);
	registrations.register(services.clone());

	let port = std::env::var("PORT")
		.ok()
		.and_then(|port| port.parse().ok())
		.unwrap_or(DEFAULT_PORT);
	let origins = settings.accounts.request_origins(settings.core.debug, port);

	let proxies = settings
		.accounts
		.trusted_proxy_addresses()
		.expect("trusted proxies were validated at startup");
	if proxies.is_empty()
		&& settings.core.security.secure_hsts_seconds.unwrap_or(0) > 0
		&& !settings.core.debug
	{
		tracing::warn!(
			"HSTS is configured but no trusted proxy is: behind a TLS-terminating proxy, set REINHARDT_CLOUD_TRUSTED_PROXIES or Strict-Transport-Security is never sent"
		);
	}

	routes
		.mount("/admin/", admin_router)
		.mount("/static/admin/", admin_static_routes())
		.with_di_registrations(registrations)
		.with_middleware(ProxyTrust::new(proxies))
		.with_middleware(transport_headers(&settings.core.security))
		.with_middleware(ContentSecurityPolicy)
		.with_middleware(CrossSiteGuard::new(origins))
		.with_middleware(SessionAuthMiddleware::new(services.sessions.clone()))
		.with_middleware(AccessGate)
}
