//! Telemetry primitives for reinhardt-cloud.
//!
//! This crate hosts the log schema used by the operator, CLI, and dashboard.
//!
//! # Planned Features
//!
//! - `LogService` trait + in-memory and Loki-backed implementations (Issue #373).
//! - Tracing / OTel integration (Issue #374).

mod schema;

pub use schema::{LogFields, LogLevel, LogRecord};
