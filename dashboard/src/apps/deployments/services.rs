//! Deployment service layer.

pub mod agent;
pub mod manifest;
pub mod preview_status;
pub mod submission;

pub use agent::{cluster_uuid_from_pk, send_project_apply_to_cluster, validate_cluster_for_apply};
pub use manifest::validate_project_manifest;
pub use submission::{
	SubmitProjectDeploymentError, SubmitProjectDeploymentInput, submit_project_deployment,
	validate_submission_cluster, validate_submission_manifest,
};
