//! The assembled router and the startup validation that guards it (SR-13).
//!
//! These exercise the stack as `config::web::assemble_with` builds it, not a
//! middleware alone, so the order of the layers is part of what is tested.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use async_trait::async_trait;
use bytes::Bytes;
use hyper::{HeaderMap, Method, Version};
use reinhardt::core::exception::Result;
use reinhardt::{Handler, Request, Response, UnifiedRouter};
use rstest::rstest;
use serial_test::serial;

use crate::apps::accounts::server::context::AccountsServices;
use crate::config::settings::test_support::{EnvGuard, required_env};
use crate::config::settings::{ProjectSettings, get_resolved_settings};
use crate::config::web::assemble_with;
use crate::server::prepare_context;

const PROXY: IpAddr = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 7));

struct Reached;

#[async_trait]
impl Handler for Reached {
	async fn handle(&self, _request: Request) -> Result<Response> {
		Ok(Response::ok().with_body("reached"))
	}
}

/// The `ci` settings with HSTS switched on and `proxies` declared.
fn settings_declaring(proxies: &str) -> ProjectSettings {
	let mut settings = get_resolved_settings()
		.expect("the ci settings load")
		.into_parts()
		.0;
	settings.core.debug = false;
	settings.core.security.secure_hsts_seconds = Some(31_536_000);
	settings.accounts.trusted_proxies = proxies.to_owned();
	settings
}

/// Sends `GET /probe/` through the assembled router from `peer`, claiming
/// `X-Forwarded-Proto: https`, and returns the response.
async fn through_the_assembled_router(settings: &ProjectSettings, peer: IpAddr) -> Response {
	let services = AccountsServices::build(settings).expect("the services build");
	let routes = UnifiedRouter::new().server(|server| server.handler("/probe/", Reached));
	let router = assemble_with(routes, settings, &services).into_server();
	let mut headers = HeaderMap::new();
	headers.insert("X-Forwarded-Proto", "https".parse().unwrap());
	let request = Request::builder()
		.method(Method::GET)
		.uri("/probe/")
		.version(Version::HTTP_11)
		.headers(headers)
		.remote_addr(SocketAddr::new(peer, 40_000))
		.body(Bytes::new())
		.build()
		.unwrap();
	router.handle(request).await.expect("the router answers")
}

fn hsts(response: &Response) -> Option<&str> {
	response
		.headers
		.get("strict-transport-security")
		.and_then(|value| value.to_str().ok())
}

#[rstest]
#[tokio::test]
#[serial(env_settings_load)]
async fn sr_13_proxy_trust_wraps_every_other_layer_of_the_assembled_router() {
	// Arrange
	let _env = EnvGuard::apply(&required_env("ci"));
	let settings = settings_declaring("10.0.0.7");

	// Act
	let response = through_the_assembled_router(&settings, PROXY).await;

	// Assert
	assert_eq!(
		hsts(&response),
		Some("max-age=31536000"),
		"the transport headers saw the trusted proxy ProxyTrust declared, status {}",
		response.status
	);
}

#[rstest]
#[case::stranger(PROXY, "10.0.0.8")]
#[case::no_proxy_declared(PROXY, "")]
#[tokio::test]
#[serial(env_settings_load)]
async fn sr_13_a_peer_that_is_not_a_declared_proxy_cannot_earn_hsts(
	#[case] peer: IpAddr,
	#[case] declared: &str,
) {
	// Arrange
	let _env = EnvGuard::apply(&required_env("ci"));
	let settings = settings_declaring(declared);

	// Act
	let response = through_the_assembled_router(&settings, peer).await;

	// Assert
	assert_eq!(hsts(&response), None, "status {}", response.status);
}

#[rstest]
#[case::cidr_range("10.0.0.0/8")]
#[case::hostname("proxy.internal")]
#[case::one_bad_entry_among_good("10.0.0.7, not-an-address")]
#[serial(env_settings_load)]
fn sr_13_a_malformed_trusted_proxy_list_stops_startup(#[case] value: &str) {
	// Arrange
	let mut vars = required_env("ci");
	vars.push(("REINHARDT_CLOUD_TRUSTED_PROXIES", Some(value)));
	let _env = EnvGuard::apply(&vars);

	// Act
	let started = prepare_context("127.0.0.1:0");

	// Assert
	let error = started.expect_err("startup is refused").to_string();
	assert!(
		error.contains("accounts.trusted_proxies"),
		"the error names the setting: {error}"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_13_a_well_formed_trusted_proxy_list_starts() {
	// Arrange
	let mut vars = required_env("ci");
	vars.push((
		"REINHARDT_CLOUD_TRUSTED_PROXIES",
		Some("10.0.0.7, 2001:db8::1"),
	));
	let _env = EnvGuard::apply(&vars);

	// Act
	let started = prepare_context("127.0.0.1:0");

	// Assert
	assert!(started.is_ok(), "{:?}", started.err());
}
