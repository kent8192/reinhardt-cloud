# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-rc.1](https://github.com/kent8192/reinhardt-cloud/releases/tag/reinhardt-cloud-operator@v0.1.0-rc.1) - 2026-06-21

### Added

- *(operator)* implement AWS RDS and GCP Cloud SQL database inference
- *(operator)* add secret-backed database env var helper
- *(operator)* wire explicit spec.database into reconciler
- *(operator)* generate dentdelion configmap and volumes from PluginSpec
- *(operator)* inherit image_pull_secrets into preview environments
- *(operator)* add per-app workload ServiceAccount builder
- *(operator)* wire ServiceAccountName and reconcile per-app KSA
- *(cli)* add terraform generate subcommand
- *(operator)* add /healthz endpoint and run HTTP server unconditionally
- *(operator)* add multi-stage Dockerfile and dockerignore
- *(operator)* use typed TOML interpolation for production.toml (refs [[#4232](https://github.com/kent8192/reinhardt-cloud/issues/4232)](https://github.com/kent8192/reinhardt-cloud/issues/4232))
- *(operator)* derive database env from settings
- *(e2e)* add dashboard self-deploy harness
- *(github)* apply imported apps to kubernetes
- *(operator)* gate rollouts on revision migrations
- *(operator)* add local cert-manager Issuer custom resource type
- *(operator)* add InvalidBudget error variant
- *(operator)* add preview namespace resource builders
- *(operator)* clamp preview replicas to budget.max_replicas
- *(operator)* add TLS section and cert-manager annotation to preview ingress
- *(operator)* add preview-to-status mapper
- *(operator)* add preview ACME/ingress-class config from env
- *(operator)* reconcile preview namespace, finalizer cleanup, and status.previews

### Changed

- rename crate directories from nuages to reinhardt-cloud
- update Cargo.toml package names from nuages to reinhardt-cloud
- update Rust imports and identifiers from nuages to reinhardt-cloud
- *(operator)* remove application-schema-specific ConfigMap emission
- [**breaking**] rename user-facing app concepts to project

### Documentation

- link deployment flow architecture from self-hosting and crate READMEs
- *(operator)* clarify HTTP server start behavior on metrics-addr parse error
- fix stale references and grammar in doc comments

### Fixed

- *(security)* replace hard-coded credentials with generated passwords
- *(security)* use black_box to suppress CodeQL false positives in tests
- address Copilot review feedback on PR [[#98](https://github.com/kent8192/reinhardt-cloud/issues/98)](https://github.com/kent8192/reinhardt-cloud/issues/98)
- address remaining Copilot review comments on PR [[#98](https://github.com/kent8192/reinhardt-cloud/issues/98)](https://github.com/kent8192/reinhardt-cloud/issues/98)
- *(inference)* address cloud database resource generation issues
- *(inference)* use engine-specific identifier limits, platform defaults, and GCP sanitization
- *(operator)* add /static/ path to Ingress when explicit ingress_host is set with pages
- *(deps)* add explicit rustls-tls feature and ring crypto provider to kube dependency
- *(operator)* add explicit CryptoProvider installation at startup
- *(operator)* use dot delimiter in owner label value to comply with K8s validation
- *(operator)* address Copilot review feedback on database inference wiring
- *(operator)* fix db inference service selector, host naming, and credential conflict
- *(ci)* box large DatabaseResource enum variants to resolve clippy::large_enum_variant
- *(crd)* validate plugin uniqueness and tighten wasm_dir checks
- *(operator)* nest debug/allowed_hosts under [core] in generated production.toml
- *(operator)* inject core.secret_key via per-app Secret + TOML interpolation
- *(operator)* emit complete ProjectSettings schema in production.toml
- *(deploy)* preserve generated ReinhardtApp specs
- *(deploy)* address Copilot deploy review feedback
- *(operator)* narrow app service selector to web pods
- *(operator)* narrow dashboard deployment selector
- *(github)* deploy built images for repository imports
- *(github)* address CodeRabbit repository import review
- *(github)* stabilize repository import review findings
- *(dashboard)* preserve project rename migrations
- *(operator)* add explanatory comments for #[allow(dead_code)] attributes
- *(operator)* drop reconcile-scoped TRACEPARENT from workload env
- *(operator)* address preview review feedback
- *(ci)* import kube resource trait for preview cleanup
- *(ci)* align migration service account tests
- *(operator)* harden service account cleanup ownership

### Maintenance

- *(operator)* drop dead-code attrs on now-used inference types
- merge origin/main into feat/issues-365-to-366-operator-observability
- merge main into feature/issue-374-telemetry-tracing
- *(operator)* fix clippy::unnecessary_get_then_check in tenant.rs
- merge main into preview hardening branch
- merge main into preview dashboard branch
- merge main into PR [[#759](https://github.com/kent8192/reinhardt-cloud/issues/759)](https://github.com/kent8192/reinhardt-cloud/issues/759)
- merge main into PR [[#759](https://github.com/kent8192/reinhardt-cloud/issues/759)](https://github.com/kent8192/reinhardt-cloud/issues/759)

### Other

- resolve conflicts with feature/platform-basis (6277f17)
- resolve storage.rs conflict with origin/feature/platform-basis

### Security

- *(operator)* guard preview namespace cleanup by owner labels

### Styling

- *(operator)* fix formatting in deployment resource tests
- apply cargo fmt to labels.rs and manage.rs
- apply fmt and clippy fixes
- apply cargo fmt
- apply cargo fmt to operator files
- *(operator)* remove extraneous blank line in main.rs
- *(plugins)* apply rustfmt to new plugin modules
- *(operator)* apply fmt-fix to load_emitted_toml signature

### Testing

- *(operator)* factor out parse_production_toml helper in configmap tests
- *(operator)* assert typed TOML interpolation round-trips at deserialize (refs [[#4232](https://github.com/kent8192/reinhardt-cloud/issues/4232)](https://github.com/kent8192/reinhardt-cloud/issues/4232))
- *(operator)* lock manage init container commands
- *(operator)* tighten migration revision assertions
