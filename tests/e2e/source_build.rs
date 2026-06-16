//! E2E tests for source build & deploy (#275).
//!
//! Test scenarios:
//! - e2e_source_build_deploy: Deploy Project with spec.source only
//!   → kaniko Job created → build completes → Deployment image updated
//! - e2e_image_precedence: Set both spec.image + spec.source
//!   → Source build skipped, spec.image used
