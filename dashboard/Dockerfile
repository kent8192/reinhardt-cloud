# Dashboard image with build-time collection of generated component styles.
FROM rust:1.96.0-bookworm AS chef
RUN apt-get update && \
    apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && \
    rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM rust:1.96.0-bookworm AS builder
RUN apt-get update && \
    apt-get install -y --no-install-recommends protobuf-compiler libprotobuf-dev && \
    rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
WORKDIR /app
COPY --from=chef /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release -p reinhardt-cloud-dashboard

FROM rust:1.96.0-bookworm AS wasm
RUN rustup target add wasm32-unknown-unknown
RUN cargo install wasm-bindgen-cli@0.2.126
WORKDIR /app
COPY . .
RUN cargo build --release --target wasm32-unknown-unknown --lib -p reinhardt-cloud-dashboard
RUN wasm-bindgen --out-dir /wasm-dist --target web \
    target/wasm32-unknown-unknown/release/reinhardt_cloud_dashboard.wasm

FROM builder AS assets
COPY --from=wasm /wasm-dist /build/wasm-dist
WORKDIR /app/dashboard
# buildstatic uses the same full settings loader as collectstatic.
RUN build_secret="$(od -An -N32 -tx1 /dev/urandom | tr -d ' \n')" && \
    REINHARDT_ENV=ci \
    REINHARDT_CORE__SECRET_KEY="$build_secret" \
    REINHARDT_CLOUD_JWT_SECRET="$build_secret" \
    REINHARDT_DATABASE_PASSWORD="$build_secret" \
    /app/target/release/manage buildstatic \
    --pages-dir /build/wasm-dist \
    --pages-entry reinhardt_cloud_dashboard.js \
    --pages-document index.html \
    --package reinhardt-cloud-dashboard --all-features

FROM debian:bookworm-slim AS runtime
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates tini libpq5 && \
    rm -rf /var/lib/apt/lists/*
RUN useradd --create-home appuser
WORKDIR /app
COPY --from=builder /app/target/release/reinhardt-cloud-dashboard /app/
COPY --from=builder /app/target/release/manage /app/
COPY --from=assets /app/dashboard/static /app/static
COPY --from=builder /app/dashboard/settings /app/settings
COPY --from=builder /app/dashboard/migrations /app/migrations
RUN chown -R appuser:appuser /app
# Run as non-root
USER appuser
ENV RUST_LOG=info \
    PATH=/app:$PATH \
    REINHARDT_CLOUD_CONFIG_DIR=/app/settings \
    REINHARDT_ENV=production
EXPOSE 8000
ENTRYPOINT ["tini", "--", "/app/reinhardt-cloud-dashboard"]
