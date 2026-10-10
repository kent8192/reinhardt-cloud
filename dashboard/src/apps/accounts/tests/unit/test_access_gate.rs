//! The default-deny access gate (SR-09, SR-10), exercised as a middleware.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use hyper::{HeaderMap, Method, Version};
use reinhardt::core::exception::Result;
use reinhardt::http::AuthState;
use reinhardt::{Handler, Middleware, Request, Response};
use rstest::rstest;

use crate::apps::accounts::urls::server_router::ANONYMOUS_PATHS;
use crate::config::middleware::access_gate::{AccessGate, is_unauthenticated};

struct Reached;

#[async_trait]
impl Handler for Reached {
	async fn handle(&self, _request: Request) -> Result<Response> {
		Ok(Response::ok().with_body("reached"))
	}
}

#[derive(Clone, Copy)]
enum Caller {
	Anonymous,
	SignedIn,
	Staff,
}

async fn answer(method: Method, path: &str, accept: Option<&str>, caller: Caller) -> Response {
	let mut headers = HeaderMap::new();
	if let Some(accept) = accept {
		headers.insert("Accept", accept.parse().unwrap());
	}
	let request = Request::builder()
		.method(method)
		.uri(path)
		.version(Version::HTTP_11)
		.headers(headers)
		.body(Bytes::new())
		.build()
		.unwrap();
	request.extensions.insert(match caller {
		Caller::Anonymous => AuthState::anonymous(),
		Caller::SignedIn => AuthState::authenticated("user", false, true),
		Caller::Staff => AuthState::authenticated("staff", true, true),
	});
	AccessGate
		.process(request, Arc::new(Reached))
		.await
		.unwrap()
}

#[rstest]
#[case::api_root("/api/")]
#[case::unknown_api("/api/whatever/")]
#[case::server_fn("/api/server_fn/something_new")]
#[case::admin_api("/admin/api/server_fn/get_list")]
#[tokio::test]
async fn sr_09_an_anonymous_caller_is_refused_everywhere_not_listed(#[case] path: &str) {
	// Arrange / Act
	let response = answer(Method::POST, path, None, Caller::Anonymous).await;

	// Assert
	assert_eq!(response.status.as_u16(), 401);
	assert_eq!(
		String::from_utf8(response.body.to_vec()).unwrap(),
		r#"{"error":"authentication required"}"#
	);
}

#[rstest]
#[tokio::test]
async fn sr_09_the_listed_surface_answers_anonymous_callers() {
	// Arrange / Act
	let mut statuses = Vec::new();
	for path in ANONYMOUS_PATHS {
		statuses.push(
			answer(Method::GET, path, None, Caller::Anonymous)
				.await
				.status
				.as_u16(),
		);
	}
	let admin_asset = answer(
		Method::GET,
		"/static/admin/main.js",
		None,
		Caller::Anonymous,
	)
	.await;

	// Assert
	assert!(statuses.iter().all(|status| *status == 200), "{statuses:?}");
	assert_eq!(admin_asset.status.as_u16(), 200);
}

#[rstest]
#[tokio::test]
async fn sr_09_a_signed_in_user_reaches_private_api_routes() {
	// Arrange / Act
	let response = answer(
		Method::POST,
		"/api/server_fn/anything",
		None,
		Caller::SignedIn,
	)
	.await;

	// Assert
	assert_eq!(response.status.as_u16(), 200);
}

#[rstest]
#[tokio::test]
async fn sr_09_paths_outside_the_api_and_admin_are_not_the_gates_business() {
	// Arrange / Act
	let page = answer(Method::GET, "/sign-in/", None, Caller::Anonymous).await;

	// Assert
	assert_eq!(page.status.as_u16(), 200, "the application shell is public");
}

#[rstest]
#[tokio::test]
async fn sr_09_an_anonymous_browser_asking_for_the_admin_site_is_sent_to_sign_in() {
	// Arrange / Act
	let response = answer(
		Method::GET,
		"/admin/",
		Some("text/html,*/*"),
		Caller::Anonymous,
	)
	.await;

	// Assert
	assert_eq!(response.status.as_u16(), 302);
	assert_eq!(
		response
			.headers
			.get("location")
			.and_then(|v| v.to_str().ok()),
		Some("/sign-in/")
	);
}

#[rstest]
#[case::anonymous(Caller::Anonymous, 401)]
#[case::signed_in(Caller::SignedIn, 403)]
#[case::staff(Caller::Staff, 200)]
#[tokio::test]
async fn sr_20_the_admin_site_is_reachable_only_by_staff(
	#[case] caller: Caller,
	#[case] expected: u16,
) {
	// Arrange / Act
	let shell = answer(Method::GET, "/admin/", None, caller).await;
	let api = answer(Method::POST, "/admin/api/server_fn/get_list", None, caller).await;

	// Assert
	assert_eq!(shell.status.as_u16(), expected);
	assert_eq!(api.status.as_u16(), expected);
}

#[rstest]
#[case::login("/admin/api/server_fn/admin_login")]
#[case::login_with_header("/admin/api/server_fn/admin_login_with_header")]
#[tokio::test]
async fn sr_01_the_admin_password_login_does_not_exist_for_anyone(#[case] path: &str) {
	// Arrange / Act
	let anonymous = answer(Method::POST, path, None, Caller::Anonymous).await;
	let staff = answer(Method::POST, path, None, Caller::Staff).await;

	// Assert
	assert_eq!(anonymous.status.as_u16(), 404);
	assert_eq!(staff.status.as_u16(), 404);
}

#[rstest]
#[case::dot_dot("/api/auth/github/../server_fn/secret")]
#[case::dot_dot_from_asset("/static/admin/../../api/server_fn/secret")]
#[case::encoded_slash("/api/auth%2Fgithub/")]
#[case::encoded_letter("/api/server_fn/%73ign_out")]
#[case::double_slash("//api/auth/github/")]
#[case::inner_double_slash("/api//auth/github/")]
#[case::dot_segment("/api/./auth/github/")]
#[case::no_trailing_slash("/api/auth/github")]
#[case::prefix_only("/api/auth/")]
#[case::longer("/api/auth/github/callback/extra")]
fn sr_10_a_path_that_is_not_exactly_a_listed_route_is_never_public(#[case] path: &str) {
	// Arrange / Act
	let public = is_unauthenticated(path);

	// Assert
	assert!(!public, "{path} must not be treated as public");
}

#[rstest]
fn sr_10_the_listed_routes_are_public_exactly() {
	// Arrange / Act / Assert
	for path in ANONYMOUS_PATHS {
		assert!(is_unauthenticated(path), "{path}");
	}
	assert!(is_unauthenticated("/static/admin/css/app.css"));
}

#[rstest]
#[case::dot_dot("/api/anything/../../static/admin/x")]
#[case::leading_double_slash("//api/x")]
#[case::inner_dot_segment("/api/./x")]
#[case::leading_dot_segment("/./api/x")]
#[case::encoded_prefix("/%61pi/x")]
#[case::outside_the_api_with_a_dot_segment("/static/./admin/x")]
#[tokio::test]
async fn sr_10_traversal_cannot_borrow_a_public_prefix(#[case] path: &str) {
	// Arrange / Act
	let response = answer(Method::GET, path, None, Caller::Anonymous).await;

	// Assert
	assert_eq!(response.status.as_u16(), 401);
}
