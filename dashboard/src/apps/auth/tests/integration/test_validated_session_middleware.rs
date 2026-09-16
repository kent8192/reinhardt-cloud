//! Integration tests for database-backed cookie-session authentication.

#[cfg(test)]
mod tests {
	use std::sync::{Arc, Mutex};

	use reinhardt::async_trait::async_trait;
	use reinhardt::db::orm::{Model, get_connection};
	use reinhardt::di::{InjectionContext, SingletonScope};
	use reinhardt::http::{AuthState, IsActive, IsAdmin, IsAuthenticated};
	use reinhardt::test::fixtures::postgres_with_migrations_from_dir;
	use reinhardt::test::fixtures::{ContainerAsync, GenericImage, MigrationDatabase};
	use reinhardt::{Handler, Middleware, MiddlewareChain, Request, Response};
	use rstest::{fixture, rstest};
	use serial_test::serial;

	use crate::apps::auth::middleware::api_token::ApiTokenAuthMiddleware;
	use crate::apps::auth::middleware::validated_session::ValidatedSessionAuthMiddleware;
	use crate::apps::auth::models::User;
	use crate::apps::auth::services::api_key::generate_api_key;

	type CapturedAuthState = (
		Option<AuthState>,
		Option<String>,
		Option<IsAuthenticated>,
		Option<IsAdmin>,
		Option<IsActive>,
	);

	struct CaptureAuthState(Arc<Mutex<Option<CapturedAuthState>>>);

	#[async_trait]
	impl Handler for CaptureAuthState {
		async fn handle(&self, request: Request) -> reinhardt::core::exception::Result<Response> {
			*self.0.lock().expect("capture lock should remain available") = Some((
				request.extensions.get::<AuthState>(),
				request.extensions.get::<String>(),
				request.extensions.get::<IsAuthenticated>(),
				request.extensions.get::<IsAdmin>(),
				request.extensions.get::<IsActive>(),
			));
			Ok(Response::ok())
		}
	}

	#[fixture]
	async fn db() -> (ContainerAsync<GenericImage>, MigrationDatabase) {
		let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
		postgres_with_migrations_from_dir(&migrations_dir)
			.await
			.expect("Failed to start PostgreSQL with migrations")
	}

	async fn create_test_user(username: &str, is_active: bool, is_staff: bool) -> User {
		let user = User::build()
			.username(username.to_string())
			.email(format!("{username}@example.test"))
			.first_name(String::new())
			.last_name(String::new())
			.password_hash(None)
			.is_active(is_active)
			.is_staff(is_staff)
			.is_superuser(false)
			.finish();
		User::objects()
			.create(&user)
			.await
			.expect("Failed to create user")
	}

	async fn request_after_cookie_auth(user: &User) -> Request {
		let mut request = Request::builder()
			.uri("/api/resource")
			.body(Vec::new().into())
			.build()
			.expect("request should build");
		let connection = get_connection()
			.await
			.expect("migration fixture should initialize the ORM connection");
		let scope = Arc::new(SingletonScope::new());
		scope.set(connection);
		request.set_di_context(Arc::new(InjectionContext::builder(scope).build()));
		request.extensions.insert(user.id.to_string());
		request.extensions.insert(IsAuthenticated(true));
		request.extensions.insert(IsAdmin(false));
		request.extensions.insert(IsActive(true));
		request
			.extensions
			.insert(AuthState::authenticated(user.id.to_string(), false, true));
		request
	}

	async fn captured_after(
		middleware: impl Middleware + 'static,
		request: Request,
	) -> CapturedAuthState {
		let captured = Arc::new(Mutex::new(None));
		let handler = Arc::new(CaptureAuthState(Arc::clone(&captured)));

		middleware
			.process(request, handler)
			.await
			.expect("authentication middleware should continue");

		captured
			.lock()
			.expect("capture lock should remain available")
			.clone()
			.expect("capture handler should receive the request")
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn active_cookie_session_uses_current_database_privileges(
		#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
	) {
		// Arrange
		let (_container, _connection) = db.await;
		let user = create_test_user("validated-session-active", true, true).await;
		let request = request_after_cookie_auth(&user).await;

		// Act
		let (state, user_id, authenticated, admin, active) =
			captured_after(ValidatedSessionAuthMiddleware, request).await;

		// Assert
		assert_eq!(
			state,
			Some(AuthState::authenticated(user.id.to_string(), true, true))
		);
		assert_eq!(user_id, Some(user.id.to_string()));
		assert_eq!(authenticated, Some(IsAuthenticated(true)));
		assert_eq!(admin, Some(IsAdmin(true)));
		assert_eq!(active, Some(IsActive(true)));
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn inactive_cookie_session_user_becomes_anonymous(
		#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
	) {
		// Arrange
		let (_container, _connection) = db.await;
		let user = create_test_user("validated-session-inactive", false, false).await;
		let request = request_after_cookie_auth(&user).await;

		// Act
		let (state, user_id, authenticated, admin, active) =
			captured_after(ValidatedSessionAuthMiddleware, request).await;

		// Assert
		assert_eq!(state, Some(AuthState::anonymous()));
		assert_eq!(user_id, None);
		assert_eq!(authenticated, Some(IsAuthenticated(false)));
		assert_eq!(admin, Some(IsAdmin(false)));
		assert_eq!(active, Some(IsActive(false)));
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn deleted_cookie_session_user_becomes_anonymous(
		#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
	) {
		// Arrange
		let (_container, _connection) = db.await;
		let user = create_test_user("validated-session-deleted", true, false).await;
		User::objects()
			.delete(user.id)
			.await
			.expect("Failed to delete user");
		let request = request_after_cookie_auth(&user).await;

		// Act
		let (state, user_id, authenticated, admin, active) =
			captured_after(ValidatedSessionAuthMiddleware, request).await;

		// Assert
		assert_eq!(state, Some(AuthState::anonymous()));
		assert_eq!(user_id, None);
		assert_eq!(authenticated, Some(IsAuthenticated(false)));
		assert_eq!(admin, Some(IsAdmin(false)));
		assert_eq!(active, Some(IsActive(false)));
	}

	#[rstest]
	#[tokio::test(flavor = "multi_thread")]
	#[serial(database)]
	async fn valid_bearer_token_replaces_validated_cookie_session(
		#[future] db: (ContainerAsync<GenericImage>, MigrationDatabase),
	) {
		// Arrange
		let (_container, _connection) = db.await;
		let session_user = create_test_user("validated-session-cookie", true, false).await;
		let bearer_user = create_test_user("validated-session-bearer", true, false).await;
		let (plaintext, _api_key) = generate_api_key(bearer_user.id, "priority".to_string(), None)
			.await
			.expect("API key should be generated");
		let mut request = request_after_cookie_auth(&session_user).await;
		request.headers.insert(
			"Authorization",
			format!("Bearer {plaintext}")
				.parse()
				.expect("Authorization header should be valid"),
		);
		let captured = Arc::new(Mutex::new(None));
		let chain = MiddlewareChain::new(Arc::new(CaptureAuthState(Arc::clone(&captured))))
			.with_middleware(Arc::new(ValidatedSessionAuthMiddleware))
			.with_middleware(Arc::new(ApiTokenAuthMiddleware));

		// Act
		chain
			.handle(request)
			.await
			.expect("authentication middleware chain should continue");
		let (state, user_id, authenticated, admin, active) = captured
			.lock()
			.expect("capture lock should remain available")
			.clone()
			.expect("capture handler should receive the request");

		// Assert
		assert_eq!(
			state,
			Some(AuthState::authenticated(
				bearer_user.id.to_string(),
				false,
				true
			))
		);
		assert_eq!(user_id, Some(bearer_user.id.to_string()));
		assert_eq!(authenticated, Some(IsAuthenticated(true)));
		assert_eq!(admin, Some(IsAdmin(false)));
		assert_eq!(active, Some(IsActive(true)));
	}
}
