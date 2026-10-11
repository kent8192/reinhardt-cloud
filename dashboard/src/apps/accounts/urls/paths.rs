//! Paths the server and the browser both need.
//!
//! Workaround for the browser being unable to reverse a route that only the
//! server serves (tracked in kent8192/reinhardt-cloud#905): the paths of the
//! accounts HTTP endpoints and of the pages the server redirects to are declared
//! once, here, for both sides. Remove the constants the browser consumes when
//! that is resolved.
//!
//! Ideal implementation (without workaround):
//!   `href: reverse_server_route("accounts:github-sign-in", &[])`
//!   // The browser resolves the server route's URL from the shared route
//!   // declaration, so no path is written out twice.

/// Prefix of the accounts HTTP endpoints.
pub const AUTH_PREFIX: &str = "/api/auth/";

/// Starts a GitHub sign-in (a browser navigation, answered with a redirect).
pub const GITHUB_SIGN_IN_PATH: &str = "/api/auth/github/";

/// The sign-in page.
pub const SIGN_IN_PAGE_PATH: &str = "/sign-in/";

/// The page a Login Link opens (`manage create-login-link` prints this path, an
/// origin before it, and the secret after a `#`). The secret sits in the URL
/// fragment on purpose: the browser never sends a fragment, so the GET that loads
/// this page carries nothing to log, to forward in a `Referer`, or for a link
/// previewer to consume. Only the deliberate confirmation POST carries it.
pub const LOGIN_LINK_PAGE_PATH: &str = "/sign-in/link/";

/// Where a signed-in browser lands.
pub const HOME_PATH: &str = "/";
