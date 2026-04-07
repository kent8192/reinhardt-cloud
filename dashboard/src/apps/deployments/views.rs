//! View functions for deployment endpoints.

pub mod create_deployment;
pub mod delete_deployment;
pub mod deployment_logs;
pub mod deployment_status;
pub mod list_deployments;
pub mod retrieve_deployment;
pub mod update_deployment;

pub use create_deployment::create_deployment;
pub use delete_deployment::delete_deployment;
pub use deployment_logs::deployment_logs;
pub use deployment_status::deployment_status;
pub use list_deployments::list_deployments;
pub use retrieve_deployment::retrieve_deployment;
pub use update_deployment::update_deployment;
