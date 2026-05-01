# syntax=docker/dockerfile:1.7
#
# Development container image for the Reinhardt Cloud dashboard.
#
# This image is for local/CI development only — the production image lives in
# `dashboard/Dockerfile` (multi-stage chef -> builder -> wasm -> runtime).
# Both intentionally pin Rust 1.94.0 to match `rust-toolchain.toml`.

FROM mcr.microsoft.com/devcontainers/rust:1-bookworm

# System packages required by the dashboard build (protoc / openssl / libpq /
# wasm-opt) and by interactive workflows (lldb, postgresql-client, redis-tools).
RUN apt-get update \
	&& apt-get install -y --no-install-recommends \
		protobuf-compiler \
		libprotobuf-dev \
		binaryen \
		pkg-config \
		libssl-dev \
		libpq-dev \
		postgresql-client \
		redis-tools \
		lldb \
	&& apt-get clean \
	&& rm -rf /var/lib/apt/lists/*

# Pin the toolchain to 1.94.0 with the components needed by rust-analyzer.
# `cargo` will auto-honor `rust-toolchain.toml` once the workspace is mounted,
# but installing the same version here lets `cargo install` below resolve
# without an extra rustup network hop.
RUN rustup toolchain install 1.94.0 \
		--component rustfmt \
		--component clippy \
		--component rust-src \
		--component rust-analyzer \
	&& rustup target add --toolchain 1.94.0 wasm32-unknown-unknown \
	&& rustup default 1.94.0

# Install cargo tooling. Each `cargo install` is a separate layer so that
# bumping one tool does not invalidate the others' caches.
RUN cargo install cargo-make --locked
RUN cargo install cargo-nextest --locked
RUN cargo install cargo-audit --locked
RUN cargo install bacon --locked

# wasm-bindgen-cli must match the `wasm-bindgen` version in `Cargo.lock`
# (currently 0.2.118). Note: `dashboard/Makefile.toml`'s `wasm-install-tools`
# task pins 0.2.114 today; that mismatch is tracked separately and may
# overwrite this version when `cargo make wasm-build-dev` runs.
RUN cargo install wasm-bindgen-cli --locked --version 0.2.118

# `reinhardt-admin` provides the `fmt-all` subcommand used by `cargo make fmt-check`.
RUN cargo install reinhardt-admin-cli --locked --version 0.1.0-rc.15
