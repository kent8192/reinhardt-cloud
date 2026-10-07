#![cfg(not(target_arch = "wasm32"))]

#[path = "support/fixture.rs"]
mod fixture;

#[path = "e2e/cluster_deployment_identity_organization_project/test_authenticated_projects.rs"]
mod test_authenticated_projects;
