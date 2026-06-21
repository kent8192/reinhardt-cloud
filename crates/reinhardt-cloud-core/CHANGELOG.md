# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-core@v0.1.0-alpha.1) - 2026-06-21

### Added

- *(core)* define async service traits for auth, build, cluster, and log
- *(core)* add mock service implementations and gRPC test harness
- *(core)* implement LocalBuildService with streaming and cancellation
- *(core)* implement in-memory log ring buffer with filtering
- *(core)* implement LocalLogService backed by ring buffer
- *(core)* implement plugin traits and registry
- *(grpc)* implement report_deploy_status processing in agent service
- *(log)* add deployment_id to LogFilter across proto, types, and core
- *(core)* derive infrastructure specs

### Changed

- rename crate directories from nuages to reinhardt-cloud
- update Cargo.toml package names from nuages to reinhardt-cloud
- update Rust imports and identifiers from nuages to reinhardt-cloud
- *(log)* address Copilot review on deployment_id filter
- [**breaking**] rename user-facing app concepts to project
- align variable and field names with Project terminology

### Fixed

- resolve all clippy warnings from grpc feature branch
- *(ci)* implement K8s deploy handler and fix formatting
- address second round of review feedback
- *(core)* add default implementation for report_deploy_status trait method
- *(core)* use correct ApiError::Internal variant capitalization
- *(grpc)* use appropriate error for unimplemented default and Display for BuildPhase
- *(grpc)* bind agent to authenticated cluster_id from JWT claims
- *(dashboard)* use facade auth settings

### Maintenance

- migrate UUID v4 to v7
- merge main into PR [[#755](https://github.com/kent8192/reinhardt-cloud/issues/755)](https://github.com/kent8192/reinhardt-cloud/issues/755)
- *(release)* publish crates through release-plz

### Security

- *(dashboard)* constrain deployment log filters

### Styling

- apply cargo fmt formatting to all test files

### Testing

- add workspace test dependencies for comprehensive test coverage
- add comprehensive test suite for reinhardt-cloud-core services
- *(core)* cover infrastructure validation errors
