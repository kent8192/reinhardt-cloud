//! `manage` commands of the accounts application.
//!
//! These are the only way to do the operations that must never be reachable
//! over the network: granting or revoking Staff (SR-20, SR-105), issuing a Login
//! Link (SR-18), and moving a User to another GitHub account (SR-107).
//! `end-sessions`, `reactivate-user`, and `deactivate-user` are the activation
//! path (the User admin is read-only) and the recovery tools the fail-closed
//! paths of Staff revocation and re-pointing point to.
//!
//! Running one needs host operator access: a shell on the host (or in the
//! container) with the Control Plane's settings and secrets in its environment.
//! Each command loads the validated settings, connects to the database (and to
//! Redis when it must end sessions), does its one job through a service in
//! `services/server/`, and emits an audit event.
//!
//! The commands declare no settings views or services of their own: they
//! opt in to the command framework's capability interface only for its argument
//! parsing and help, and load the full validated settings themselves (see
//! [`runtime::CommandRuntime`]).

pub mod create_login_link;
pub mod deactivate_user;
pub mod end_sessions;
pub mod grant_staff;
pub mod reactivate_user;
pub mod repoint_github_account;
pub mod runtime;

use reinhardt::commands::CommandRegistry;

/// Register the accounts commands with `registry`.
pub fn register(registry: &mut CommandRegistry) {
	registry.register_capability(Box::new(grant_staff::GrantStaffCommand));
	registry.register_capability(Box::new(create_login_link::CreateLoginLinkCommand));
	registry.register_capability(Box::new(
		repoint_github_account::RepointGithubAccountCommand,
	));
	registry.register_capability(Box::new(end_sessions::EndSessionsCommand));
	registry.register_capability(Box::new(reactivate_user::ReactivateUserCommand));
	registry.register_capability(Box::new(deactivate_user::DeactivateUserCommand));
}
