#![cfg(not(target_arch = "wasm32"))]

#[path = "support/fixture.rs"]
mod fixture;

#[path = "integration/cluster_deployment_identity_organization_project_secret_source_observability/test_schema_replay.rs"]
mod test_schema_replay;
