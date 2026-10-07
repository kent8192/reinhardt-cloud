#![cfg(not(target_arch = "wasm32"))]

#[path = "support/fixture.rs"]
mod fixture;

#[path = "integration/identity_organization_project/test_password_sessions.rs"]
mod test_password_sessions;
