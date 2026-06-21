# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-proto@v0.1.0) - 2026-06-21

### Added

- *(proto)* create reinhardt-cloud-proto crate with gRPC service definitions
- *(dashboard)* add dual-protocol HTTP/gRPC server startup
- *(proto)* add PluginService proto definitions
- *(agent)* implement rollback, scale, and restart command handlers
- *(log)* add deployment_id to LogFilter across proto, types, and core
- *(agent)* apply ReinhardtApp manifests from commands

### Changed

- *(log)* address Copilot review on deployment_id filter
- [**breaking**] rename user-facing app concepts to project

### Maintenance

- *(release)* publish crates through release-plz

### Security

- scope app log queries by tenant namespace

### Styling

- apply cargo fmt formatting to all test files
