# syntax=docker/dockerfile:1

FROM rust:1.97-slim-bookworm AS base
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked --version 0.1.72

# Reduces the whole dependency tree to a recipe that changes only when the
# dependencies do, so editing this project's own code no longer rebuilds the
# six hundred crates underneath it.
FROM base AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --package mavicms-api

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /usr/sbin/nologin mavicms \
    && chown mavicms:mavicms /app

COPY --from=builder /app/target/release/mavicms-api /usr/local/bin/mavicms-api

# A fresh named volume inherits the ownership of the mount point in the image.
# Without this directory Docker creates it as root and the unprivileged process
# cannot write to its own data directory.
RUN mkdir -p /data && chown mavicms:mavicms /data
VOLUME /data

USER mavicms
EXPOSE 8080
ENV HOST=0.0.0.0 \
    PORT=8080 \
    MAVICMS_DATA_DIR=/data

ENTRYPOINT ["mavicms-api"]
