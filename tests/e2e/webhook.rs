//! E2E tests for webhook automation (#276).
//!
//! Test scenarios:
//! - e2e_webhook_github_push: Send GitHub push webhook payload
//!   → Signature verified → annotation updated → rebuild triggered
//! - e2e_webhook_gitlab_push: Send GitLab push webhook payload
//!   → Token verified → annotation updated → rebuild triggered
//! - e2e_webhook_invalid_signature: Send with invalid signature
//!   → 401 Unauthorized returned
