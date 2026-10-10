//! Declares which proxies may vouch for the scheme of a request (SR-13).
//!
//! The framework honors `X-Forwarded-Proto` only for requests that arrive from
//! an address in the request's `TrustedProxies`, and nothing in the framework
//! fills that in, so behind a TLS-terminating proxy every request looks like
//! plain HTTP and `Strict-Transport-Security` would never be sent. This
//! middleware sets the configured proxy addresses on every request. With none
//! configured (the default) the header is never trusted, which is the safe
//! failure: no HSTS rather than HSTS earned by a forged header.

use std::net::IpAddr;
use std::sync::Arc;

use async_trait::async_trait;
use reinhardt::core::exception::Result;
use reinhardt::http::TrustedProxies;
use reinhardt::{Handler, Middleware, Request, Response};

/// Marks the configured proxies as trusted on every request.
#[derive(Clone, Debug)]
pub struct ProxyTrust {
	addresses: Vec<IpAddr>,
}

impl ProxyTrust {
	/// Create the middleware for `addresses`.
	#[must_use]
	pub fn new(addresses: Vec<IpAddr>) -> Self {
		Self { addresses }
	}
}

#[async_trait]
impl Middleware for ProxyTrust {
	async fn process(&self, request: Request, next: Arc<dyn Handler>) -> Result<Response> {
		request.set_trusted_proxies(TrustedProxies::new(self.addresses.iter().copied()));
		next.handle(request).await
	}
}
