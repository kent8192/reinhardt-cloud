//! Installed app registry for cloud_dashboard.
//!
//! `reinhardt-admin startapp` automatically appends new apps here.

use reinhardt::installed_apps;

installed_apps! {
	identity: "identity",
	organization: "organization",
	project: "project",
	source: "source",
	deployment: "deployment",
	cluster: "cluster",
	secret: "secret",
	observability: "observability",
}
