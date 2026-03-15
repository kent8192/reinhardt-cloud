//! View functions for deployment endpoints.

pub mod create_deployment;
pub mod list_deployments;

pub use create_deployment::create_deployment;
pub use list_deployments::list_deployments;
