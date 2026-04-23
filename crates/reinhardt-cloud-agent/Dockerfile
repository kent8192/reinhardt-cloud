# syntax=docker/dockerfile:1.7
#
# Multi-stage Dockerfile for reinhardt-cloud-agent.
#
# - Builder stage: pinned Rust toolchain (matches rust-toolchain.toml) with
#   cargo-chef for dependency-layer caching and protoc for tonic codegen.
# - Runtime stage: Google distroless cc image running as the built-in nonroot
#   user (uid 65532). The Agent initiates outbound gRPC to the control plane
#   and does not listen on any port, so no EXPOSE directive is needed.

###############################################################################
# Stage 1: cargo-chef planner
# Computes a recipe from the workspace manifest so the dependency build layer
# can be cached independently from the application source.
###############################################################################
FROM rust:1.94-bookworm AS chef

# protoc is required by the `reinhardt-cloud-proto` build script (tonic-build).
# libssl-dev is omitted on purpose: the workspace uses rustls for TLS.
RUN apt-get update \
	&& apt-get install -y --no-install-recommends protobuf-compiler \
	&& rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked --version ^0.1

WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

###############################################################################
# Stage 2: builder
# Restores cached dependency artifacts from the chef recipe, then compiles the
# agent binary in release mode.
###############################################################################
FROM chef AS builder

COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --bin reinhardt-cloud-agent

COPY . .
RUN cargo build --release --bin reinhardt-cloud-agent \
	&& strip target/release/reinhardt-cloud-agent

###############################################################################
# Stage 3: runtime
# Distroless cc-debian12:nonroot provides libc, CA certificates, and runs as
# uid 65532 by default. No shell, no package manager, minimal attack surface.
###############################################################################
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=builder /build/target/release/reinhardt-cloud-agent /usr/local/bin/reinhardt-cloud-agent

USER nonroot:nonroot

ENTRYPOINT ["/usr/local/bin/reinhardt-cloud-agent"]
