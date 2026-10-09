//! Installed application registry for the Control Plane.
//!
//! Each entry maps an application module to its label. `reinhardt-admin startapp`
//! appends new entries.

use reinhardt::installed_apps;

installed_apps! {
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
