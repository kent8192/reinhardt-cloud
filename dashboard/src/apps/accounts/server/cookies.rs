//! The cookies of sign-in and sessions, and how they are written and read.
//!
//! | Cookie | Holds | Lifetime | Path |
//! |--------|-------|----------|------|
//! | [`SESSION_COOKIE`] | the opaque session token | at most the session's absolute limit | `/` |
//! | [`BINDING_COOKIE`] | the random value binding a sign-in attempt to its browser | 10 minutes | the callback only |
//! | [`NOTICE_COOKIE`] | the ID of a one-shot sign-in notice | 5 minutes | `/` |
//!
//! All three are `HttpOnly` (script cannot read them) and `SameSite=Lax`: the
//! session must accompany the top-level navigation back from GitHub and from
//! links, but not cross-site subrequests. `Secure` follows the profile's
//! `session_cookie_secure` setting, so it is on in every deployed profile.
//! None of them contains identity data.

use std::time::Duration;

use reinhardt::Request;

/// Name of the session cookie.
pub const SESSION_COOKIE: &str = "cloud_session";
/// Name of the sign-in binding cookie.
pub const BINDING_COOKIE: &str = "cloud_signin_binding";
/// Name of the sign-in notice cookie.
pub const NOTICE_COOKIE: &str = "cloud_signin_notice";

/// How long the binding cookie lives; the state it binds to lives as long.
pub const BINDING_LIFETIME: Duration = Duration::from_secs(10 * 60);

/// Whether cookies carry the `Secure` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CookieSecurity {
	/// `Secure` is set when true.
	pub secure: bool,
}

impl CookieSecurity {
	/// `Set-Cookie` value for a cookie that lives `max_age`.
	#[must_use]
	pub fn set(self, name: &str, value: &str, path: &str, max_age: Duration) -> String {
		format!(
			"{name}={value}; HttpOnly; SameSite=Lax; Path={path}; Max-Age={}{}",
			max_age.as_secs(),
			if self.secure { "; Secure" } else { "" }
		)
	}

	/// `Set-Cookie` value that removes a cookie previously set with `path`.
	#[must_use]
	pub fn clear(self, name: &str, path: &str) -> String {
		self.set(name, "", path, Duration::ZERO)
	}
}

/// The value of cookie `name` in a `Cookie` header, if present and non-empty.
#[must_use]
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
	header
		.split(';')
		.filter_map(|pair| pair.trim().split_once('='))
		.find(|(candidate, _)| candidate.trim() == name)
		.map(|(_, value)| value.trim())
		.filter(|value| !value.is_empty())
}

/// The value of cookie `name` on `request`.
#[must_use]
pub fn request_cookie(request: &Request, name: &str) -> Option<String> {
	request
		.headers
		.get("Cookie")
		.and_then(|value| value.to_str().ok())
		.and_then(|header| cookie_value(header, name))
		.map(str::to_owned)
}
