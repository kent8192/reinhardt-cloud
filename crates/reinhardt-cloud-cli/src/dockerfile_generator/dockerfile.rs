use std::fmt;

/// Represents a complete Dockerfile with a header comment and one or more build stages.
pub(crate) struct Dockerfile {
	pub(crate) header_comment: String,
	pub(crate) stages: Vec<Stage>,
}

/// Represents a single stage in a multi-stage Dockerfile build.
pub(crate) struct Stage {
	pub(crate) base_image: String,
	pub(crate) name: Option<String>,
	pub(crate) platform: Option<String>,
	pub(crate) instructions: Vec<Instruction>,
}

/// Represents a single Dockerfile instruction.
// Variants Arg, Cmd, Label, and EmptyLine are used in tests and
// reserved for future Dockerfile patterns.
#[allow(dead_code)]
pub(crate) enum Instruction {
	Arg {
		name: String,
		default: Option<String>,
	},
	Workdir(String),
	Run(String),
	RunMulti(Vec<String>),
	Copy {
		from: Option<String>,
		src: String,
		dst: String,
	},
	Env(Vec<(String, String)>),
	Expose(u16),
	Entrypoint(Vec<String>),
	Cmd(Vec<String>),
	Label(Vec<(String, String)>),
	User(String),
	Comment(String),
	EmptyLine,
}

impl fmt::Display for Instruction {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Instruction::Arg { name, default } => match default {
				Some(v) => write!(f, "ARG {name}={v}"),
				None => write!(f, "ARG {name}"),
			},
			Instruction::Workdir(path) => write!(f, "WORKDIR {path}"),
			Instruction::Run(cmd) => write!(f, "RUN {cmd}"),
			Instruction::RunMulti(cmds) => {
				if cmds.len() == 1 {
					write!(f, "RUN {}", cmds[0])
				} else {
					write!(f, "RUN {}", cmds.join(" && \\\n    "))
				}
			}
			Instruction::Copy { from, src, dst } => match from {
				Some(s) => write!(f, "COPY --from={s} {src} {dst}"),
				None => write!(f, "COPY {src} {dst}"),
			},
			Instruction::Env(vars) => {
				let formatted: Vec<String> = vars
					.iter()
					.map(|(k, v)| {
						if v.contains(' ') {
							format!("{k}=\"{v}\"")
						} else {
							format!("{k}={v}")
						}
					})
					.collect();
				write!(f, "ENV {}", formatted.join(" \\\n    "))
			}
			Instruction::Expose(port) => write!(f, "EXPOSE {port}"),
			Instruction::Entrypoint(args) => {
				let quoted: Vec<String> = args.iter().map(|a| format!("\"{a}\"")).collect();
				write!(f, "ENTRYPOINT [{}]", quoted.join(", "))
			}
			Instruction::Cmd(args) => {
				let quoted: Vec<String> = args.iter().map(|a| format!("\"{a}\"")).collect();
				write!(f, "CMD [{}]", quoted.join(", "))
			}
			Instruction::Label(labels) => {
				let formatted: Vec<String> =
					labels.iter().map(|(k, v)| format!("{k}=\"{v}\"")).collect();
				write!(f, "LABEL {}", formatted.join(" \\\n      "))
			}
			Instruction::User(user) => write!(f, "USER {user}"),
			Instruction::Comment(text) => write!(f, "# {text}"),
			Instruction::EmptyLine => Ok(()),
		}
	}
}

impl fmt::Display for Stage {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "FROM ")?;
		if let Some(platform) = &self.platform {
			write!(f, "--platform={platform} ")?;
		}
		write!(f, "{}", self.base_image)?;
		if let Some(name) = &self.name {
			write!(f, " AS {name}")?;
		}
		writeln!(f)?;
		for instruction in &self.instructions {
			writeln!(f, "{instruction}")?;
		}
		Ok(())
	}
}

impl fmt::Display for Dockerfile {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		writeln!(f, "# {}", self.header_comment)?;
		for (i, stage) in self.stages.iter().enumerate() {
			if i > 0 {
				writeln!(f)?;
			}
			write!(f, "{stage}")?;
		}
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::*;

	// D1
	#[rstest]
	fn instruction_run_single() {
		let instr = Instruction::Run("cargo build --release".to_string());
		assert_eq!(instr.to_string(), "RUN cargo build --release");
	}

	// D2
	#[rstest]
	fn instruction_run_multi() {
		// Arrange
		let instr = Instruction::RunMulti(vec![
			"apt-get update".to_string(),
			"apt-get install -y curl".to_string(),
		]);

		// Act
		let result = instr.to_string();

		// Assert
		assert_eq!(
			result,
			"RUN apt-get update && \\\n    apt-get install -y curl"
		);
	}

	// D3
	#[rstest]
	fn instruction_copy_with_from() {
		let instr = Instruction::Copy {
			from: Some("builder".to_string()),
			src: "/app/target".to_string(),
			dst: "/usr/local/bin/".to_string(),
		};
		assert_eq!(
			instr.to_string(),
			"COPY --from=builder /app/target /usr/local/bin/"
		);
	}

	// D4
	#[rstest]
	fn instruction_copy_without_from() {
		let instr = Instruction::Copy {
			from: None,
			src: "Cargo.toml".to_string(),
			dst: "./".to_string(),
		};
		assert_eq!(instr.to_string(), "COPY Cargo.toml ./");
	}

	// D5
	#[rstest]
	fn instruction_env_single() {
		let instr = Instruction::Env(vec![("RUST_LOG".to_string(), "info".to_string())]);
		assert_eq!(instr.to_string(), "ENV RUST_LOG=info");
	}

	// D6
	#[rstest]
	fn instruction_env_multiple() {
		// Arrange
		let instr = Instruction::Env(vec![
			("RUST_LOG".to_string(), "info".to_string()),
			("APP_ENV".to_string(), "production".to_string()),
		]);

		// Act
		let result = instr.to_string();

		// Assert
		assert_eq!(result, "ENV RUST_LOG=info \\\n    APP_ENV=production");
	}

	// D7
	#[rstest]
	fn instruction_entrypoint() {
		let instr = Instruction::Entrypoint(vec!["/app/server".to_string()]);
		assert_eq!(instr.to_string(), "ENTRYPOINT [\"/app/server\"]");
	}

	// D8
	#[rstest]
	fn instruction_cmd() {
		let instr = Instruction::Cmd(vec!["--port".to_string(), "8080".to_string()]);
		assert_eq!(instr.to_string(), "CMD [\"--port\", \"8080\"]");
	}

	// D9
	#[rstest]
	fn instruction_arg_with_default() {
		let instr = Instruction::Arg {
			name: "RUST_VERSION".to_string(),
			default: Some("1.82".to_string()),
		};
		assert_eq!(instr.to_string(), "ARG RUST_VERSION=1.82");
	}

	// D10
	#[rstest]
	fn instruction_arg_without_default() {
		let instr = Instruction::Arg {
			name: "FEATURES".to_string(),
			default: None,
		};
		assert_eq!(instr.to_string(), "ARG FEATURES");
	}

	// D11
	#[rstest]
	fn instruction_expose() {
		let instr = Instruction::Expose(8080);
		assert_eq!(instr.to_string(), "EXPOSE 8080");
	}

	// D12
	#[rstest]
	fn instruction_comment() {
		let instr = Instruction::Comment("Install dependencies".to_string());
		assert_eq!(instr.to_string(), "# Install dependencies");
	}

	// D13
	#[rstest]
	fn instruction_empty_line() {
		let instr = Instruction::EmptyLine;
		assert_eq!(instr.to_string(), "");
	}

	// D14
	#[rstest]
	fn instruction_label_single() {
		let instr = Instruction::Label(vec![(
			"maintainer".to_string(),
			"team@example.com".to_string(),
		)]);
		assert_eq!(instr.to_string(), "LABEL maintainer=\"team@example.com\"");
	}

	// D15
	#[rstest]
	fn instruction_workdir() {
		let instr = Instruction::Workdir("/app".to_string());
		assert_eq!(instr.to_string(), "WORKDIR /app");
	}

	// D16
	#[rstest]
	fn stage_with_name() {
		// Arrange
		let stage = Stage {
			base_image: "rust:1.82-slim".to_string(),
			name: Some("builder".to_string()),
			platform: None,
			instructions: vec![Instruction::Workdir("/app".to_string())],
		};

		// Act
		let result = stage.to_string();

		// Assert
		assert_eq!(result, "FROM rust:1.82-slim AS builder\nWORKDIR /app\n");
	}

	// D17
	#[rstest]
	fn stage_without_name() {
		// Arrange
		let stage = Stage {
			base_image: "debian:bookworm-slim".to_string(),
			name: None,
			platform: None,
			instructions: vec![Instruction::Expose(8080)],
		};

		// Act
		let result = stage.to_string();

		// Assert
		assert_eq!(result, "FROM debian:bookworm-slim\nEXPOSE 8080\n");
	}

	// D18
	#[rstest]
	fn stage_with_platform() {
		// Arrange
		let stage = Stage {
			base_image: "rust:1.82".to_string(),
			name: Some("builder".to_string()),
			platform: Some("linux/amd64".to_string()),
			instructions: vec![],
		};

		// Act
		let result = stage.to_string();

		// Assert
		assert_eq!(result, "FROM --platform=linux/amd64 rust:1.82 AS builder\n");
	}

	// D19
	#[rstest]
	fn full_dockerfile_header() {
		// Arrange
		let dockerfile = Dockerfile {
			header_comment: "Auto-generated by reinhardt-cloud-cli".to_string(),
			stages: vec![
				Stage {
					base_image: "rust:1.82".to_string(),
					name: Some("builder".to_string()),
					platform: None,
					instructions: vec![Instruction::Workdir("/app".to_string())],
				},
				Stage {
					base_image: "debian:bookworm-slim".to_string(),
					name: None,
					platform: None,
					instructions: vec![Instruction::Expose(8080)],
				},
			],
		};

		// Act
		let result = dockerfile.to_string();

		// Assert
		let expected = "\
# Auto-generated by reinhardt-cloud-cli
FROM rust:1.82 AS builder
WORKDIR /app

FROM debian:bookworm-slim
EXPOSE 8080
";
		assert_eq!(result, expected);
	}

	// D20
	#[rstest]
	fn run_multi_single_element() {
		let instr = Instruction::RunMulti(vec!["cargo build".to_string()]);
		assert_eq!(instr.to_string(), "RUN cargo build");
	}

	// D21
	#[rstest]
	fn env_value_with_spaces() {
		let instr = Instruction::Env(vec![("GREETING".to_string(), "hello world".to_string())]);
		assert_eq!(instr.to_string(), "ENV GREETING=\"hello world\"");
	}

	// D22
	#[rstest]
	fn entrypoint_with_args() {
		// Arrange
		let instr = Instruction::Entrypoint(vec![
			"/app/server".to_string(),
			"--port".to_string(),
			"8080".to_string(),
		]);

		// Act
		let result = instr.to_string();

		// Assert
		assert_eq!(result, "ENTRYPOINT [\"/app/server\", \"--port\", \"8080\"]");
	}
}
