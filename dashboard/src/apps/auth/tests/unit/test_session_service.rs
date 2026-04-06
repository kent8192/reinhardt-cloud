//! Tests for session service module structure.
//!
//! The session service functions (`create_session`, `destroy_session`,
//! `validate_session`) require a running Redis instance and are tested
//! in integration tests. These unit tests verify module-level invariants
//! that don't require external services.

#[cfg(test)]
mod tests {
    use rstest::rstest;

    /// The session service re-exports are accessible from the services module.
    #[rstest]
    fn test_session_service_exports_are_accessible() {
        // Verify that the public API of the session service module is importable.
        // The functions are async and require Redis; we verify they exist as symbols
        // by referencing them without calling.
        let _create = crate::apps::auth::services::create_session;
        let _validate = crate::apps::auth::services::validate_session;
        let _destroy = crate::apps::auth::services::destroy_session;
    }
}
