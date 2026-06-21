# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-rc.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-cli@v0.1.0-rc.1) - 2026-06-21

### Added

- *(cli)* add credentials subcommand for Git/registry Secret management
- *(cli)* add pages and graphql fields to InfraSignals
- *(cli)* add Dockerfile auto-generation module
- *(cli)* integrate Dockerfile generation into init and sync commands
- *(cli)* add crd_version module for runtime apiVersion discovery
- *(cli)* add --api-version flag and resolve apiVersion at deploy time
- *(cli)* add terraform generate subcommand
- *(cli/dockerfile-generator)* detect prost/tonic in cargo.lock and install protoc
- *(cli)* generate infrastructure toml baseline
- *(deploy)* derive infrastructure spec
- *(sync)* preserve infrastructure config
- *(terraform)* derive infrastructure fallback
- *(e2e)* add dashboard self-deploy harness
- *(cli)* add reqwest HTTP client with typed me()/post() callers
- *(cli)* add save_token and resolve_token with flag>env>file priority
- *(cli)* restore login command with token verification and persistence

### Changed

- rename crate directories from nuages to reinhardt-cloud
- update Cargo.toml package names from nuages to reinhardt-cloud
- update Rust imports and identifiers from nuages to reinhardt-cloud
- *(cli)* unify kubectl invocation paths in deploy command
- *(dashboard)* make apps server function based
- [**breaking**] rename user-facing app concepts to project
- align variable and field names with Project terminology

### Documentation

- add crate-level READMEs for cli, operator, agent
- *(sync)* clarify infrastructure preservation

### Fixed

- *(settings)* align TOML structure with composable settings macro
- address remaining Copilot review comments on PR [[#98](https://github.com/kent8192/reinhardt-cloud/issues/98)](https://github.com/kent8192/reinhardt-cloud/issues/98)
- resolve all clippy warnings from grpc feature branch
- resolve clippy warnings across workspace
- *(cli)* sanitize credential command output for CodeQL compliance
- *(cli)* avoid taint-tracked secret field access in credential status output
- *(cli)* address Copilot review on Dockerfile generation
- *(cli)* preserve real-time kubectl output in production mode
- *(cli)* address Copilot review feedback on crd_version
- *(cli)* address review feedback on apiVersion discovery
- *(cli)* address Copilot review feedback on apiVersion discovery
- *(cli)* fix version ranking stability priority, use max_by_key, validate apiVersion whitespace
- *(ci)* suppress dead_code on DatabaseConfig and apply cargo fmt
- *(cli)* clarify kube::Client::try_default() config inference in warning
- *(cli)* install rustls CryptoProvider in kube Client test
- *(cli)* skip cluster discovery for --dry-run --direct and clarify --api-version scope
- *(cli)* resolve workspace=true and ancestor files for Dockerfile generation
- *(dockerfile-generator)* scope cargo build to project package to skip wasm-incompatible workspace members
- *(dockerfile-generator)* bundle project settings TOMLs into runtime image
- *(cli/dockerfile-generator)* install libprotobuf-dev with protobuf-compiler
- *(cli)* copy index.html into runtime image for pages projects
- *(deploy)* preserve generated ReinhardtApp specs
- *(deploy)* address Copilot deploy review feedback
- *(cli)* preserve session signal conversion
- *(cli)* derive infrastructure from effective database
- *(terraform)* validate crd infrastructure fallback
- *(terraform)* preserve auth secret refs
- *(deploy)* preserve auth secret refs
- *(cli)* align derived infrastructure names
- *(sync)* validate preserved infrastructure
- *(cli)* resolve conflicts with main
- *(cli)* address CodeRabbit infrastructure review
- *(cli)* include manage binary in generated images
- expose manage binary on runtime path
- *(cli)* require self-deploy introspection
- *(cli)* version wasm assets for immutable caching
- *(dashboard)* address server function review feedback
- *(dashboard)* preserve project rename migrations
- *(ci)* resolve credential permissions branch conflicts
- *(cli)* update credential test fixtures

### Maintenance

- merge origin/main into feat/issue-367-cli-api-version-discovery
- merge main into feature/issue-374-telemetry-tracing
- merge main into tls autoscaling branch
- merge main into PR [[#754](https://github.com/kent8192/reinhardt-cloud/issues/754)](https://github.com/kent8192/reinhardt-cloud/issues/754)

### Performance

- *(cli)* avoid String allocation in pick_best_version tie-breaker

### Security

- *(cli)* restrict credential file permissions

### Styling

- apply cargo fmt --all
- *(cli)* apply reinhardt-admin fmt-all to credentials command
- apply fmt and clippy fixes
- apply fmt-all and clippy-fix
- apply rustfmt and clippy fixes
- *(cli)* apply formatter output

### Testing

- *(cli/dockerfile-generator)* cover protoc detection from prost/tonic deps
- *(cli)* avoid default field reassignment in dockerfile tests
