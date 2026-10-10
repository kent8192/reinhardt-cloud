//! Request-surface middleware shared by every application.
//!
//! The project router installs these in a fixed order (outermost first):
//!
//! 1. [`proxy_trust`]: declares the TLS-terminating proxies, so the scheme
//!    they report can be believed (SR-13);
//! 2. [`security_headers`]: response headers (SR-13), applied to every
//!    router-served response, errors included;
//! 3. [`cross_site_guard`]: rejects cross-site state-changing requests that
//!    carry a session cookie (SR-12);
//! 4. the accounts application's session authentication (SR-07);
//! 5. [`access_gate`]: default-deny for everything that is not on the
//!    enumerated unauthenticated surface (SR-09, SR-10).
//!
//! Only responses the router produces pass through them. The framework's
//! static layer sits in front of the router and answers the single-page
//! application shell and the static assets itself.

pub mod access_gate;
pub mod cross_site_guard;
pub mod proxy_trust;
pub mod security_headers;
