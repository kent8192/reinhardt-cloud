//! E2E tests for preview environments (#277).
//!
//! Test scenarios:
//! - e2e_preview_lifecycle: PR open → sync → close full lifecycle
//!   → Preview ReinhardtApp created → updated → deleted
//! - e2e_preview_ttl_expiry: Preview with expired TTL → Auto-deleted
//! - e2e_preview_owner_cascade: Delete parent ReinhardtApp
//!   → Preview cascade-deleted via ownerReferences
