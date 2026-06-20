//! Request serializers for deployment endpoints.

pub mod cli;
pub mod request;

pub use cli::{CliDeploymentRequest, CliDeploymentResponse};
pub use request::{CreateDeploymentRequest, DeploymentStatusRequest, UpdateDeploymentRequest};
