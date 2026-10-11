//! The cross-site request guard (SR-12), exercised as a middleware.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use hyper::{HeaderMap, Method, Version};
use reinhardt::core::exception::Result;
use reinhardt::{Handler, Middleware, Request, Response};
use rstest::rstest;

use crate::config::middleware::cross_site_guard::CrossSiteGuard;

const ALLOWED: &str = "https://reinhardt-cloud.dev";

struct Reached;

#[async_trait]
impl Handler for Reached {
	async fn handle(&self, _request: Request) -> Result<Response> {
		Ok(Response::ok().with_body("reached"))
	}
}

fn request(method: Method, headers: &[(&str, &str)]) -> Request {
	request_to("/api/server_fn/sign_out", method, headers)
}

fn request_to(path: &str, method: Method, headers: &[(&str, &str)]) -> Request {
	let mut map = HeaderMap::new();
	for (name, value) in headers {
		map.insert(
			hyper::header::HeaderName::from_bytes(name.as_bytes()).unwrap(),
			value.parse().unwrap(),
		);
	}
	Request::builder()
		.method(method)
		.uri(path)
		.version(Version::HTTP_11)
		.headers(map)
		.body(Bytes::new())
		.build()
		.unwrap()
}

async fn status_of(method: Method, headers: &[(&str, &str)]) -> u16 {
	let guard = CrossSiteGuard::new(vec![ALLOWED.to_owned()]);
	guard
		.process(request(method, headers), Arc::new(Reached))
		.await
		.unwrap()
		.status
		.as_u16()
}

const SESSION: (&str, &str) = ("Cookie", "cloud_session=tok");

#[rstest]
#[case::no_origin(&[SESSION], 403)]
#[case::wrong_origin(&[SESSION, ("Origin", "https://evil.example")], 403)]
#[case::null_origin(&[SESSION, ("Origin", "null")], 403)]
#[case::lookalike_suffix(&[SESSION, ("Origin", "https://reinhardt-cloud.dev.evil.example")], 403)]
#[case::other_scheme(&[SESSION, ("Origin", "http://reinhardt-cloud.dev")], 403)]
#[case::right_origin(&[SESSION, ("Origin", ALLOWED)], 200)]
#[case::referer_only_right(&[SESSION, ("Referer", "https://reinhardt-cloud.dev/projects/")], 200)]
#[case::referer_only_wrong(&[SESSION, ("Referer", "https://evil.example/reinhardt-cloud.dev")], 403)]
#[tokio::test]
async fn sr_12_a_cookie_authenticated_post_must_prove_same_origin_intent(
	#[case] headers: &[(&str, &str)],
	#[case] expected: u16,
) {
	// Arrange / Act
	let status = status_of(Method::POST, headers).await;

	// Assert
	assert_eq!(status, expected);
}

#[rstest]
#[case(Method::PUT)]
#[case(Method::PATCH)]
#[case(Method::DELETE)]
#[tokio::test]
async fn sr_12_every_state_changing_method_is_checked(#[case] method: Method) {
	// Arrange / Act
	let status = status_of(method, &[SESSION]).await;

	// Assert
	assert_eq!(status, 403);
}

#[rstest]
#[case(Method::GET)]
#[case(Method::HEAD)]
#[case(Method::OPTIONS)]
#[tokio::test]
async fn sr_12_safe_methods_are_not_checked(#[case] method: Method) {
	// Arrange / Act
	let status = status_of(method, &[SESSION]).await;

	// Assert
	assert_eq!(status, 200);
}

#[rstest]
#[tokio::test]
async fn sr_12_a_request_without_the_session_cookie_has_no_ambient_credential_to_abuse() {
	// Arrange / Act
	let status = status_of(Method::POST, &[]).await;

	// Assert
	assert_eq!(status, 200);
}

#[rstest]
#[tokio::test]
async fn sr_12_an_authorization_header_does_not_exempt_a_cookie_request_from_the_origin_rule() {
	// Arrange / Act
	let status = status_of(
		Method::POST,
		&[SESSION, ("Authorization", "Bearer cli-token")],
	)
	.await;

	// Assert
	assert_eq!(status, 403);
}

#[rstest]
#[tokio::test]
async fn sr_12_the_rejection_is_a_generic_json_error() {
	// Arrange
	let guard = CrossSiteGuard::new(vec![ALLOWED.to_owned()]);

	// Act
	let response = guard
		.process(request(Method::POST, &[SESSION]), Arc::new(Reached))
		.await
		.unwrap();

	// Assert
	assert_eq!(response.status.as_u16(), 403);
	assert_eq!(
		String::from_utf8(response.body.to_vec()).unwrap(),
		r#"{"error":"cross-site request rejected"}"#
	);
}

const CONSUME: &str = "/api/server_fn/consume_login_link";

async fn status_of_path(path: &str, method: Method, headers: &[(&str, &str)]) -> u16 {
	let guard = CrossSiteGuard::new(vec![ALLOWED.to_owned()]);
	guard
		.process(request_to(path, method, headers), Arc::new(Reached))
		.await
		.unwrap()
		.status
		.as_u16()
}

#[rstest]
#[case::no_origin(&[], 403)]
#[case::wrong_origin(&[("Origin", "https://evil.example")], 403)]
#[case::null_origin(&[("Origin", "null")], 403)]
#[case::right_origin(&[("Origin", ALLOWED)], 200)]
#[case::referer_only_right(&[("Referer", "https://reinhardt-cloud.dev/sign-in/link/")], 200)]
#[tokio::test]
async fn sr_12_confirming_a_login_link_proves_its_origin_even_without_a_session_cookie(
	#[case] headers: &[(&str, &str)],
	#[case] expected: u16,
) {
	// Arrange / Act
	let status = status_of_path(CONSUME, Method::POST, headers).await;

	// Assert
	assert_eq!(status, expected);
}

#[rstest]
#[tokio::test]
async fn sr_12_only_state_changing_requests_to_a_session_starting_path_need_proof() {
	// Arrange / Act
	let get = status_of_path(CONSUME, Method::GET, &[]).await;
	let other_path = status_of_path("/api/server_fn/take_sign_in_notice", Method::POST, &[]).await;

	// Assert
	assert_eq!(get, 200, "a GET is a safe method");
	assert_eq!(other_path, 200, "other cookie-less requests stay unchecked");
}
