use super::dockerfile::{Instruction, Stage};

/// All signals required for Dockerfile generation.
#[derive(Debug, Clone)]
// Reserved fields (cache, session_backend, graphql) are populated by
// collect_signals and will be consumed when corresponding Dockerfile
// optimizations are added.
#[allow(dead_code)]
pub(crate) struct DockerfileSignals {
	pub(crate) app_name: String,
	pub(crate) rust_version: String,
	pub(crate) pages: bool,
	pub(crate) grpc: bool,
	pub(crate) graphql: bool,
	pub(crate) wasm_bindgen_version: Option<String>,
	pub(crate) database: Option<String>,
	pub(crate) cache: Option<String>,
	pub(crate) session_backend: Option<String>,
	pub(crate) base_image_override: Option<String>,
}

const DEFAULT_RUNTIME_IMAGE: &str = "debian:bookworm-slim";

/// Determines the runtime packages needed based on the database signal.
fn runtime_packages(signals: &DockerfileSignals) -> Vec<&str> {
	let mut pkgs = vec!["ca-certificates", "tini"];

	match signals.database.as_deref() {
		Some("postgresql") | Some("cockroachdb") => pkgs.push("libpq5"),
		Some("mysql") => pkgs.push("default-mysql-client-core"),
		_ => {}
	}

	pkgs
}

/// Builds the "chef" stage that prepares the cargo-chef recipe.
pub(crate) fn build_chef_stage(signals: &DockerfileSignals) -> Stage {
	Stage {
		base_image: format!("rust:{}-bookworm", signals.rust_version),
		name: Some("chef".to_string()),
		platform: None,
		instructions: vec![
			Instruction::Run("cargo install cargo-chef".to_string()),
			Instruction::Workdir("/app".to_string()),
			Instruction::Copy {
				from: None,
				src: ".".to_string(),
				dst: ".".to_string(),
			},
			Instruction::Run("cargo chef prepare --recipe-path recipe.json".to_string()),
		],
	}
}

/// Builds the "builder" stage that compiles the application.
pub(crate) fn build_builder_stage(signals: &DockerfileSignals) -> Stage {
	let mut instructions = Vec::new();

	if signals.grpc {
		instructions.push(Instruction::RunMulti(vec![
			"apt-get update".to_string(),
			"apt-get install -y protobuf-compiler".to_string(),
			"rm -rf /var/lib/apt/lists/*".to_string(),
		]));
	}

	instructions.push(Instruction::Run("cargo install cargo-chef".to_string()));
	instructions.push(Instruction::Workdir("/app".to_string()));
	instructions.push(Instruction::Copy {
		from: Some("chef".to_string()),
		src: "/app/recipe.json".to_string(),
		dst: "recipe.json".to_string(),
	});
	instructions.push(Instruction::Run(
		"cargo chef cook --release --recipe-path recipe.json".to_string(),
	));
	instructions.push(Instruction::Copy {
		from: None,
		src: ".".to_string(),
		dst: ".".to_string(),
	});
	instructions.push(Instruction::Run("cargo build --release".to_string()));

	Stage {
		base_image: format!("rust:{}-bookworm", signals.rust_version),
		name: Some("builder".to_string()),
		platform: None,
		instructions,
	}
}

/// Builds the "wasm" stage for compiling WebAssembly output.
pub(crate) fn build_wasm_stage(signals: &DockerfileSignals) -> Stage {
	let version = signals.wasm_bindgen_version.as_deref().unwrap_or("0.2.100");

	let app_name_underscored = signals.app_name.replace('-', "_");

	Stage {
		base_image: format!("rust:{}-bookworm", signals.rust_version),
		name: Some("wasm".to_string()),
		platform: None,
		instructions: vec![
			Instruction::RunMulti(vec![
				"apt-get update".to_string(),
				"apt-get install -y binaryen".to_string(),
				"rm -rf /var/lib/apt/lists/*".to_string(),
			]),
			Instruction::Run("rustup target add wasm32-unknown-unknown".to_string()),
			Instruction::Run(format!("cargo install wasm-bindgen-cli@{version}")),
			Instruction::Workdir("/app".to_string()),
			Instruction::Copy {
				from: None,
				src: ".".to_string(),
				dst: ".".to_string(),
			},
			Instruction::Run("cargo build --release --target wasm32-unknown-unknown".to_string()),
			Instruction::RunMulti(vec![
				format!(
					"wasm-bindgen --out-dir /wasm-dist --target web \
                     target/wasm32-unknown-unknown/release/{app_name_underscored}.wasm"
				),
				"wasm-opt -Oz -o /wasm-dist/optimized.wasm /wasm-dist/*.wasm".to_string(),
			]),
		],
	}
}

/// Builds the "runtime" stage that produces the final container image.
pub(crate) fn build_runtime_stage(signals: &DockerfileSignals) -> Stage {
	let base_image = signals
		.base_image_override
		.clone()
		.unwrap_or_else(|| DEFAULT_RUNTIME_IMAGE.to_string());

	let pkgs = runtime_packages(signals).join(" ");

	let mut instructions = vec![
		Instruction::RunMulti(vec![
			"apt-get update".to_string(),
			format!("apt-get install -y --no-install-recommends {pkgs}"),
			"rm -rf /var/lib/apt/lists/*".to_string(),
		]),
		Instruction::Run("useradd --create-home appuser".to_string()),
		Instruction::Workdir("/app".to_string()),
		Instruction::Copy {
			from: Some("builder".to_string()),
			src: format!("/app/target/release/{}", signals.app_name),
			dst: "/app/".to_string(),
		},
	];

	if signals.pages {
		instructions.push(Instruction::Copy {
			from: Some("wasm".to_string()),
			src: "/wasm-dist".to_string(),
			dst: "/app/static/wasm/".to_string(),
		});
	}

	instructions.push(Instruction::Run(
		"chown -R appuser:appuser /app".to_string(),
	));
	instructions.push(Instruction::Comment("Run as non-root".to_string()));
	instructions.push(Instruction::User("appuser".to_string()));
	instructions.push(Instruction::Env(vec![(
		"RUST_LOG".to_string(),
		"info".to_string(),
	)]));
	instructions.push(Instruction::Expose(8000));
	instructions.push(Instruction::Entrypoint(vec![
		"tini".to_string(),
		"--".to_string(),
		format!("/app/{}", signals.app_name),
	]));

	Stage {
		base_image,
		name: Some("runtime".to_string()),
		platform: None,
		instructions,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	#[fixture]
	fn minimal_signals() -> DockerfileSignals {
		DockerfileSignals {
			app_name: "my-app".to_string(),
			rust_version: "1.94.1".to_string(),
			pages: false,
			grpc: false,
			graphql: false,
			wasm_bindgen_version: None,
			database: None,
			cache: None,
			session_backend: None,
			base_image_override: None,
		}
	}

	fn stage_contains_run(stage: &Stage, needle: &str) -> bool {
		stage.instructions.iter().any(|inst| match inst {
			Instruction::Run(cmd) => cmd.contains(needle),
			Instruction::RunMulti(cmds) => cmds.iter().any(|c| c.contains(needle)),
			_ => false,
		})
	}

	fn stage_contains_copy_from(stage: &Stage, from_name: &str) -> bool {
		stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { from: Some(f), .. } if f == from_name
			)
		})
	}

	// S1
	#[rstest]
	fn chef_stage_basic(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_chef_stage(&minimal_signals);

		// Assert
		assert_eq!(stage.name.as_deref(), Some("chef"));
		assert!(stage.base_image.contains("rust:1.94.1"));
		assert!(stage_contains_run(&stage, "cargo install cargo-chef"));
		assert!(stage_contains_run(&stage, "cargo chef prepare"));
	}

	// S2
	#[rstest]
	fn builder_stage_basic(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_run(&stage, "protobuf-compiler"));
	}

	// S3
	#[rstest]
	fn builder_stage_with_grpc(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.grpc = true;

		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "protobuf-compiler"));
	}

	// S4
	#[rstest]
	fn builder_stage_with_graphql(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.graphql = true;

		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "cargo build --release"));
	}

	// S5
	#[rstest]
	fn wasm_stage_basic(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.pages = true;
		minimal_signals.wasm_bindgen_version = Some("0.2.100".to_string());

		// Act
		let stage = build_wasm_stage(&minimal_signals);

		// Assert
		assert_eq!(stage.name.as_deref(), Some("wasm"));
		assert!(stage_contains_run(&stage, "wasm-bindgen-cli@0.2.100"));
	}

	// S6
	#[rstest]
	fn wasm_stage_with_wasm_opt(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.pages = true;
		minimal_signals.wasm_bindgen_version = Some("0.2.100".to_string());

		// Act
		let stage = build_wasm_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "binaryen"));
		assert!(stage_contains_run(&stage, "wasm-opt"));
	}

	// S7
	#[rstest]
	fn runtime_stage_postgres(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.database = Some("postgresql".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "libpq5"));
	}

	// S8
	#[rstest]
	fn runtime_stage_mysql(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.database = Some("mysql".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "default-mysql-client-core"));
	}

	// S9
	#[rstest]
	fn runtime_stage_sqlite(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.database = Some("sqlite".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_run(&stage, "libpq5"));
		assert!(!stage_contains_run(&stage, "mysql"));
	}

	// S10
	#[rstest]
	fn runtime_stage_no_db(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_run(&stage, "libpq5"));
		assert!(!stage_contains_run(&stage, "mysql"));
	}

	// S11
	#[rstest]
	fn runtime_stage_base_image_override(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.base_image_override = Some("gcr.io/distroless/cc-debian12".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert_eq!(stage.base_image, "gcr.io/distroless/cc-debian12");
	}

	// S12
	#[rstest]
	fn runtime_stage_default_base_image(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert_eq!(stage.base_image, "debian:bookworm-slim");
	}

	// S13
	#[rstest]
	fn runtime_stage_has_tini(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "tini"));
	}

	// S14
	#[rstest]
	fn runtime_stage_has_ca_certificates(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "ca-certificates"));
	}

	// S15
	#[rstest]
	fn runtime_copies_wasm_dist(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.pages = true;
		minimal_signals.wasm_bindgen_version = Some("0.2.100".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_copy_from(&stage, "wasm"));
	}

	// S16
	#[rstest]
	fn runtime_no_wasm_copy(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_copy_from(&stage, "wasm"));
	}

	// S17
	#[rstest]
	fn builder_grpc_and_graphql(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.grpc = true;
		minimal_signals.graphql = true;

		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_run(&stage, "protobuf-compiler"));
		assert!(stage_contains_run(&stage, "cargo build --release"));
	}

	// S18
	#[rstest]
	fn runtime_mysql_with_override(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.database = Some("mysql".to_string());
		minimal_signals.base_image_override = Some("custom:latest".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert_eq!(stage.base_image, "custom:latest");
		assert!(stage_contains_run(&stage, "default-mysql-client-core"));
	}
}
