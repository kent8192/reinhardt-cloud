//! The surfaces M1 removed have no live code path: account linking (SR-05),
//! password credentials (SR-21, SR-23, SR-25), email verification and change
//! (SR-22), registration gating and unverified accounts (SR-24), and merging
//! Users by email (SR-26).
//!
//! Each test pins the absence at the layer where the path would live: the
//! route inventory the server registers, the `User` model, the committed
//! migrations, and the management commands the framework adds. A change that
//! brings one back has to change a test that names the requirement it revives.

use std::fs;
use std::path::Path;

use reinhardt::auth::get_superuser_creator;
use reinhardt::db::orm::Model;
use rstest::rstest;
use serial_test::serial;

use crate::apps::accounts::models::User;
use crate::config::settings::test_support::{EnvGuard, required_env};

/// Every `(method, path, name)` the assembled server registers.
fn registered_routes() -> Vec<String> {
	let _env = EnvGuard::apply(&required_env("ci"));
	let router = crate::config::urls::routes().into_server();
	router
		.get_all_routes()
		.into_iter()
		.flat_map(|(path, name, _namespace, methods)| {
			let name = name.unwrap_or_default();
			methods
				.into_iter()
				.map(move |method| format!("{method} {path} {name}").to_ascii_lowercase())
		})
		.collect()
}

/// The registered routes whose method, path, or name contains any of `words`.
fn routes_mentioning(words: &[&str]) -> Vec<String> {
	registered_routes()
		.into_iter()
		// Confirming a Login Link is the one route that has "link" in it, and it
		// links nothing: the link is a sign-in credential (SR-16).
		.map(|route| route.replace("login_link", "").replace("login-link", ""))
		.filter(|route| words.iter().any(|word| route.contains(word)))
		.collect()
}

#[rstest]
#[serial(env_settings_load)]
fn the_route_inventory_is_not_empty_so_the_absence_checks_mean_something() {
	// Arrange / Act
	let routes = registered_routes();

	// Assert
	assert!(
		routes
			.iter()
			.any(|route| route.contains("/api/auth/github/")),
		"{routes:?}"
	);
	assert!(
		routes
			.iter()
			.any(|route| route.contains("/api/server_fn/sign_out")),
		"{routes:?}"
	);
}

#[rstest]
#[serial(env_settings_load)]
fn sr_05_no_route_links_or_unlinks_a_github_account() {
	// Arrange / Act
	let offending = routes_mentioning(&["link", "unlink", "connect", "disconnect", "attach"]);

	// Assert
	assert_eq!(offending, Vec::<String>::new());
}

#[rstest]
#[serial(env_settings_load)]
fn sr_21_23_25_no_route_sets_checks_changes_or_resets_a_password() {
	// Arrange / Act
	let offending = routes_mentioning(&["password", "passwd", "reset", "credential"]);

	// Assert
	assert_eq!(offending, Vec::<String>::new());
}

#[rstest]
#[serial(env_settings_load)]
fn sr_22_24_no_route_registers_verifies_or_changes_an_email() {
	// Arrange / Act
	let offending = routes_mentioning(&[
		"register",
		"signup",
		"sign_up",
		"sign-up",
		"verify",
		"verification",
		"confirm_email",
		"email",
	]);

	// Assert
	assert_eq!(offending, Vec::<String>::new());
}

#[rstest]
#[serial(env_settings_load)]
fn sr_26_no_route_merges_users() {
	// Arrange / Act
	let offending = routes_mentioning(&["merge", "claim"]);

	// Assert
	assert_eq!(offending, Vec::<String>::new());
}

#[rstest]
fn sr_21_24_26_the_user_model_has_no_credential_verification_or_merge_state() {
	// Arrange
	let expected = [
		"id",
		"github_user_id",
		"github_login",
		"display_name",
		"avatar_url",
		"email",
		"is_active",
		"is_staff",
		"last_login",
		"created_at",
		"updated_at",
	];

	// Act
	let mut columns: Vec<String> = User::field_metadata()
		.into_iter()
		.map(|field| field.name)
		.collect();
	columns.sort();
	let mut expected: Vec<&str> = expected.to_vec();
	expected.sort_unstable();

	// Assert
	assert_eq!(columns, expected);
}

/// The committed migration sources of every application.
fn migration_sources() -> Vec<(String, String)> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
	let mut sources = Vec::new();
	for app in fs::read_dir(&root).expect("the migrations directory exists") {
		let app = app.expect("a readable entry").path();
		if !app.is_dir() {
			continue;
		}
		for file in fs::read_dir(&app).expect("a readable application directory") {
			let file = file.expect("a readable entry").path();
			if file.extension().is_some_and(|extension| extension == "rs") {
				let text = fs::read_to_string(&file).expect("a readable migration");
				sources.push((file.display().to_string(), text));
			}
		}
	}
	assert!(!sources.is_empty(), "the committed migrations were found");
	sources
}

#[rstest]
fn sr_21_22_23_24_the_committed_migrations_create_no_credential_or_verification_storage() {
	// Arrange
	let forbidden = [
		"password",
		"passwd",
		"verified",
		"verification",
		"unverified",
		"reset_token",
		"confirm",
		"failed_login",
	];

	// Act
	let offending: Vec<String> = migration_sources()
		.into_iter()
		.flat_map(|(file, text)| {
			let lower = text.to_ascii_lowercase();
			forbidden
				.iter()
				.filter(move |word| lower.contains(*word))
				.map(move |word| format!("{file}: {word}"))
				.collect::<Vec<_>>()
		})
		.collect();

	// Assert
	assert_eq!(offending, Vec::<String>::new());
}

#[rstest]
fn sr_25_no_migration_seeds_a_row() {
	// Arrange / Act
	let seeding: Vec<String> = migration_sources()
		.into_iter()
		.filter(|(_, text)| {
			let lower = text.to_ascii_lowercase();
			lower.contains("insert into")
				|| lower.contains("insertrows")
				|| lower.contains("loaddata")
		})
		.map(|(file, _)| file)
		.collect();

	// Assert
	assert_eq!(seeding, Vec::<String>::new());
}

#[rstest]
fn sr_21_25_the_createsuperuser_command_has_no_user_model_to_create() {
	// Arrange / Act
	let creator = get_superuser_creator();

	// Assert
	assert!(
		creator.is_none(),
		"a registered superuser creator would make `manage createsuperuser` create a User with a password"
	);
}
