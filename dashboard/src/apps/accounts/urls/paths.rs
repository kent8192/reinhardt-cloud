//! Paths the server and the browser both need.
//!
//! The browser cannot reverse a route that only the server serves (upstream
//! gap, tracked in kent8192/reinhardt-cloud#905), so the paths of the accounts
//! HTTP endpoints and of the pages the server redirects to are declared once,
//! here, for both sides.

/// Prefix of the accounts HTTP endpoints.
pub const AUTH_PREFIX: &str = "/api/auth/";

/// Starts a GitHub sign-in (a browser navigation, answered with a redirect).
pub const GITHUB_SIGN_IN_PATH: &str = "/api/auth/github/";

/// The sign-in page.
pub const SIGN_IN_PAGE_PATH: &str = "/sign-in/";

/// Where a signed-in browser lands.
pub const HOME_PATH: &str = "/";
