//! Telemetry primitives for reinhardt-cloud.
//!
//! This crate hosts the log schema used by the operator, CLI, and dashboard.
//!
//! # Planned Features
//!
//! - `LogService` trait + in-memory and Loki-backed implementations (Issue #373).
//! - Tracing / OTel integration (Issue #374).

mod log_service;
mod proto_convert;
mod schema;
mod tracing_init;

pub use log_service::{
	LogFilter, LogService, LogServiceError, Pagination, RetentionPolicy,
	in_memory::InMemoryLogService, loki::LokiLogService,
};
pub use proto_convert::{log_entry_to_record, log_record_to_entry};
pub use schema::{LogFields, LogLevel, LogRecord};
pub use tracing_init::{
	TraceContext, TraceContextLogLayer, TracingConfig, TracingGuard, current_trace_context,
	init_tracing,
};
