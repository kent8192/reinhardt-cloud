# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-telemetry@v0.1.0-alpha.1) - 2026-06-21

### Added

- *(telemetry)* scaffold reinhardt-cloud-telemetry crate with LogRecord schema
- *(telemetry)* add LogService trait, LogFilter, Pagination, RetentionPolicy
- *(telemetry)* add InMemoryLogService with capacity+TTL retention
- *(telemetry)* add LokiLogService read-only stub
- *(telemetry)* add LogRecord<->proto LogEntry conversion
- *(telemetry)* implement loki query_range list path and logql builder
- *(telemetry)* implement loki /tail websocket stream and add integration tests

### Changed

- *(telemetry)* use workspace chrono and drop unused tempfile dep
- [**breaking**] rename user-facing app concepts to project

### Documentation

- *(telemetry)* clarify TraceContextLogLayer stores IDs in extensions only
- *(telemetry)* fix doc inaccuracies and remove unused anyhow dependency

### Fixed

- *(telemetry)* honor LogFilter.deployment_id in InMemoryLogService
- *(telemetry)* address Copilot review feedback on log schema and RBAC
- *(telemetry)* address Copilot review issues in reinhardt-cloud-telemetry
- *(telemetry)* address Copilot review on OTel tracing integration
- *(ci)* suppress dead_code lint on unused host field in DatabaseConfig
- *(telemetry)* add flatten_event(true) to JSON fmt layer
- *(dashboard)* address observability review gaps
- *(observability)* harden Loki app log review paths
- *(observability)* placate CodeQL timestamp field use
- *(ci)* align log namespace test fixtures

### Maintenance

- merge origin/main into feature/issue-374-telemetry-tracing
- *(telemetry)* add core/types/tokio-tungstenite deps for loki read path
- *(release)* publish crates through release-plz

### Security

- scope Loki app logs by tenant namespace

### Styling

- *(telemetry)* fix rustfmt formatting in trace_context_layer test
- *(telemetry)* apply rustfmt to tracing_init.rs
- apply reinhardt-formatter to issue-708 changes

### Testing

- *(telemetry)* cover full LogFields roundtrip and nanosecond preservation
- *(telemetry)* rewrite trace_context_layer test to verify extension attachment
