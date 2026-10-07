#![cfg(not(target_arch = "wasm32"))]

#[path = "support/fixture.rs"]
mod fixture;

#[path = "integration/cluster_deployment_identity_organization_project_observability/test_runtime_transactions.rs"]
mod test_runtime_transactions;
