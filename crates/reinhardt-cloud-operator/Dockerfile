# syntax=docker/dockerfile:1.7
#
# Multi-stage Dockerfile for reinhardt-cloud-operator.
#
# - Builder stage: pinned Rust toolchain (matches rust-toolchain.toml) with
#   cargo-chef for dependency-layer caching and protoc for tonic codegen.
# - Runtime stage: Google distroless cc-debian12:nonroot (uid 65532). The
#   operator listens on a metrics/healthz HTTP port (default 9090) and does
#   not require any privileged capability or shell at runtime.

###############################################################################
# Stage 1: cargo-chef planner
# Computes a recipe from the workspace manifest so the dependency build layer
# can be cached independently from the application source.
###############################################################################
FROM rust:1.94-bookworm AS chef

# protoc is required by the `reinhardt-cloud-proto` build script (tonic-build).
# libprotobuf-dev ships the well-known protos (google/protobuf/timestamp.proto
# etc.) that build.proto imports. libssl-dev is omitted on purpose: the
# workspace uses rustls for TLS.
RUN apt-get update \
	&& apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev \
	&& rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked --version ^0.1

WORKDIR /build

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

###############################################################################
# Stage 2: builder
# Restores cached dependency artifacts from the chef recipe, then compiles the
# operator binary in release mode.
###############################################################################
FROM chef AS builder

COPY --from=planner /build/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json --bin reinhardt-cloud-operator

COPY . .
RUN cargo build --release --bin reinhardt-cloud-operator \
	&& strip target/release/reinhardt-cloud-operator

###############################################################################
# Stage 3: runtime
# Distroless cc-debian12:nonroot provides libc, CA certificates, and runs as
# uid 65532 by default. No shell, no package manager, minimal attack surface.
###############################################################################
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime

COPY --from=builder /build/target/release/reinhardt-cloud-operator /usr/local/bin/reinhardt-cloud-operator

USER nonroot:nonroot

# Operator HTTP server (always serves /healthz; /metrics gated by env).
EXPOSE 9090

ENTRYPOINT ["/usr/local/bin/reinhardt-cloud-operator"]
