# syntax=docker/dockerfile:1

FROM rust:1.97-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

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

USER mavicms
EXPOSE 8080
ENV HOST=0.0.0.0 \
    PORT=8080

ENTRYPOINT ["mavicms-api"]
