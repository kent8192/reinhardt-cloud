//! English message catalog.
//!
//! The first element of each pair is the message ID used with `t!`; the second
//! is its English rendering. Keep the list sorted by feature so a missing
//! entry is easy to spot; the `every_t_message_has_an_english_entry` test in
//! `tests/i18n.rs` fails when a `t!` literal in `src/` has no entry here.

/// Message ID and English rendering pairs.
pub const MESSAGES: &[(&str, &str)] = &[
	// Product
	("Reinhardt Cloud", "Reinhardt Cloud"),
	// Signed-out layout
	("Skip to main content", "Skip to main content"),
	// Theme toggle
	("Switch to dark theme", "Switch to dark theme"),
	("Switch to light theme", "Switch to light theme"),
	// Code block
	("Copy", "Copy"),
	("Copied", "Copied"),
	// Sign-in page
	(
		"Run your Reinhardt Projects on your own Clusters.",
		"Run your Reinhardt Projects on your own Clusters.",
	),
	(
		"Register a Kubernetes Cluster, connect a GitHub repository, and every push becomes a Deployment you can watch.",
		"Register a Kubernetes Cluster, connect a GitHub repository, and every push becomes a Deployment you can watch.",
	),
	("Continue with GitHub", "Continue with GitHub"),
	(
		"GitHub is the only way to sign in. There are no passwords to set or reset.",
		"GitHub is the only way to sign in. There are no passwords to set or reset.",
	),
	(
		"No Invitation found for @{login}",
		"No Invitation found for @{login}",
	),
	(
		"This Control Plane accepts new Users by Invitation only. Ask an owner of your Organization to send an Invitation to your GitHub account, then sign in again.",
		"This Control Plane accepts new Users by Invitation only. Ask an owner of your Organization to send an Invitation to your GitHub account, then sign in again.",
	),
	("Sign-in did not complete", "Sign-in did not complete"),
	(
		"GitHub did not complete the sign-in. Try again, and ask Staff of this Control Plane if it keeps happening.",
		"GitHub did not complete the sign-in. Try again, and ask Staff of this Control Plane if it keeps happening.",
	),
	// Login Link page
	("Sign in with a Login Link", "Sign in with a Login Link"),
	(
		"A host operator of this Control Plane issued this link to sign you in. It works once and expires soon. Press the button to use it now.",
		"A host operator of this Control Plane issued this link to sign you in. It works once and expires soon. Press the button to use it now.",
	),
	("Sign in", "Sign in"),
	(
		"This sign-in link did not work",
		"This sign-in link did not work",
	),
	(
		"The link may already have been used, may have expired, or may be incomplete. Ask the operator who sent it to issue a new one.",
		"The link may already have been used, may have expired, or may be incomplete. Ask the operator who sent it to issue a new one.",
	),
	// Signed-in frame
	("Sidebar", "Sidebar"),
	("Primary", "Primary"),
	("Overview", "Overview"),
	("User menu", "User menu"),
	("Account", "Account"),
	("@{login}", "@{login}"),
	("Sign out", "Sign out"),
	("Sign-out did not complete", "Sign-out did not complete"),
	(
		"Your session may still be active. Try again.",
		"Your session may still be active. Try again.",
	),
	(
		"The Dashboard could not load your account",
		"The Dashboard could not load your account",
	),
	(
		"Reload the page. If it keeps happening, ask Staff of this Control Plane.",
		"Reload the page. If it keeps happening, ask Staff of this Control Plane.",
	),
	// Overview page
	("Welcome, {name}.", "Welcome, {name}."),
	("Nothing to show yet", "Nothing to show yet"),
	(
		"Your Organizations, Projects, and Clusters will appear here as they become available.",
		"Your Organizations, Projects, and Clusters will appear here as they become available.",
	),
	// Account page
	(
		"The GitHub profile this Control Plane knows you by.",
		"The GitHub profile this Control Plane knows you by.",
	),
	("Profile", "Profile"),
	("GitHub account", "GitHub account"),
	("Staff", "Staff"),
	("No", "No"),
	(
		"You can operate this Control Plane across all Organizations.",
		"You can operate this Control Plane across all Organizations.",
	),
	("Session", "Session"),
	(
		"Signing out ends your session in this browser.",
		"Signing out ends your session in this browser.",
	),
	// Sign-in preview (decorative sample content)
	("Running", "Running"),
	("Applying", "Applying"),
	("Connected", "Connected"),
	("Waiting", "Waiting"),
	("Submitted", "Submitted"),
	("Sent to Agent", "Sent to Agent"),
	("Cluster", "Cluster"),
	("Commit", "Commit"),
	("Live", "Live"),
	("Deployment {number}", "Deployment {number}"),
	(
		"{project}, submitted {minutes} minutes ago by {user}",
		"{project}, submitted {minutes} minutes ago by {user}",
	),
	(
		"{ready} of {total} replicas ready",
		"{ready} of {total} replicas ready",
	),
	("Fix cart rounding for JPY", "Fix cart rounding for JPY"),
	(
		"applying Project {project} generation {number}",
		"applying Project {project} generation {number}",
	),
	(
		"migrate: {count} pending migrations",
		"migrate: {count} pending migrations",
	),
	("migrate: done", "migrate: done"),
	(
		"replicas: starting {count} replicas",
		"replicas: starting {count} replicas",
	),
	(
		"{replica} listening on 0.0.0.0:8000",
		"{replica} listening on 0.0.0.0:8000",
	),
	(
		"{replica} readiness probe passed",
		"{replica} readiness probe passed",
	),
	(
		"{replica} readiness probe failed (attempt {attempt} of {attempts})",
		"{replica} readiness probe failed (attempt {attempt} of {attempts})",
	),
	(
		"replicas: {ready} of {total} replicas ready",
		"replicas: {ready} of {total} replicas ready",
	),
	("{replica} cache warm-up: ok", "{replica} cache warm-up: ok"),
];
