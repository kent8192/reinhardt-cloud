# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-rc.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-agent@v0.1.0-rc.1) - 2026-06-21

### Added

- *(agent)* scaffold reinhardt-cloud-agent binary crate
- *(agent)* implement rollback, scale, and restart command handlers
- *(agent)* add dockerfile and ghcr publishing job
- *(agent)* apply ReinhardtApp manifests from commands

### Changed

- [**breaking**] rename user-facing app concepts to project

### Documentation

- add crate-level READMEs for cli, operator, agent

### Fixed

- *(ci)* implement K8s deploy handler and fix formatting
- address second round of review feedback
- *(agent)* correct rollback, patch types, annotation key, and error logging
- *(agent)* guard against missing pod template and int32 overflow in commands
- *(deps)* add explicit rustls-tls feature and ring crypto provider to kube dependency
- *(operator)* add explicit CryptoProvider installation at startup
- *(dashboard)* preserve project rename migrations

### Maintenance

- migrate UUID v4 to v7
- merge main into agent security branch
- *(release)* publish crates through release-plz

### Security

- *(agent)* reject legacy direct deploy commands

### Styling

- apply cargo fmt formatting to all test files
- *(agent)* apply reinhardt-admin fmt-all formatting
- apply fmt and clippy fixes
