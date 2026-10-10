//! Security response headers (SR-13, SR-14), exercised as middleware.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use hyper::{HeaderMap, Method, Version};
use reinhardt::conf::SecuritySettings;
use reinhardt::core::exception::{Error, Result};
use reinhardt::{Handler, Middleware, Request, Response};
use rstest::rstest;

use crate::config::middleware::proxy_trust::ProxyTrust;
use crate::config::middleware::security_headers::{
	API_CSP, ContentSecurityPolicy, PAGE_CSP, transport_headers,
};

enum Outcome {
	Page,
	PageWithOwnPolicy,
	Failure,
}

struct Probe(Outcome);

#[async_trait]
impl Handler for Probe {
	async fn handle(&self, _request: Request) -> Result<Response> {
		match self.0 {
			Outcome::Page => Ok(Response::ok().with_body("ok")),
			Outcome::PageWithOwnPolicy => Ok(Response::ok().with_header(
				"Content-Security-Policy",
				"default-src 'self' 'unsafe-inline'",
			)),
			Outcome::Failure => Err(Error::Internal(
				"postgres://app:hunter2@db.internal:5432/cloud constraint accounts_users_pkey"
					.to_owned(),
			)),
		}
	}
}

fn get(path: &str, secure: bool, headers: &[(&str, &str)]) -> Request {
	let mut map = HeaderMap::new();
	for (name, value) in headers {
		map.insert(
			hyper::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
			value.parse().unwrap(),
		);
	}
	Request::builder()
		.method(Method::GET)
		.uri(path)
		.version(Version::HTTP_11)
		.headers(map)
		.secure(secure)
		.body(Bytes::new())
		.build()
		.unwrap()
}

async fn through_csp(path: &str, outcome: Outcome) -> Response {
	ContentSecurityPolicy
		.process(get(path, false, &[]), Arc::new(Probe(outcome)))
		.await
		.unwrap()
}

fn header<'a>(response: &'a Response, name: &str) -> Option<&'a str> {
	response
		.headers
		.get(name)
		.and_then(|value| value.to_str().ok())
}

#[rstest]
fn sr_13_the_api_policy_allows_nothing() {
	// Arrange / Act / Assert
	assert_eq!(API_CSP, "default-src 'none'; frame-ancestors 'none'");
}

#[rstest]
fn sr_13_the_page_policy_restricts_scripts_to_the_origin_plus_webassembly() {
	// Arrange / Act
	let directives: Vec<&str> = PAGE_CSP.split(';').map(str::trim).collect();

	// Assert
	assert!(directives.contains(&"default-src 'self'"));
	assert!(directives.contains(&"script-src 'self' 'wasm-unsafe-eval'"));
	assert!(!PAGE_CSP.contains("script-src 'self' 'unsafe-inline'"));
	assert!(!PAGE_CSP.contains("unsafe-eval'") || PAGE_CSP.contains("wasm-unsafe-eval"));
}

#[rstest]
#[case::session_api("/api/server_fn/current_viewer")]
#[case::unknown_api("/api/nothing-here/")]
#[tokio::test]
async fn sr_13_api_responses_carry_the_deny_everything_policy_and_are_not_cached(
	#[case] path: &str,
) {
	// Arrange / Act
	let response = through_csp(path, Outcome::Page).await;

	// Assert
	assert_eq!(header(&response, "content-security-policy"), Some(API_CSP));
	assert_eq!(header(&response, "cache-control"), Some("no-store"));
}

#[rstest]
#[tokio::test]
async fn sr_13_api_policy_replaces_one_a_handler_set() {
	// Arrange / Act
	let response = through_csp("/api/anything/", Outcome::PageWithOwnPolicy).await;

	// Assert
	assert_eq!(header(&response, "content-security-policy"), Some(API_CSP));
}

#[rstest]
#[case::page("/sign-in/")]
#[case::root("/")]
#[tokio::test]
async fn sr_13_page_responses_carry_the_page_policy(#[case] path: &str) {
	// Arrange / Act
	let response = through_csp(path, Outcome::Page).await;

	// Assert
	assert_eq!(header(&response, "content-security-policy"), Some(PAGE_CSP));
}

#[rstest]
#[case::admin_shell("/admin/")]
#[case::admin_asset("/static/admin/main.js")]
#[tokio::test]
async fn sr_13_the_admin_site_keeps_its_own_documented_policy(#[case] path: &str) {
	// Arrange / Act
	let response = through_csp(path, Outcome::PageWithOwnPolicy).await;

	// Assert
	assert_eq!(
		header(&response, "content-security-policy"),
		Some("default-src 'self' 'unsafe-inline'")
	);
}

#[rstest]
#[case::docs("/api/docs")]
#[case::redoc("/api/redoc")]
#[tokio::test]
async fn sr_11_the_documentation_viewers_get_no_policy_that_would_blank_them(#[case] path: &str) {
	// Arrange / Act
	let response = through_csp(path, Outcome::Page).await;

	// Assert
	assert_eq!(header(&response, "content-security-policy"), None);
}

#[rstest]
#[tokio::test]
async fn sr_14_a_failing_handler_yields_a_generic_error_that_still_carries_the_headers() {
	// Arrange / Act
	let response = through_csp("/api/anything/", Outcome::Failure).await;

	// Assert
	assert_eq!(response.status.as_u16(), 500);
	let body = String::from_utf8(response.body.to_vec()).unwrap();
	for leaked in [
		"postgres://",
		"hunter2",
		"db.internal",
		"accounts_users_pkey",
		"constraint",
	] {
		assert!(
			!body.contains(leaked),
			"the error body leaks `{leaked}`: {body}"
		);
	}
	assert_eq!(header(&response, "content-security-policy"), Some(API_CSP));
}

fn security_settings() -> SecuritySettings {
	SecuritySettings {
		secure_hsts_seconds: Some(31_536_000),
		secure_hsts_include_subdomains: true,
		secure_hsts_preload: true,
		secure_ssl_redirect: true,
		..SecuritySettings::default()
	}
}

async fn through_transport(request: Request) -> Response {
	transport_headers(&security_settings())
		.process(request, Arc::new(Probe(Outcome::Page)))
		.await
		.unwrap()
}

#[rstest]
#[tokio::test]
async fn sr_13_hsts_is_sent_when_the_request_arrived_over_https() {
	// Arrange / Act
	let response = through_transport(get("/", true, &[])).await;

	// Assert
	assert_eq!(
		header(&response, "strict-transport-security"),
		Some("max-age=31536000; includeSubDomains; preload")
	);
}

#[rstest]
#[tokio::test]
async fn sr_13_hsts_is_not_sent_over_plain_http() {
	// Arrange / Act
	let response = through_transport(get("/", false, &[])).await;

	// Assert
	assert_eq!(header(&response, "strict-transport-security"), None);
}

#[rstest]
#[tokio::test]
async fn sr_13_a_forged_forwarded_proto_header_does_not_earn_hsts() {
	// Arrange: the request did not come from a configured proxy.
	let request = get("/", false, &[("X-Forwarded-Proto", "https")]);

	// Act
	let response = through_transport(request).await;

	// Assert
	assert_eq!(header(&response, "strict-transport-security"), None);
}

#[rstest]
#[tokio::test]
async fn sr_13_plain_http_is_never_redirected_by_the_application() {
	// Arrange / Act
	let response = through_transport(get("/", false, &[])).await;

	// Assert
	assert_eq!(
		response.status.as_u16(),
		200,
		"redirecting is the proxy's job"
	);
}

#[rstest]
#[tokio::test]
async fn sr_13_every_response_carries_the_basic_hardening_headers() {
	// Arrange / Act
	let response = through_transport(get("/", false, &[])).await;

	// Assert
	assert_eq!(header(&response, "x-content-type-options"), Some("nosniff"));
	assert_eq!(header(&response, "x-frame-options"), Some("DENY"));
	assert_eq!(header(&response, "referrer-policy"), Some("same-origin"));
}

const PROXY: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));

/// The transport headers behind a proxy declaration, as the router stacks them.
struct BehindProxyTrust;

#[async_trait]
impl Handler for BehindProxyTrust {
	async fn handle(&self, request: Request) -> Result<Response> {
		transport_headers(&security_settings())
			.process(request, Arc::new(Probe(Outcome::Page)))
			.await
	}
}

async fn through_proxy_trust(
	trusted: Vec<IpAddr>,
	peer: IpAddr,
	forwarded_proto: &str,
) -> Response {
	let mut headers = HeaderMap::new();
	headers.insert("X-Forwarded-Proto", forwarded_proto.parse().unwrap());
	let request = Request::builder()
		.method(Method::GET)
		.uri("/api/anything/")
		.version(Version::HTTP_11)
		.headers(headers)
		.remote_addr(SocketAddr::new(peer, 40_000))
		.body(Bytes::new())
		.build()
		.unwrap();
	ProxyTrust::new(trusted)
		.process(request, Arc::new(BehindProxyTrust))
		.await
		.unwrap()
}

#[rstest]
#[tokio::test]
async fn sr_13_hsts_is_sent_behind_a_configured_tls_terminating_proxy() {
	// Arrange / Act
	let response = through_proxy_trust(vec![PROXY], PROXY, "https").await;

	// Assert
	assert_eq!(
		header(&response, "strict-transport-security"),
		Some("max-age=31536000; includeSubDomains; preload")
	);
}

#[rstest]
#[case::proxy_says_http(vec![PROXY], PROXY, "http")]
#[case::no_proxy_configured(vec![], PROXY, "https")]
#[case::forwarded_by_a_stranger(vec![PROXY], IpAddr::V4(Ipv4Addr::new(203, 0, 113, 9)), "https")]
#[tokio::test]
async fn sr_13_hsts_is_not_sent_when_the_proxy_is_not_trusted_or_says_http(
	#[case] trusted: Vec<IpAddr>,
	#[case] peer: IpAddr,
	#[case] proto: &str,
) {
	// Arrange / Act
	let response = through_proxy_trust(trusted, peer, proto).await;

	// Assert
	assert_eq!(header(&response, "strict-transport-security"), None);
}
