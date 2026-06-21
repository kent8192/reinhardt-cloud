# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-k8s@v0.1.0-alpha.1) - 2026-06-21

### Added

- *(github)* apply imported apps to kubernetes
- *(agent)* apply ReinhardtApp manifests from commands

### Changed

- rename crate directories from nuages to reinhardt-cloud
- update Cargo.toml package names from nuages to reinhardt-cloud
- update Rust imports and identifiers from nuages to reinhardt-cloud
- [**breaking**] rename user-facing app concepts to project

### Fixed

- *(dashboard)* preserve project rename migrations

### Maintenance

- *(release)* publish crates through release-plz
