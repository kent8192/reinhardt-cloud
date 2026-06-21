# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-rc.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-types@v0.1.0-rc.1) - 2026-06-21

### Added

- *(types)* add build, agent, and log domain types
- *(grpc)* implement report_deploy_status processing in agent service
- *(types)* add base_image field to BuildSection
- *(log)* add deployment_id to LogFilter across proto, types, and core
- *(crd)* add PluginSpec types to reinhardtapp crd
- *(types)* add spec.imagePullSecrets to ReinhardtApp CRD
- *(crd)* add InfrastructureSpec for per-app infrastructure declaration
- *(types)* persist infrastructure in cloud toml
- *(agent)* apply ReinhardtApp manifests from commands
- *(operator)* gate rollouts on revision migrations
- *(types)* add PreviewBudget and PreviewSpec.budget
- *(types)* add PreviewStatus and ProjectStatus.previews

### Changed

- rename crate directories from nuages to reinhardt-cloud
- update Cargo.toml package names from nuages to reinhardt-cloud
- update Rust imports and identifiers from nuages to reinhardt-cloud
- [**breaking**] rename user-facing app concepts to project

### Documentation

- *(types)* document infrastructure toml section
- fix stale references and grammar in doc comments

### Fixed

- *(operator)* address Copilot review comments on isolation module
- address PR [[#7](https://github.com/kent8192/reinhardt-cloud/issues/7)](https://github.com/kent8192/reinhardt-cloud/issues/7) review comments
- resolve all clippy warnings from grpc feature branch
- *(ci)* implement K8s deploy handler and fix formatting
- *(grpc)* use appropriate error for unimplemented default and Display for BuildPhase
- *(crd)* validate plugin uniqueness and tighten wasm_dir checks
- *(terraform)* fix validate errors and add .gitignore patterns
- *(github)* address CodeRabbit repository import review
- *(agent)* reject redacted secret deserialization
- *(dashboard)* preserve project rename migrations
- *(types)* serialize PreviewStatus fields as camelCase
- *(operator)* address preview review feedback
- *(ci)* merge main into loki logging branch

### Maintenance

- migrate UUID v4 to v7
- merge main into preview hardening branch
- merge main into PR [[#758](https://github.com/kent8192/reinhardt-cloud/issues/758)](https://github.com/kent8192/reinhardt-cloud/issues/758)
- *(release)* publish crates through release-plz

### Other

- resolve conflicts with feature/platform-basis rename
- resolve storage.rs conflict with origin/feature/platform-basis

### Security

- *(operator)* restrict source build credentials secret

### Styling

- apply cargo fmt formatting to all test files
- *(plugins)* apply rustfmt to new plugin modules
- apply rustfmt and clippy fixes

### Testing

- add workspace test dependencies for comprehensive test coverage
- add edge case, boundary, property-based, and fuzz tests for domain types
