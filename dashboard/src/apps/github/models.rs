//! ORM models for the GitHub App integration.

pub mod installation;
pub mod repository;

pub use installation::GitHubInstallation;
pub use repository::GitHubRepository;
