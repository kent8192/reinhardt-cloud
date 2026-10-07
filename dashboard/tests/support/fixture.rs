use std::path::Path;

use tempfile::TempDir;
use testcontainers::core::{IntoContainerPort, WaitFor};
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::process::Command;
use uuid::Uuid;

/// Own every external resource used by a native Control Plane test.
pub(super) struct CloudFixture {
	_settings: TempDir,
	_postgres: ContainerAsync<GenericImage>,
	port: u16,
	password: String,
	secret_key: String,
	pub(super) origin: String,
}

impl CloudFixture {
	pub(super) async fn new() -> Self {
		let settings = TempDir::new().unwrap();
		let project = Path::new(env!("CARGO_MANIFEST_DIR"));
		for profile in ["base", "local"] {
			std::fs::copy(
				project
					.join("settings")
					.join(format!("{profile}.example.toml")),
				settings.path().join(format!("{profile}.toml")),
			)
			.unwrap();
		}
		let password = format!("{}", Uuid::new_v4().simple());
		let secret_key = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
		let postgres = GenericImage::new("postgres", "16-alpine")
			.with_exposed_port(5432.tcp())
			.with_wait_for(WaitFor::message_on_stderr(
				"database system is ready to accept connections",
			))
			.with_wait_for(WaitFor::message_on_stdout(
				"database system is ready to accept connections",
			))
			.with_env_var("POSTGRES_DB", "reinhardt_cloud")
			.with_env_var("POSTGRES_USER", "reinhardt_cloud")
			.with_env_var("POSTGRES_PASSWORD", &password)
			.start()
			.await
			.unwrap();
		let port = postgres.get_host_port_ipv4(5432).await.unwrap();
		let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let origin = format!("http://{}", listener.local_addr().unwrap());
		Self {
			_settings: settings,
			_postgres: postgres,
			port,
			password,
			secret_key,
			origin,
		}
	}

	pub(super) fn database_url(&self) -> String {
		format!(
			"postgres://reinhardt_cloud:{}@127.0.0.1:{}/reinhardt_cloud",
			self.password, self.port
		)
	}

	pub(super) fn command(&self, arguments: &[&str]) -> Command {
		let mut command = Command::new(env!("CARGO_BIN_EXE_manage"));
		command
			.kill_on_drop(true)
			.current_dir(env!("CARGO_MANIFEST_DIR"))
			.env("CLOUD_SETTINGS_DIR", self._settings.path())
			.env("REINHARDT_ENV", "local")
			.env("CLOUD_DB_HOST", "127.0.0.1")
			.env("CLOUD_DB_PORT", format!("{}", self.port))
			.env("CLOUD_DB_NAME", "reinhardt_cloud")
			.env("CLOUD_DB_USER", "reinhardt_cloud")
			.env("CLOUD_DB_PASSWORD", &self.password)
			.env("CLOUD_SECRET_KEY", &self.secret_key)
			.env("CLOUD_PUBLIC_ORIGIN", &self.origin)
			.env("DATABASE_URL", self.database_url())
			.args(arguments);
		command
	}

	pub(super) async fn run(&self, arguments: &[&str]) -> std::process::Output {
		let output = self.command(arguments).output().await.unwrap();
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		output
	}
}
