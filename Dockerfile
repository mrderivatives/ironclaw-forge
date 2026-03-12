# Agent Forge IronClaw image.
#
# Two-stage build:
#   1. base  — vanilla IronClaw binary compiled from nearai/ironclaw source
#   2. forge — adds python3 + psycopg2 and our forge-init entrypoint
#
# The forge stage wraps the binary so agents know their name and wallet
# from the very first chat message.

ARG IRONCLAW_REF=main

# ── Stage 1: build IronClaw from source ────────────────────────────────────
FROM rust:1.92-slim-bookworm AS builder

ARG IRONCLAW_REF

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake gcc g++ git \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-wasip2 \
    && cargo install wasm-tools

WORKDIR /app
RUN git clone --depth 1 --branch ${IRONCLAW_REF} \
    https://github.com/nearai/ironclaw.git . \
    || git clone --depth 1 https://github.com/nearai/ironclaw.git .

RUN cargo build --release --bin ironclaw

# ── Stage 2: forge runtime ─────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    python3 python3-psycopg2 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/ironclaw /usr/local/bin/ironclaw
COPY --from=builder /app/migrations /app/migrations

COPY entrypoint.sh /usr/local/bin/forge-init
RUN chmod +x /usr/local/bin/forge-init

RUN useradd -m -u 1000 -s /bin/bash ironclaw
USER ironclaw

EXPOSE 3000 8080

ENV RUST_LOG=ironclaw=info

ENTRYPOINT ["/usr/local/bin/forge-init"]
