//! E2E tests for credential management (#278).
//!
//! Test scenarios:
//! - e2e_credentials_missing: Reference non-existent Secret
//!   → Warning condition set in Status
//! - e2e_credentials_set_check: CLI credentials set → check
//!   → Secret created correctly, check reports status
