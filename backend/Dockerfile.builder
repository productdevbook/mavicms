# syntax=docker/dockerfile:1
#
# The builder runs other people's build scripts, so it carries a toolchain the
# API image has no reason to: git to fetch a project and bun to build it.

FROM rust:1.97-slim-bookworm AS base
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

RUN cargo install cargo-chef --locked --version 0.1.72

FROM base AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
RUN cargo build --release --bin mavicms-builder

FROM debian:bookworm-slim AS runtime
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    unzip \
    curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --shell /bin/bash mavicms

# Bun, because that is what these projects are built with. Installed as the
# user that will run it, so a build never needs to be root for anything.
#
# The canary rather than the latest release: bun writes a lockfile its own
# version can read and older ones refuse, and projects are already pinning
# versions that live only in the canary. An image a release behind cannot read
# their lockfiles, ignores them, and resolves every dependency afresh — which
# is how two publishes of one commit ship different code.
#
# The cost is that this line means something different on every rebuild. Each
# build prints the version it actually ran with, so the log says which one.
USER mavicms
ENV BUN_INSTALL=/home/mavicms/.bun
RUN curl -fsSL https://bun.sh/install | bash -s canary
ENV PATH="/home/mavicms/.bun/bin:${PATH}"

COPY --from=builder /app/target/release/mavicms-builder /usr/local/bin/mavicms-builder

ENV MAVICMS_DATA_DIR=/data \
    MAVICMS_WORKSPACE=/workspace

ENTRYPOINT ["mavicms-builder"]
