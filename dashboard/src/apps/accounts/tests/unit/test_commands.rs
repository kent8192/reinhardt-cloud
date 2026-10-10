//! The accounts `manage` commands as registered (SR-18, SR-20, SR-107).

use reinhardt::commands::CapabilityCommand;
use rstest::rstest;

use crate::apps::accounts::server::commands::create_login_link::CreateLoginLinkCommand;
use crate::apps::accounts::server::commands::grant_staff::GrantStaffCommand;
use crate::apps::accounts::server::commands::repoint_github_account::RepointGithubAccountCommand;
use crate::config::commands::registry;

#[rstest]
#[case("grant-staff")]
#[case("create-login-link")]
#[case("repoint-github-account")]
fn the_operator_commands_are_registered_by_name(#[case] name: &str) {
	// Arrange
	let registry = registry();

	// Act
	let command = registry.get_capability(name);

	// Assert
	assert!(command.is_some(), "`{name}` is not registered");
	assert_eq!(command.unwrap().cli().get_name(), name);
}

#[rstest]
fn the_command_definitions_are_valid_clap_commands() {
	// Arrange
	let commands = [
		GrantStaffCommand.cli(),
		CreateLoginLinkCommand.cli(),
		RepointGithubAccountCommand.cli(),
	];

	// Act / Assert
	for mut command in commands {
		command.build();
		command.debug_assert();
	}
}

#[rstest]
fn every_identifier_argument_is_a_required_numeric_github_user_id() {
	// Arrange
	let grant = GrantStaffCommand.cli();
	let link = CreateLoginLinkCommand.cli();
	let repoint = RepointGithubAccountCommand.cli();

	// Act
	let required: Vec<_> = [
		(&grant, "github-user-id"),
		(&link, "github-user-id"),
		(&repoint, "github-user-id"),
		(&repoint, "new-github-user-id"),
	]
	.iter()
	.map(|(command, id)| {
		command
			.get_arguments()
			.find(|argument| argument.get_id() == *id)
			.map(clap::Arg::is_required_set)
	})
	.collect();

	// Assert
	assert_eq!(required, [Some(true), Some(true), Some(true), Some(true)]);
}

#[rstest]
fn the_commands_load_their_own_validated_settings_instead_of_declaring_views() {
	// Arrange
	let matches =
		GrantStaffCommand
			.cli()
			.get_matches_from(["grant-staff", "--github-user-id", "1"]);

	// Act
	let requirements = GrantStaffCommand.requirements(&matches);

	// Assert
	assert!(requirements.is_empty());
}
