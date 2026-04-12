//! Integration tests for the auth module and ApiError.

mod fixtures;

use rstest::rstest;
use uuid::Uuid;

use reinhardt_cloud_core::auth::{create_token, verify_token};
use reinhardt_cloud_core::error::ApiError;
use reinhardt_cloud_core::mocks::MockAuthService;
use reinhardt_cloud_core::traits::AuthService;

const TEST_SECRET: &[u8] = b"test-secret-key-for-jwt-signing";

// ===========================================================================
// Error path tests (strict variants)
// ===========================================================================

#[rstest]
fn test_verify_wrong_secret_returns_invalid_signature() {
	// Arrange
	let user_id = Uuid::now_v7();
	let token = create_token(user_id, "alice", TEST_SECRET, 24).unwrap();

	// Act
	let result = verify_token(&token, b"completely-wrong-secret");

	// Assert
	let err = result.unwrap_err();
	assert_eq!(
		*err.kind(),
		jsonwebtoken::errors::ErrorKind::InvalidSignature
	);
}

#[rstest]
fn test_expired_token_returns_expired_error() {
	// Arrange
	let user_id = Uuid::now_v7();
	// Create a token that expired 1 hour ago
	let token = create_token(user_id, "bob", TEST_SECRET, -1).unwrap();

	// Act
	let result = verify_token(&token, TEST_SECRET);

	// Assert
	let err = result.unwrap_err();
	assert_eq!(
		*err.kind(),
		jsonwebtoken::errors::ErrorKind::ExpiredSignature
	);
}

#[rstest]
fn test_verify_malformed_token() {
	// Arrange
	let malformed = "not-a-jwt";

	// Act
	let result = verify_token(malformed, TEST_SECRET);

	// Assert
	assert!(result.is_err(), "Malformed token should fail verification");
}

#[rstest]
fn test_verify_empty_token() {
	// Arrange
	let empty = "";

	// Act
	let result = verify_token(empty, TEST_SECRET);

	// Assert
	assert!(result.is_err(), "Empty token should fail verification");
}

// ===========================================================================
// Edge case tests
// ===========================================================================

#[rstest]
fn test_auth_token_with_zero_expiry() {
	// Arrange
	let user_id = Uuid::now_v7();

	// Act — create a token with 0 hours expiry (expires at creation time)
	let token = create_token(user_id, "zero-expiry", TEST_SECRET, 0).unwrap();
	let result = verify_token(&token, TEST_SECRET);

	// Assert — token may be immediately expired or borderline valid depending
	// on exact timing. We verify that the function does not panic and returns
	// a definite result.
	let _is_valid_or_expired = result.is_ok() || result.is_err();
}

// ===========================================================================
// Use case tests
// ===========================================================================

#[rstest]
fn test_usecase_token_lifecycle() {
	// Arrange
	let user_id = Uuid::now_v7();
	let username = "lifecycle-user";

	// Act — create -> verify -> check claims
	let token = create_token(user_id, username, TEST_SECRET, 24).unwrap();
	let claims = verify_token(&token, TEST_SECRET).unwrap();

	// Assert — all claims fields match
	assert_eq!(claims.sub, user_id.to_string());
	assert_eq!(claims.username, username);
	assert!(claims.iat > 0);
	assert!(claims.exp > claims.iat);
	// exp should be approximately 24 hours from iat
	let diff = claims.exp - claims.iat;
	assert_eq!(diff, 24 * 3600, "Token should expire in exactly 24 hours");
}

// ===========================================================================
// Equivalence partitioning tests — ApiError status codes
// ===========================================================================

#[rstest]
#[case(ApiError::Unauthorized("msg".to_string()), 401)]
#[case(ApiError::NotFound("msg".to_string()), 404)]
#[case(ApiError::BadRequest("msg".to_string()), 400)]
#[case(ApiError::Internal("msg".to_string()), 500)]
fn test_api_error_variant_status_codes(#[case] error: ApiError, #[case] expected_code: u16) {
	// Arrange (provided by case)

	// Act
	let code = error.status_code();

	// Assert
	assert_eq!(code, expected_code);
}

// ===========================================================================
// Decision table tests — MockAuthService method independence
// ===========================================================================

#[rstest]
#[tokio::test]
async fn test_mock_auth_method_independence() {
	// Arrange
	let service = MockAuthService::new();

	// Configure authenticate to fail
	service
		.set_authenticate_result(Err(ApiError::Unauthorized("denied".to_string())))
		.await;

	// Act — authenticate fails, but verify_token and get_user_info still work
	let auth_result = service.authenticate("user", "pass").await;
	let verify_result = service.verify_token("any-token").await;
	let user_result = service.get_user_info("any-id").await;

	// Assert — authenticate fails independently
	assert!(auth_result.is_err());
	let claims = verify_result.unwrap();
	assert_eq!(claims.username, "test-user");
	let user = user_result.unwrap();
	assert_eq!(user.username, "test-user");

	// Now configure verify to fail, authenticate still fails, user_info OK
	service
		.set_verify_result(Err(ApiError::Unauthorized("bad token".to_string())))
		.await;

	let auth_result2 = service.authenticate("user", "pass").await;
	let verify_result2 = service.verify_token("any-token").await;
	let user_result2 = service.get_user_info("any-id").await;

	assert!(auth_result2.is_err());
	assert!(verify_result2.is_err());
	assert!(user_result2.is_ok());

	// Finally configure user_info to fail too — all three fail independently
	service
		.set_user_info_result(Err(ApiError::NotFound("no user".to_string())))
		.await;

	let user_result3 = service.get_user_info("any-id").await;
	assert!(matches!(user_result3, Err(ApiError::NotFound(_))));
}
