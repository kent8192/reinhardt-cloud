//! Installed app registry for cloud_control_plane.
//!
//! `reinhardt-admin startapp` automatically appends new apps here.

use reinhardt::installed_apps;

installed_apps! {
	// Apps will be added here by `reinhardt-admin startapp`.
	accounts: "accounts",
	organizations: "organizations",
	clusters: "clusters",
	agents: "agents",
	projects: "projects",
	deployments: "deployments",
	logs: "logs",
	github: "github",
	health: "health",
}
