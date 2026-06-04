use super::dockerfile::{Instruction, Stage};

/// All signals required for Dockerfile generation.
#[derive(Debug, Clone)]
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
	/// When true, emits OTEL environment variables in the runtime stage so
	/// the application is pre-configured to export traces to the cluster's
	/// OpenTelemetry Collector. The specific service name is intentionally
	/// left as a placeholder ("app") because the real name is injected at
	/// Pod launch time via the Kubernetes Downward API.
	pub(crate) tracing: bool,
	/// When true, install `protobuf-compiler` (provides `protoc`) in the
	/// chef and builder stages. Set when `Cargo.lock` shows a transitive
	/// dependency on `prost`, `prost-build`, `tonic`, or `tonic-build`.
	///
	/// Independent from the `grpc` reinhardt-web feature flag because
	/// indirect dependencies (e.g., `reinhardt-cloud-grpc` pulling in
	/// `tonic-build`) can require protoc even when the consumer crate
	/// does not enable the reinhardt-web `grpc` feature.
	pub(crate) protoc_needed: bool,
	/// When true, the project has a `settings/` directory next to its
	/// `Cargo.toml`. The runtime stage will COPY it into `/app/settings`
	/// and set `REINHARDT_CLOUD_CONFIG_DIR=/app/settings` so the binary
	/// loads its configuration from a deterministic, container-local
	/// location rather than falling back to `CARGO_MANIFEST_DIR` (which
	/// does not exist inside the runtime image).
	///
	/// See kent8192/reinhardt-cloud#486 (issue 2).
	pub(crate) has_settings_dir: bool,
	/// Path to the project crate relative to the Docker build context
	/// (typically the workspace root). When `Some("dashboard")`, the
	/// builder stage's `COPY . .` puts the project sources at
	/// `/app/dashboard/...`, so the runtime stage must reference
	/// `/app/dashboard/settings` rather than `/app/settings`. `None` for
	/// single-crate projects where the project dir IS the build context.
	///
	/// See kent8192/reinhardt-cloud#486 (issue 2).
	pub(crate) project_relative_path: Option<String>,
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

	// Ship redis-tools when a Redis cache backend is configured so `redis-cli`
	// is available inside the container for probes and diagnostics.
	if signals.cache.as_deref() == Some("redis") {
		pkgs.push("redis-tools");
	}

	pkgs
}

/// Builds the "chef" stage that prepares the cargo-chef recipe.
pub(crate) fn build_chef_stage(signals: &DockerfileSignals) -> Stage {
	let mut instructions = Vec::new();

	// Install protoc up front so any subsequent step that resolves the
	// dependency tree (including transitive build scripts invoked when
	// cargo-chef inspects metadata) has the compiler available.
	if signals.grpc || signals.protoc_needed {
		// Install both `protobuf-compiler` (provides /usr/bin/protoc) and
		// `libprotobuf-dev` (ships well-known proto headers under
		// /usr/include/google/protobuf/, e.g. timestamp.proto, duration.proto,
		// empty.proto). prost-build invokes protoc with `-I/usr/include`
		// implicitly, so without libprotobuf-dev any `import
		// "google/protobuf/*.proto"` fails at build time. See
		// kent8192/reinhardt-cloud#496.
		instructions.push(Instruction::RunMulti(vec![
			"apt-get update".to_string(),
			"apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev"
				.to_string(),
			"rm -rf /var/lib/apt/lists/*".to_string(),
		]));
	}

	instructions.push(Instruction::Run("cargo install cargo-chef".to_string()));
	instructions.push(Instruction::Workdir("/app".to_string()));
	instructions.push(Instruction::Copy {
		from: None,
		src: ".".to_string(),
		dst: ".".to_string(),
	});
	instructions.push(Instruction::Run(
		"cargo chef prepare --recipe-path recipe.json".to_string(),
	));

	Stage {
		base_image: format!("rust:{}-bookworm", signals.rust_version),
		name: Some("chef".to_string()),
		platform: None,
		instructions,
	}
}

/// Builds the "builder" stage that compiles the application.
pub(crate) fn build_builder_stage(signals: &DockerfileSignals) -> Stage {
	let mut instructions = Vec::new();

	if signals.grpc || signals.protoc_needed {
		// Install both `protobuf-compiler` (provides /usr/bin/protoc) and
		// `libprotobuf-dev` (ships well-known proto headers under
		// /usr/include/google/protobuf/, e.g. timestamp.proto, duration.proto,
		// empty.proto). prost-build invokes protoc with `-I/usr/include`
		// implicitly, so without libprotobuf-dev any `import
		// "google/protobuf/*.proto"` fails at build time. See
		// kent8192/reinhardt-cloud#496.
		instructions.push(Instruction::RunMulti(vec![
			"apt-get update".to_string(),
			"apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev"
				.to_string(),
			"rm -rf /var/lib/apt/lists/*".to_string(),
		]));
	}

	// When the graphql signal is set, propagate the feature to both cargo-chef
	// dependency caching and the final build so the resulting binary includes
	// GraphQL schema code generation.
	let feature_args = if signals.graphql {
		" --features graphql"
	} else {
		""
	};

	instructions.push(Instruction::Run("cargo install cargo-chef".to_string()));
	instructions.push(Instruction::Workdir("/app".to_string()));
	instructions.push(Instruction::Copy {
		from: Some("chef".to_string()),
		src: "/app/recipe.json".to_string(),
		dst: "recipe.json".to_string(),
	});
	instructions.push(Instruction::Run(format!(
		"cargo chef cook --release{feature_args} --recipe-path recipe.json"
	)));
	instructions.push(Instruction::Copy {
		from: None,
		src: ".".to_string(),
		dst: ".".to_string(),
	});
	// Scope the build to the project crate so wasm-incompatible workspace
	// members (operator/agent/CLI/grpc/etc.) are not pulled into the build
	// graph. See kent8192/reinhardt-cloud#485 for the underlying mio failure
	// that motivated scoping.
	instructions.push(Instruction::Run(format!(
		"cargo build --release -p {}{feature_args}",
		signals.app_name
	)));

	Stage {
		base_image: format!("rust:{}-bookworm", signals.rust_version),
		name: Some("builder".to_string()),
		platform: None,
		instructions,
	}
}

/// Builds the "wasm" stage for compiling WebAssembly output.
pub(crate) fn build_wasm_stage(signals: &DockerfileSignals) -> Stage {
	// collect_signals guarantees wasm_bindgen_version is Some when pages is true.
	let version = signals
		.wasm_bindgen_version
		.as_deref()
		.expect("wasm_bindgen_version must be set when pages is enabled");

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
			// `--lib -p {app_name}` ensures only the dashboard crate's cdylib is
			// built. Without `-p` cargo builds every workspace member for the
			// wasm32 target, which fails on members that depend on
			// `tokio = { features = ["full"] }` because `mio`'s net feature is
			// not wasm-compatible. Without `--lib`, cargo also tries to build
			// the project's host binary (`main.rs`) for wasm32 and fails.
			// See kent8192/reinhardt-cloud#485.
			Instruction::Run(format!(
				"cargo build --release --target wasm32-unknown-unknown --lib -p {}",
				signals.app_name
			)),
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
///
/// When `base_image_override` is set, Debian-specific commands (`apt-get`,
/// `useradd`, `tini`) are omitted because the custom image may not be
/// Debian-based (e.g., distroless). The user is responsible for ensuring
/// the custom image contains all required runtime dependencies.
pub(crate) fn build_runtime_stage(signals: &DockerfileSignals) -> Stage {
	let is_custom_image = signals.base_image_override.is_some();
	let base_image = signals
		.base_image_override
		.clone()
		.unwrap_or_else(|| DEFAULT_RUNTIME_IMAGE.to_string());

	let mut instructions = Vec::new();

	if !is_custom_image {
		let pkgs = runtime_packages(signals).join(" ");
		instructions.push(Instruction::RunMulti(vec![
			"apt-get update".to_string(),
			format!("apt-get install -y --no-install-recommends {pkgs}"),
			"rm -rf /var/lib/apt/lists/*".to_string(),
		]));
		instructions.push(Instruction::Run(
			"useradd --create-home appuser".to_string(),
		));
	}

	instructions.push(Instruction::Workdir("/app".to_string()));
	instructions.push(Instruction::Copy {
		from: Some("builder".to_string()),
		src: format!("/app/target/release/{}", signals.app_name),
		dst: "/app/".to_string(),
	});
	instructions.push(Instruction::Copy {
		from: Some("builder".to_string()),
		src: "/app/target/release/manage".to_string(),
		dst: "/app/".to_string(),
	});

	if signals.pages {
		instructions.push(Instruction::Copy {
			from: Some("wasm".to_string()),
			src: "/wasm-dist".to_string(),
			dst: "/app/static/wasm/".to_string(),
		});
		// SPA fallback: ship `index.html` alongside the WASM artifacts so
		// `RunServerCommand`'s `--with-pages` mode can serve it for unknown
		// routes. Source path mirrors the settings/ COPY logic below
		// (workspace-member layouts vs. project-root layouts).
		// See kent8192/reinhardt-cloud#511.
		let index_src = match signals.project_relative_path.as_deref() {
			Some(rel) => format!("/app/{rel}/index.html"),
			None => "/app/index.html".to_string(),
		};
		instructions.push(Instruction::Copy {
			from: Some("builder".to_string()),
			src: index_src,
			dst: "/app/static/wasm/index.html".to_string(),
		});
	}

	// Bundle the project's `settings/` TOMLs into the runtime image so the
	// binary can load its configuration without bind-mounts. The source
	// path depends on whether the project lives at the build-context root
	// or as a workspace member. See kent8192/reinhardt-cloud#486 (issue 2).
	if signals.has_settings_dir {
		let settings_src = match signals.project_relative_path.as_deref() {
			Some(rel) => format!("/app/{rel}/settings"),
			None => "/app/settings".to_string(),
		};
		instructions.push(Instruction::Copy {
			from: Some("builder".to_string()),
			src: settings_src,
			dst: "/app/settings".to_string(),
		});
	}

	if is_custom_image {
		// Custom image: use numeric UID (65534 = nobody) since useradd is
		// unavailable on non-Debian images such as distroless.
		instructions.push(Instruction::Comment("Run as non-root".to_string()));
		instructions.push(Instruction::User("65534".to_string()));
	} else {
		instructions.push(Instruction::Run(
			"chown -R appuser:appuser /app".to_string(),
		));
		instructions.push(Instruction::Comment("Run as non-root".to_string()));
		instructions.push(Instruction::User("appuser".to_string()));
	}

	let mut env_pairs = vec![
		("RUST_LOG".to_string(), "info".to_string()),
		("PATH".to_string(), "/app:$PATH".to_string()),
	];
	if let Some(backend) = signals.session_backend.as_deref() {
		env_pairs.push(("REINHARDT_SESSION_BACKEND".to_string(), backend.to_string()));
	}
	// Pin the config dir so the binary loads its TOMLs from a known
	// location instead of falling back to `CARGO_MANIFEST_DIR` (which
	// resolves to the host build path and is meaningless in the runtime
	// image). See kent8192/reinhardt-cloud#486 (issue 2).
	if signals.has_settings_dir {
		env_pairs.push((
			"REINHARDT_CLOUD_CONFIG_DIR".to_string(),
			"/app/settings".to_string(),
		));
	}
	instructions.push(Instruction::Env(env_pairs));

	if signals.tracing {
		// Emit OTEL environment variables so the runtime image is pre-wired
		// for trace propagation. OTEL_SERVICE_NAME is set to the placeholder
		// "app"; the real service name is injected at Pod launch time via the
		// operator.
		//
		// OTEL_EXPORTER_OTLP_ENDPOINT is intentionally omitted here: the
		// operator injects it at Pod launch time when tracing is enabled (see
		// inference/env_vars.rs). Hard-coding the endpoint in the Dockerfile
		// would break apps when the operator-side tracing is disabled or the
		// endpoint differs from the baked-in value.
		instructions.push(Instruction::Env(vec![(
			"OTEL_PROPAGATORS".to_string(),
			"tracecontext".to_string(),
		)]));
		instructions.push(Instruction::Env(vec![(
			"OTEL_SERVICE_NAME".to_string(),
			"app".to_string(),
		)]));
	}

	instructions.push(Instruction::Expose(8000));

	if is_custom_image {
		// Custom image: run binary directly without tini since it may not
		// be available.
		instructions.push(Instruction::Entrypoint(vec![format!(
			"/app/{}",
			signals.app_name
		)]));
	} else {
		instructions.push(Instruction::Entrypoint(vec![
			"tini".to_string(),
			"--".to_string(),
			format!("/app/{}", signals.app_name),
		]));
	}

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
			tracing: false,
			protoc_needed: false,
			has_settings_dir: false,
			project_relative_path: None,
		}
	}

	fn stage_contains_run(stage: &Stage, needle: &str) -> bool {
		stage.instructions.iter().any(|inst| match inst {
			Instruction::Run(cmd) => cmd.contains(needle),
			Instruction::RunMulti(cmds) => cmds.iter().any(|c| c.contains(needle)),
			_ => false,
		})
	}

	/// Finds the apt-get install line within a stage's RunMulti instructions.
	/// Returns the full command string so callers can assert its exact content
	/// (e.g., that both `protobuf-compiler` and `libprotobuf-dev` are listed).
	fn stage_apt_install_line(stage: &Stage) -> Option<String> {
		stage.instructions.iter().find_map(|inst| match inst {
			Instruction::RunMulti(cmds) => cmds
				.iter()
				.find(|c| c.starts_with("apt-get install"))
				.cloned(),
			_ => None,
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

	fn stage_contains_copy(stage: &Stage, from_name: &str, src_path: &str, dst_path: &str) -> bool {
		stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { from: Some(f), src, dst }
					if f == from_name && src == src_path && dst == dst_path
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

		// Assert: install line must include both protobuf-compiler (provides
		// protoc) AND libprotobuf-dev (ships well-known proto headers under
		// /usr/include/google/protobuf/). See kent8192/reinhardt-cloud#496.
		assert_eq!(
			stage_apt_install_line(&stage).as_deref(),
			Some("apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev")
		);
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

	// S10b (Refs #637): generated runtime images must include the
	// management binary that operator init containers execute.
	#[rstest]
	fn runtime_stage_copies_manage_binary(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(
			stage_contains_copy(&stage, "builder", "/app/target/release/manage", "/app/"),
			"runtime stage must COPY /app/target/release/manage -> /app/; \
			 see kent8192/reinhardt-cloud#637"
		);
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
		// Custom image: no apt-get, no useradd, no tini
		assert!(!stage_contains_run(&stage, "apt-get"));
		assert!(!stage_contains_run(&stage, "useradd"));
		assert!(!stage_contains_run(&stage, "tini"));
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

	// S15b (Refs #511): pages-enabled runtime stage MUST also COPY
	// `index.html` so `RunServerCommand`'s SPA fallback can serve it for
	// unknown routes. For single-crate projects, the source path is at
	// the build-context root.
	#[rstest]
	fn runtime_copies_index_html_for_root_project(mut minimal_signals: DockerfileSignals) {
		// Arrange — single-crate project: project_relative_path is None
		minimal_signals.pages = true;
		minimal_signals.wasm_bindgen_version = Some("0.2.100".to_string());
		minimal_signals.project_relative_path = None;

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		let copies_index = stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { from: Some(f), src, dst }
					if f == "builder"
						&& src == "/app/index.html"
						&& dst == "/app/static/wasm/index.html"
			)
		});
		assert!(
			copies_index,
			"runtime stage must COPY /app/index.html -> /app/static/wasm/index.html; \
			 see kent8192/reinhardt-cloud#511"
		);
	}

	// S15c (Refs #511): for workspace-member projects, the index.html
	// source path must include the project's relative path so COPY
	// resolves to the correct location inside the builder stage's
	// filesystem.
	#[rstest]
	fn runtime_copies_index_html_for_workspace_member(mut minimal_signals: DockerfileSignals) {
		// Arrange — workspace member at `dashboard/`
		minimal_signals.pages = true;
		minimal_signals.wasm_bindgen_version = Some("0.2.100".to_string());
		minimal_signals.project_relative_path = Some("dashboard".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		let copies_index = stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { from: Some(f), src, dst }
					if f == "builder"
						&& src == "/app/dashboard/index.html"
						&& dst == "/app/static/wasm/index.html"
			)
		});
		assert!(
			copies_index,
			"runtime stage must COPY /app/dashboard/index.html for workspace-member \
			 projects; see kent8192/reinhardt-cloud#511"
		);
	}

	// S15d (Refs #511): non-pages projects must NOT trigger the index.html
	// COPY, because the source file may not exist.
	#[rstest]
	fn runtime_no_index_html_copy_when_pages_disabled(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		let copies_index = stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { dst, .. } if dst == "/app/static/wasm/index.html"
			)
		});
		assert!(
			!copies_index,
			"runtime stage must NOT COPY index.html when pages is disabled"
		);
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

	fn stage_env_value(stage: &Stage, key: &str) -> Option<String> {
		stage.instructions.iter().find_map(|inst| match inst {
			Instruction::Env(pairs) => pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()),
			_ => None,
		})
	}

	// S10c (Refs #637): operator init containers execute `manage` by
	// name, so `/app` must be on PATH for `/app/manage` to resolve.
	#[rstest]
	fn runtime_stage_adds_app_dir_to_path(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert_eq!(
			stage_env_value(&stage, "PATH").as_deref(),
			Some("/app:$PATH"),
			"runtime stage must add /app to PATH so bare `manage` resolves; \
			 see kent8192/reinhardt-cloud#637"
		);
	}

	// S19 (Refs #372): redis cache installs redis-tools in runtime
	#[rstest]
	fn runtime_stage_installs_redis_tools_when_cache_redis(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.cache = Some("redis".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(
			stage_contains_run(&stage, "redis-tools"),
			"expected redis-tools in runtime packages for cache=redis"
		);
	}

	// S20 (Refs #372): redis cache not installed when cache is not redis
	#[rstest]
	fn runtime_stage_no_redis_tools_when_cache_absent(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_run(&stage, "redis-tools"));
	}

	// S21 (Refs #372): session_backend is propagated as runtime env
	#[rstest]
	fn runtime_stage_sets_session_backend_env(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.session_backend = Some("redis".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert_eq!(
			stage_env_value(&stage, "REINHARDT_SESSION_BACKEND").as_deref(),
			Some("redis")
		);
	}

	// S22 (Refs #372): no session_backend env when signal is absent
	#[rstest]
	fn runtime_stage_no_session_backend_env_when_absent(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_env_value(&stage, "REINHARDT_SESSION_BACKEND").is_none());
	}

	// S23 (Refs #372): graphql feature is passed to cargo build
	#[rstest]
	fn builder_stage_enables_graphql_feature_when_graphql_set(
		mut minimal_signals: DockerfileSignals,
	) {
		// Arrange
		minimal_signals.graphql = true;

		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert: both cargo-chef caching and the final build must carry the
		// graphql feature so the compiled binary includes GraphQL code.
		// The cargo build line also carries `-p my-app` for #485 scoping.
		assert!(
			stage_contains_run(&stage, "cargo build --release -p my-app --features graphql"),
			"expected graphql feature in cargo build with package scope"
		);
		assert!(
			stage_contains_run(&stage, "cargo chef cook --release --features graphql"),
			"expected graphql feature in cargo chef cook"
		);
	}

	// S24 (Refs #372): graphql feature is NOT passed when signal is absent
	#[rstest]
	fn builder_stage_no_graphql_feature_when_absent(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_run(&stage, "--features graphql"));
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
		// Custom image: apt-get is skipped, so no mysql client package
		assert!(!stage_contains_run(&stage, "apt-get"));
		assert!(!stage_contains_run(&stage, "default-mysql-client-core"));
	}

	fn stage_contains_env(stage: &Stage, key: &str, value: &str) -> bool {
		stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Env(pairs)
					if pairs.iter().any(|(k, v)| k == key && v == value)
			)
		})
	}

	// S19
	#[rstest]
	fn tracing_enabled_emits_otel_env(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.tracing = true;

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(stage_contains_env(
			&stage,
			"OTEL_PROPAGATORS",
			"tracecontext"
		));
		assert!(stage_contains_env(&stage, "OTEL_SERVICE_NAME", "app"));
	}

	// S20
	#[rstest]
	fn tracing_disabled_omits_otel_env(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		assert!(!stage_contains_env(
			&stage,
			"OTEL_PROPAGATORS",
			"tracecontext"
		));
		assert!(!stage_contains_env(&stage, "OTEL_SERVICE_NAME", "app"));
	}

	// PR1 (Refs #477, #496): protoc_needed alone (grpc=false) installs protoc
	// AND libprotobuf-dev in the chef stage so cargo-chef metadata extraction
	// does not abort when a transitive build script invokes protoc, and so
	// well-known proto imports (google/protobuf/timestamp.proto, etc.) resolve.
	#[rstest]
	fn chef_stage_installs_protoc_when_protoc_needed(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.grpc = false;
		minimal_signals.protoc_needed = true;

		// Act
		let stage = build_chef_stage(&minimal_signals);

		// Assert: install line must list both packages. libprotobuf-dev ships
		// the well-known proto headers (google/protobuf/timestamp.proto, etc.)
		// that prost-build needs. See kent8192/reinhardt-cloud#496.
		assert_eq!(
			stage_apt_install_line(&stage).as_deref(),
			Some("apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev"),
			"chef stage must install protobuf-compiler AND libprotobuf-dev when protoc_needed is set"
		);
	}

	// PR2 (Refs #477, #496): protoc_needed alone (grpc=false) installs protoc
	// AND libprotobuf-dev in the builder stage so `cargo chef cook` succeeds
	// against transitive prost/tonic dependencies and well-known proto imports
	// resolve.
	#[rstest]
	fn builder_stage_installs_protoc_when_protoc_needed(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.grpc = false;
		minimal_signals.protoc_needed = true;

		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert: install line must list both packages. libprotobuf-dev ships
		// the well-known proto headers (google/protobuf/timestamp.proto, etc.)
		// that prost-build needs. See kent8192/reinhardt-cloud#496.
		assert_eq!(
			stage_apt_install_line(&stage).as_deref(),
			Some("apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev"),
			"builder stage must install protobuf-compiler AND libprotobuf-dev when protoc_needed is set"
		);
	}

	// PS1 (Refs #485): builder stage scopes cargo build to the project
	// package, preventing wasm-incompatible workspace members from being
	// pulled into the build graph.
	#[rstest]
	fn builder_stage_scopes_cargo_build_to_package(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_builder_stage(&minimal_signals);

		// Assert
		assert!(
			stage_contains_run(&stage, "cargo build --release -p my-app"),
			"builder stage must pass `-p {{app_name}}` to cargo build; \
			 see kent8192/reinhardt-cloud#485"
		);
	}

	// PS2 (Refs #485): wasm stage scopes cargo build to the project package
	// AND restricts to lib targets. Without `--lib`, cargo also tries to
	// compile the project's bin (`main.rs`) for wasm32 and fails because
	// server-only deps (tokio with full features) are not wasm-compatible.
	#[rstest]
	fn wasm_stage_scopes_cargo_build_to_lib_target(mut minimal_signals: DockerfileSignals) {
		// Arrange
		minimal_signals.pages = true;
		minimal_signals.wasm_bindgen_version = Some("0.2.100".to_string());

		// Act
		let stage = build_wasm_stage(&minimal_signals);

		// Assert
		assert!(
			stage_contains_run(
				&stage,
				"cargo build --release --target wasm32-unknown-unknown --lib -p my-app",
			),
			"wasm stage must pass `--lib -p {{app_name}}` to cargo build; \
			 see kent8192/reinhardt-cloud#485"
		);
	}

	// SD1 (Refs #486 issue 2): runtime stage bundles project settings when
	// the project ships a `settings/` directory next to its `Cargo.toml`.
	// For workspace members, the source path includes the project's
	// relative path so `COPY` resolves to the correct location inside
	// the builder stage's filesystem.
	#[rstest]
	fn runtime_stage_bundles_settings_for_workspace_member(mut minimal_signals: DockerfileSignals) {
		// Arrange — workspace member at `dashboard/` with a settings dir
		minimal_signals.has_settings_dir = true;
		minimal_signals.project_relative_path = Some("dashboard".to_string());

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert: COPY must reference the workspace-relative path AND set
		// REINHARDT_CLOUD_CONFIG_DIR so the binary doesn't fall back to
		// CARGO_MANIFEST_DIR at runtime.
		let copies_settings = stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { from: Some(f), src, dst }
					if f == "builder" && src == "/app/dashboard/settings" && dst == "/app/settings"
			)
		});
		assert!(
			copies_settings,
			"runtime stage must COPY /app/dashboard/settings -> /app/settings"
		);
		assert_eq!(
			stage_env_value(&stage, "REINHARDT_CLOUD_CONFIG_DIR").as_deref(),
			Some("/app/settings"),
			"runtime stage must pin REINHARDT_CLOUD_CONFIG_DIR; \
			 see kent8192/reinhardt-cloud#486"
		);
	}

	// SD2 (Refs #486 issue 2): single-crate project (no workspace) places
	// `settings/` at the build context root, so the COPY src is just
	// `/app/settings`.
	#[rstest]
	fn runtime_stage_bundles_settings_for_root_project(mut minimal_signals: DockerfileSignals) {
		// Arrange — single-crate project: project_relative_path is None
		minimal_signals.has_settings_dir = true;
		minimal_signals.project_relative_path = None;

		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		let copies_settings = stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { from: Some(f), src, dst }
					if f == "builder" && src == "/app/settings" && dst == "/app/settings"
			)
		});
		assert!(
			copies_settings,
			"single-crate project: COPY src must be /app/settings (no prefix)"
		);
	}

	// SD3 (Refs #486 issue 2): when has_settings_dir is false (project
	// has no settings/ directory), runtime stage must NOT emit the COPY
	// or the env var. This preserves backwards compatibility for
	// projects that load settings from elsewhere.
	#[rstest]
	fn runtime_stage_omits_settings_when_dir_absent(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_runtime_stage(&minimal_signals);

		// Assert
		let has_settings_copy = stage.instructions.iter().any(|inst| {
			matches!(
				inst,
				Instruction::Copy { dst, .. } if dst == "/app/settings"
			)
		});
		assert!(
			!has_settings_copy,
			"must not COPY settings when has_settings_dir is false"
		);
		assert!(
			stage_env_value(&stage, "REINHARDT_CLOUD_CONFIG_DIR").is_none(),
			"must not set REINHARDT_CLOUD_CONFIG_DIR when has_settings_dir is false"
		);
	}

	// PR3 (Refs #477): chef stage emits no protoc install when neither grpc
	// nor protoc_needed is set, preserving the slim baseline for non-grpc
	// projects.
	#[rstest]
	fn chef_stage_omits_protoc_when_neither_signal_set(minimal_signals: DockerfileSignals) {
		// Act
		let stage = build_chef_stage(&minimal_signals);

		// Assert
		assert!(
			!stage_contains_run(&stage, "protobuf-compiler"),
			"chef stage must not install protobuf-compiler without grpc or protoc_needed"
		);
	}
}
