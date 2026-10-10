//! JWT authentication interceptors for gRPC requests.

use std::sync::Arc;

use jsonwebtoken::{DecodingKey, Validation, decode};
use reinhardt_cloud_core::auth::Claims;
use std::task::{Context, Poll};

use tonic::server::NamedService;
use tonic::service::interceptor::InterceptedService;
use tonic::{Request, Status};
// `Service` and `http` are taken from `tonic::codegen` because importing them
// directly would require new `tower-service` and `http` dependencies.
use tonic::codegen::{Service, http};

use crate::agent_claims::{AgentClaims, verify_agent_token};

/// Paths that do not require authentication.
const PUBLIC_PATHS: &[&str] = &[
	"/grpc.health.v1.Health/Check",
	"/grpc.health.v1.Health/Watch",
	"/grpc.reflection.v1alpha.ServerReflection/ServerReflectionInfo",
	"/grpc.reflection.v1.ServerReflection/ServerReflectionInfo",
];

/// Paths that require agent authentication (not user authentication).
const AGENT_PATH_PREFIXES: &[&str] = &["/reinhardt.cloud.cluster_agent.AgentService/"];
const LOG_SERVICE_PUSH_LOGS_PATH: &str = "/reinhardt.cloud.log.LogService/PushLogs";

/// Full gRPC path (`/package.Service/Method`) of the request being served.
///
/// tonic's `Interceptor` only receives a `Request<()>` that has lost the HTTP
/// URI, and the `tonic::GrpcMethod` extension is inserted by generated client
/// code only, so a server-side interceptor cannot learn which method was
/// called by itself. [`GrpcPathService`] records the path in the request
/// extensions ahead of the interceptor, and the interceptors in this module
/// read it from there. Without it they fail closed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrpcPath(String);

impl GrpcPath {
	/// Create a path marker from a full gRPC path such as
	/// `/reinhardt.cloud.log.LogService/PushLogs`.
	pub fn new(path: impl Into<String>) -> Self {
		Self(path.into())
	}

	/// Return the full gRPC path.
	pub fn as_str(&self) -> &str {
		&self.0
	}
}

/// Return the gRPC path recorded by [`GrpcPathService`], if any.
fn request_path(request: &Request<()>) -> Option<&str> {
	request.extensions().get::<GrpcPath>().map(GrpcPath::as_str)
}

/// Tower service that records the request path for the wrapped service.
///
/// It must sit *outside* tonic's `InterceptedService`, because that is the
/// layer that strips the URI before calling the interceptor. Use
/// [`intercepted`] to build the pair; wiring an interceptor of this module
/// through `with_interceptor` alone leaves the path unknown and every guarded
/// call is rejected.
#[derive(Debug, Clone)]
pub struct GrpcPathService<S> {
	inner: S,
}

impl<S> GrpcPathService<S> {
	/// Wrap `inner` so that its requests carry a [`GrpcPath`] extension.
	pub fn new(inner: S) -> Self {
		Self { inner }
	}
}

impl<S, B> Service<http::Request<B>> for GrpcPathService<S>
where
	S: Service<http::Request<B>>,
{
	type Response = S::Response;
	type Error = S::Error;
	type Future = S::Future;

	fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
		self.inner.poll_ready(cx)
	}

	fn call(&mut self, mut request: http::Request<B>) -> Self::Future {
		let path = GrpcPath::new(request.uri().path());
		request.extensions_mut().insert(path);
		self.inner.call(request)
	}
}

impl<S: NamedService> NamedService for GrpcPathService<S> {
	const NAME: &'static str = S::NAME;
}

/// Guard `service` with `interceptor`, making the request path available to it.
///
/// Drop-in replacement for `XxxServer::with_interceptor(service, interceptor)`
/// for the interceptors in this module.
pub fn intercepted<S, I>(service: S, interceptor: I) -> GrpcPathService<InterceptedService<S, I>> {
	GrpcPathService::new(InterceptedService::new(service, interceptor))
}

type AgentTokenValidator = Arc<dyn Fn(&str, &AgentClaims) -> Result<(), Status> + Send + Sync>;

/// JWT authentication interceptor for gRPC.
///
/// Extracts Bearer tokens from the `authorization` metadata key,
/// validates them, and injects `Claims` into request extensions. Wire it with
/// [`intercepted`] so the public-path bypass can see the request path.
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
		if let Some(path) = request_path(&request)
			&& PUBLIC_PATHS.contains(&path)
		{
			return Ok(request);
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

/// JWT authentication interceptor for cluster agent gRPC calls.
///
/// Validates tokens issued to cluster agents (containing a `cluster_id`
/// claim). Only agent paths (`AGENT_PATH_PREFIXES` and `LogService/PushLogs`)
/// can be authorized; every other path, and any call without a [`GrpcPath`],
/// is rejected with `Unauthenticated`. Wire it with [`intercepted`] on agent
/// services only.
#[derive(Clone)]
pub struct AgentJwtInterceptor {
	secret: Vec<u8>,
	token_validator: Option<AgentTokenValidator>,
}

impl AgentJwtInterceptor {
	/// Create a new agent JWT interceptor with the given secret.
	pub fn new(secret: &[u8]) -> Self {
		Self {
			secret: secret.to_vec(),
			token_validator: None,
		}
	}

	/// Attach a validator that enforces stateful token revocation.
	pub fn with_token_validator(
		mut self,
		validator: impl Fn(&str, &AgentClaims) -> Result<(), Status> + Send + Sync + 'static,
	) -> Self {
		self.token_validator = Some(Arc::new(validator));
		self
	}

	/// Validate an agent token and return decoded claims.
	pub fn validate_token(&self, token: &str) -> Result<AgentClaims, Status> {
		let claims = verify_agent_token(token, &self.secret)
			.map_err(|e| Status::unauthenticated(format!("Invalid agent token: {e}")))?;
		if let Some(validator) = &self.token_validator {
			validator(token, &claims)?;
		}
		Ok(claims)
	}

	/// Return true when the path belongs to the cluster-agent service.
	fn is_agent_path(full_path: &str) -> bool {
		if full_path == LOG_SERVICE_PUSH_LOGS_PATH {
			return true;
		}
		AGENT_PATH_PREFIXES
			.iter()
			.any(|prefix| full_path.starts_with(prefix))
	}
}

impl tonic::service::Interceptor for AgentJwtInterceptor {
	fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
		// Only agent paths may be authorized here. Any other path, or an
		// unknown one, is refused rather than silently passed through.
		match request_path(&request) {
			Some(path) if Self::is_agent_path(path) => {}
			Some(_) => {
				return Err(Status::unauthenticated(
					"Not an agent path; cannot authorize request with the agent interceptor",
				));
			}
			None => {
				return Err(Status::unauthenticated(
					"Missing gRPC path; cannot authorize agent request",
				));
			}
		}

		// Extract Bearer token.
		let token = request
			.metadata()
			.get("authorization")
			.and_then(|v| v.to_str().ok())
			.and_then(|v| v.strip_prefix("Bearer "))
			.ok_or_else(|| Status::unauthenticated("Missing agent authorization token"))?;

		// Validate and inject claims into request extensions.
		let claims = self.validate_token(token)?;
		request.extensions_mut().insert(claims);

		Ok(request)
	}
}

/// Method-aware JWT interceptor for the log service.
///
/// Log reads are dashboard-user actions, while `PushLogs` is an agent
/// ingestion path. Keeping the split in one interceptor avoids registering
/// the same tonic service twice with conflicting authentication policies.
/// Wire it with [`intercepted`]; without a [`GrpcPath`] every call is rejected.
#[derive(Clone)]
pub struct LogServiceJwtInterceptor {
	user: JwtInterceptor,
	agent: AgentJwtInterceptor,
}

impl LogServiceJwtInterceptor {
	/// Create a new log-service interceptor with shared JWT secret material.
	pub fn new(secret: &[u8]) -> Self {
		Self {
			user: JwtInterceptor::new(secret),
			agent: AgentJwtInterceptor::new(secret),
		}
	}

	/// Attach the stateful token validator used for agent log ingestion.
	pub fn with_agent_token_validator(
		mut self,
		validator: impl Fn(&str, &AgentClaims) -> Result<(), Status> + Send + Sync + 'static,
	) -> Self {
		self.agent = self.agent.with_token_validator(validator);
		self
	}
}

impl tonic::service::Interceptor for LogServiceJwtInterceptor {
	fn call(&mut self, request: Request<()>) -> Result<Request<()>, Status> {
		match request_path(&request) {
			Some(LOG_SERVICE_PUSH_LOGS_PATH) => self.agent.call(request),
			Some(_) => self.user.call(request),
			// Without a known path the credential kind cannot be chosen.
			None => Err(Status::unauthenticated(
				"Missing gRPC path; cannot authorize log service request",
			)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::agent_claims::create_agent_token;
	use reinhardt_cloud_core::auth;
	use rstest::rstest;
	use tonic::service::Interceptor;
	use uuid::Uuid;

	const TEST_SECRET: &[u8] = b"test-secret-key-for-grpc-jwt";

	#[rstest]
	fn test_validate_valid_token() {
		// Arrange
		let interceptor = JwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::now_v7();
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
		let user_id = Uuid::now_v7();
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
		let user_id = Uuid::now_v7();
		let token = auth::create_token(user_id, "charlie", TEST_SECRET, -1).unwrap();

		// Act
		let result = interceptor.validate_token(&token);

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_interceptor_requires_auth_for_log_service_path() {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/reinhardt.cloud.log.LogService/ListLogs"));

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_interceptor_accepts_user_token_for_log_service_path() {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::now_v7();
		let token = auth::create_token(user_id, "log-reader", TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/reinhardt.cloud.log.LogService/ListLogs"));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req).unwrap();

		// Assert
		let claims = result.extensions().get::<Claims>().unwrap();
		assert_eq!(claims.sub, user_id.to_string());
		assert_eq!(claims.username, "log-reader");
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
		let user_id = Uuid::now_v7();
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		// Use "Token" prefix instead of "Bearer"
		req.metadata_mut()
			.insert("authorization", format!("Token {token}").parse().unwrap());

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
		req.metadata_mut()
			.insert("authorization", "Bearer ".parse().unwrap());

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
		let user_id = Uuid::now_v7();
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
		req2.metadata_mut()
			.insert("authorization", "Bearer bad-token".parse().unwrap());
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
		let user_id = Uuid::now_v7();
		// Token created with the original secret won't match empty secret
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();

		// Act
		let result = interceptor.validate_token(&token);

		// Assert
		assert!(result.is_err());
	}

	// --- AgentJwtInterceptor tests ---

	#[rstest]
	fn test_agent_interceptor_validates_agent_token() {
		// Arrange
		let interceptor = AgentJwtInterceptor::new(TEST_SECRET);
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();

		// Act
		let claims = interceptor.validate_token(&token).unwrap();

		// Assert
		assert_eq!(claims.cluster_id, cluster_id.to_string());
		assert_eq!(claims.sub, cluster_id.to_string());
	}

	#[rstest]
	fn test_agent_interceptor_rejects_user_token() {
		// Arrange — create a regular user token, NOT an agent token
		let interceptor = AgentJwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::now_v7();
		let user_token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();

		// Act — user token lacks `cluster_id` claim
		let result = interceptor.validate_token(&user_token);

		// Assert — must fail because `cluster_id` is missing/empty
		assert!(result.is_err());
		assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_agent_interceptor_rejects_invalid_token() {
		// Arrange
		let interceptor = AgentJwtInterceptor::new(TEST_SECRET);

		// Act
		let result = interceptor.validate_token("not-a-real-token");

		// Assert
		assert!(result.is_err());
	}

	#[rstest]
	fn test_agent_interceptor_calls_stateful_token_validator() {
		// Arrange
		let interceptor =
			AgentJwtInterceptor::new(TEST_SECRET).with_token_validator(|token, claims| {
				assert!(!token.is_empty());
				if claims.cluster_id == claims.sub {
					Ok(())
				} else {
					Err(Status::unauthenticated("cluster mismatch"))
				}
			});
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();

		// Act
		let claims = interceptor.validate_token(&token).unwrap();

		// Assert
		assert_eq!(claims.cluster_id, cluster_id.to_string());
	}

	#[rstest]
	fn test_agent_interceptor_rejects_validator_failure() {
		// Arrange
		let interceptor = AgentJwtInterceptor::new(TEST_SECRET)
			.with_token_validator(|_, _| Err(Status::unauthenticated("revoked token")));
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();

		// Act
		let result = interceptor.validate_token(&token);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
		assert_eq!(err.message(), "revoked token");
	}

	#[rstest]
	#[case("AgentStream")]
	#[case("ReportHealth")]
	#[case("ReportDeployStatus")]
	fn test_agent_interceptor_call_missing_auth_on_agent_path(#[case] method: &'static str) {
		// Arrange
		let mut interceptor = AgentJwtInterceptor::new(TEST_SECRET);
		let mut req = Request::new(());
		req.extensions_mut().insert(GrpcPath::new(format!(
			"/reinhardt.cloud.cluster_agent.AgentService/{method}"
		)));

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_agent_interceptor_accepts_valid_agent_call() {
		// Arrange
		let mut interceptor = AgentJwtInterceptor::new(TEST_SECRET);
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut().insert(GrpcPath::new(
			"/reinhardt.cloud.cluster_agent.AgentService/AgentStream",
		));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req).unwrap();

		// Assert — claims should be injected into extensions
		let claims = result.extensions().get::<AgentClaims>().unwrap();
		assert_eq!(claims.cluster_id, cluster_id.to_string());
	}

	#[rstest]
	fn test_agent_interceptor_rejects_non_agent_path() {
		// Arrange — even a valid agent token must not authorize a non-agent path
		let mut interceptor = AgentJwtInterceptor::new(TEST_SECRET);
		let token = create_agent_token(Uuid::now_v7(), TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/some.other.Service/Method"));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
		assert_eq!(
			err.message(),
			"Not an agent path; cannot authorize request with the agent interceptor"
		);
	}

	#[rstest]
	fn test_log_service_interceptor_accepts_agent_token_for_push_logs() {
		// Arrange
		let mut interceptor = LogServiceJwtInterceptor::new(TEST_SECRET);
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/reinhardt.cloud.log.LogService/PushLogs"));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req).unwrap();

		// Assert
		let claims = result.extensions().get::<AgentClaims>().unwrap();
		assert_eq!(claims.cluster_id, cluster_id.to_string());
	}

	#[rstest]
	fn test_log_service_interceptor_rejects_user_token_for_push_logs() {
		// Arrange
		let mut interceptor = LogServiceJwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::now_v7();
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/reinhardt.cloud.log.LogService/PushLogs"));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req);

		// Assert
		assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
	}

	#[rstest]
	fn test_log_service_interceptor_accepts_user_token_for_list_logs() {
		// Arrange
		let mut interceptor = LogServiceJwtInterceptor::new(TEST_SECRET);
		let user_id = Uuid::now_v7();
		let token = auth::create_token(user_id, "alice", TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/reinhardt.cloud.log.LogService/ListLogs"));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req).unwrap();

		// Assert
		let claims = result.extensions().get::<Claims>().unwrap();
		assert_eq!(claims.sub, user_id.to_string());
	}

	#[rstest]
	fn test_log_service_interceptor_rejects_agent_token_for_list_logs() {
		// Arrange
		let mut interceptor = LogServiceJwtInterceptor::new(TEST_SECRET);
		let cluster_id = Uuid::now_v7();
		let token = create_agent_token(cluster_id, TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.extensions_mut()
			.insert(GrpcPath::new("/reinhardt.cloud.log.LogService/ListLogs"));
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req);

		// Assert
		assert_eq!(result.unwrap_err().code(), tonic::Code::Unauthenticated);
	}

	// --- Path propagation tests ---

	#[rstest]
	fn test_log_service_interceptor_rejects_call_without_path() {
		// Arrange — a valid user token, but no GrpcPath extension
		let mut interceptor = LogServiceJwtInterceptor::new(TEST_SECRET);
		let token = auth::create_token(Uuid::now_v7(), "alice", TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
		assert_eq!(
			err.message(),
			"Missing gRPC path; cannot authorize log service request"
		);
	}

	#[rstest]
	fn test_agent_interceptor_rejects_call_without_path() {
		// Arrange — a valid agent token, but no GrpcPath extension
		let mut interceptor = AgentJwtInterceptor::new(TEST_SECRET);
		let token = create_agent_token(Uuid::now_v7(), TEST_SECRET, 24).unwrap();
		let mut req = Request::new(());
		req.metadata_mut()
			.insert("authorization", format!("Bearer {token}").parse().unwrap());

		// Act
		let result = interceptor.call(req);

		// Assert
		let err = result.unwrap_err();
		assert_eq!(err.code(), tonic::Code::Unauthenticated);
		assert_eq!(
			err.message(),
			"Missing gRPC path; cannot authorize agent request"
		);
	}

	#[rstest]
	#[case("/grpc.health.v1.Health/Check")]
	#[case("/grpc.reflection.v1.ServerReflection/ServerReflectionInfo")]
	fn test_user_interceptor_skips_auth_for_public_path(#[case] path: &'static str) {
		// Arrange
		let mut interceptor = JwtInterceptor::new(TEST_SECRET);
		let mut req = Request::new(());
		req.extensions_mut().insert(GrpcPath::new(path));

		// Act
		let result = interceptor.call(req);

		// Assert
		let passed = result.expect("public path must bypass authentication");
		assert_eq!(
			passed.extensions().get::<GrpcPath>(),
			Some(&GrpcPath::new(path))
		);
		assert!(passed.extensions().get::<Claims>().is_none());
		assert_eq!(passed.extensions().get::<AgentClaims>(), None);
	}

	/// Inner service that echoes the recorded path.
	#[derive(Clone)]
	struct EchoPath;

	impl Service<http::Request<()>> for EchoPath {
		type Response = Option<GrpcPath>;
		type Error = std::convert::Infallible;
		type Future = std::future::Ready<Result<Self::Response, Self::Error>>;

		fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
			Poll::Ready(Ok(()))
		}

		fn call(&mut self, request: http::Request<()>) -> Self::Future {
			std::future::ready(Ok(request.extensions().get::<GrpcPath>().cloned()))
		}
	}

	#[rstest]
	#[tokio::test]
	async fn test_grpc_path_service_records_request_path() {
		// Arrange
		let mut service = GrpcPathService::new(EchoPath);
		let request = http::Request::builder()
			.uri("http://localhost:50051/reinhardt.cloud.log.LogService/PushLogs?x=1")
			.body(())
			.unwrap();

		// Act
		let recorded = service.call(request).await.unwrap();

		// Assert
		assert_eq!(
			recorded,
			Some(GrpcPath::new("/reinhardt.cloud.log.LogService/PushLogs"))
		);
	}
}
