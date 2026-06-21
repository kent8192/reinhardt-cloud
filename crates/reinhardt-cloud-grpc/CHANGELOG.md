# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-grpc@v0.1.0-alpha.1) - 2026-06-21

### Added

- *(grpc)* scaffold reinhardt-cloud-grpc crate with JWT interceptor and health
- *(core)* add mock service implementations and gRPC test harness
- *(grpc)* implement BuildService gRPC server and client
- *(grpc)* implement AgentRegistry for cluster agent management
- *(grpc)* implement AgentService and LogService gRPC servers
- *(grpc)* add SSE adapter for CLI streaming
- *(log)* add deployment_id to LogFilter across proto, types, and core
- *(grpc)* add agent jwt claims and agent authentication interceptor
- *(grpc)* add cluster-scoped agent command routing to AgentRegistry
- *(grpc)* replace MockClusterAgentService with registry-backed implementation
- *(agent)* apply ReinhardtApp manifests from commands

### Changed

- *(dashboard)* make apps server function based
- [**breaking**] rename user-facing app concepts to project

### Fixed

- resolve all clippy warnings from grpc feature branch
- *(ci)* implement K8s deploy handler and fix formatting
- *(ci)* resolve TODO check and address review feedback
- *(test)* update health check test to expect NOT_SERVING
- address second round of review feedback
- *(grpc)* re-export HealthReporter to avoid direct tonic_health dependency in dashboard
- *(grpc)* bind agent to authenticated cluster_id from JWT claims
- *(dashboard)* use facade auth settings
- *(github)* address CodeRabbit repository import review
- *(dashboard)* preserve project rename migrations
- *(ci)* include namespace in grpc log filter tests
- *(ci)* merge main into loki logging branch

### Maintenance

- migrate UUID v4 to v7
- *(release)* publish crates through release-plz

### Security

- scope app log queries by tenant namespace

### Styling

- apply cargo fmt formatting to all test files
- apply cargo fmt to agent auth changes

### Testing

- add workspace test dependencies for comprehensive test coverage
- add proto conversion, interceptor, and registry tests for grpc crate
