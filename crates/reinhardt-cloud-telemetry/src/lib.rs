//! Telemetry primitives for reinhardt-cloud.
//!
//! This crate hosts the log schema, the `LogService` trait, and its default
//! implementations. Phase 3 (Issue #374) will extend it with tracing.

mod schema;

pub use schema::{LogFields, LogLevel, LogRecord};
