#![cfg(not(target_arch = "wasm32"))]

#[path = "support/fixture.rs"]
mod fixture;

use fixture::CloudFixture;
use rstest::rstest;

#[rstest]
#[tokio::test]
async fn native_migrations_apply_and_replay_against_postgresql() {
	// Arrange
	let fixture = CloudFixture::new().await;
	// Act
	fixture.run(&["migrate"]).await;
	fixture.run(&["migrate"]).await;
	let status = fixture.run(&["showmigrations"]).await;
	let history = String::from_utf8(status.stdout).unwrap();
	// Assert
	assert_eq!(
		history
			.lines()
			.filter(|line| line.trim_start().starts_with("[X]"))
			.count(),
		12
	);
}
