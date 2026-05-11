//! Tests for session service module structure.
//!
//! The full session lifecycle (`SessionService::create_session`,
//! `destroy_session`, `validate_session`) requires a running Redis
//! instance and is covered by integration tests. These unit tests
//! verify module-level invariants that don't require external services.

#[cfg(test)]
mod tests {
	use rstest::rstest;

	/// The session service public API is accessible from the services module.
	#[rstest]
	fn test_session_service_exports_are_accessible() {
		// Verify that the public API of the session service module is importable.
		// `SessionService` is resolved via the DI factory at runtime; here we only
		// check that the type and the remaining free-function adapter
		// (`validate_session`, used by the WebSocket consumer) are reachable as
		// symbols.
		let _service: fn(_) -> _ =
			crate::apps::auth::services::session::SessionService::from_backend;
		let _validate = crate::apps::auth::services::validate_session;
	}
}
