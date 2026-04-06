//! JWT authentication interceptor for gRPC requests.

use jsonwebtoken::{DecodingKey, Validation, decode};
use reinhardt_cloud_core::auth::Claims;
use tonic::{Request, Status};

/// Paths that do not require authentication.
const PUBLIC_PATHS: &[&str] = &[
	"/grpc.health.v1.Health/Check",
	"/grpc.health.v1.Health/Watch",
	"/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
	"/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
];

/// JWT authentication interceptor for gRPC.
///
/// Extracts Bearer tokens from the `authorization` metadata key,
/// validates them, and injects `Claims` into request extensions.
#[derive(Clone)]
pub struct JwtInterceptor {
	secret: Vec<u8>,
}

impl JwtInterceptor {
	/// Create a new JWT interceptor with the given secret.
	pub fn new(secret: &[u8]) -> Self {
		Self {
			secret: secret.to_vec(),
		}
	}

	/// Validate a token and return claims.
	fn validate_token(&self, token: &str) -> Result<Claims, Status> {
		decode::<Claims>(
			token,
			&DecodingKey::from_secret(&self.secret),
			&Validation::default(),
		)
		.map(|data| data.claims)
		.map_err(|e| Status::unauthenticated(format!("Invalid token: {e}")))
	}
}

impl tonic::service::Interceptor for JwtInterceptor {
	fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
		// Check if the path is public (skip auth)
		if let Some(path) = request.extensions().get::<tonic::GrpcMethod>() {
			let full_path = format!("/{}/{}", path.service(), path.method());
			if PUBLIC_PATHS.iter().any(|p| *p == full_path) {
				return Ok(request);
			}
		}

		// Extract Bearer token from authorization metadata
		let token = request
			.metadata()
			.get("authorization")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.strip_prefix("Bearer "))
			.ok_or_else(|| Status::unauthenticated("Missing authorization token"))?;

		// Validate and inject claims
		let claims = self.validate_token(token)?;
		request.extensions_mut().insert(claims);

		Ok(request)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use reinhardt_cloud_core::auth;
	use rstest::rstest;
	use tonic::service::Interceptor;
	use uuid::Uuid;

	const TEST_SECRET: &[u8] = b"test-secret-key-for-grpc-jwt";

	#[rstest]
	fn test_validate_valid_token() {
		// Arrange
		let interceptor = JwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::new_v4();
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();

		// Act
		let claims = interceptor.validate_token(&token).unwrap();

		// Assert
		assert_eq!(claims.sub, user_id.to_string());
		assert_eq!(claims.username, "alice");
	}

	#[rstest]
	fn test_validate_invalid_token() {
		// Arrange
		let interceptor = JwtInterceptor::new(TEST_SECRET);

		// Act
		let result = interceptor.validate_token("invalid-token");

		// Assert
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_validate_wrong_secret() {
		// Arrange
		let interceptor = JwtInterceptor::new(b"different-secret");
		let user_id = Uuid::new_v4();
		let token = auth::create_token(user_id, "bob", TEST_SECRET, 24).unwrap();

		// Act
		let result = interceptor.validate_token(&token);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_validate_expired_token() {
		// Arrange
		let interceptor = JwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::new_v4();
		let token = auth::create_token(user_id, "charlie", TEST_SECRET, -1).unwrap();

		// Act
		let result = interceptor.validate_token(&token);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_interceptor_call_missing_auth_header() {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let req = Request::new(());

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_interceptor_call_malformed_bearer() {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::new_v4();
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		// Use "Token" prefix instead of "Bearer"
		req.metadata_mut().insert(
			"authorization",
			format!("Token {token}").parse().unwrap(),
		);

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_interceptor_call_empty_bearer() {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let mut req = Request::new(());
		// "Bearer " with no token after the prefix
		req.metadata_mut().insert(
			"authorization",
			"Bearer ".parse().unwrap(),
		);

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_interceptor_reusable_across_calls() {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::new_v4();
		let valid_token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();

		// Act — first call: valid token
		let mut req1 = Request::new(());
		req1.metadata_mut().insert(
			"authorization",
			format!("Bearer {valid_token}").parse().unwrap(),
		);
		let result1 = interceptor.call(req1);

		// Act — second call: invalid token
		let mut req2 = Request::new(());
		req2.metadata_mut().insert(
			"authorization",
			"Bearer bad-token".parse().unwrap(),
		);
		let result2 = interceptor.call(req2);

		// Act — third call: valid token again
		let mut req3 = Request::new(());
		req3.metadata_mut().insert(
			"authorization",
			format!("Bearer {valid_token}").parse().unwrap(),
		);
		let result3 = interceptor.call(req3);

		// Assert
		assert!(result1.is_ok());
		assert!(result2.is_err());
		assert!(result3.is_ok());
	}

	#[rstest]
	fn test_interceptor_empty_secret() {
		// Arrange — empty secret
		let interceptor = JwtInterceptor::new(&[]);
		let user_id = Uuid::new_v4();
		// Token created with the original secret won't match empty secret
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();

		// Act
		let result = interceptor.validate_token(&token);

		// Assert
		assert!(result.is_err());
	}
}
