# Agent Forge IronClaw image.
#
# Two-stage build:
#   1. base  — vanilla IronClaw binary compiled from nearai/ironclaw source
#   2. forge — adds python3 + psycopg2 and our forge-init entrypoint
#
# The forge stage wraps the binary so agents know their name and wallet
# from the very first chat message.

# Build from our fork which includes:
#   - sign-bytes / pubkey-for WIT host primitives (ed25519 signing)
#   - Pre-load allowed secrets into credentials map before WASM execution
ARG IRONCLAW_REF=staging

# ── Stage 1: build IronClaw from source ────────────────────────────────────
FROM rust:1.85-slim-bookworm AS builder

ARG IRONCLAW_REF

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev cmake gcc g++ git \
    && rm -rf /var/lib/apt/lists/* \
    && rustup target add wasm32-wasip2

WORKDIR /app
RUN git clone --depth 1 --branch ${IRONCLAW_REF} \
    https://github.com/mrderivatives/ironclaw-core.git .

COPY tools-src /app/tools-src
# Sync WIT from ironclaw-core (has sign-bytes / pubkey-for) into tools build tree
RUN cp /app/wit/tool.wit /app/tools-src/wit/tool.wit
RUN cd /app/tools-src && cargo build --release --target wasm32-wasip2 --workspace
# wasm32-wasip2 already outputs WASM components — copy directly, no wasm-tools needed
RUN mkdir -p /app/tools-dist && \
    for tool in solana-balance token-price jupiter-quote jupiter-swap; do \
      cp /app/tools-src/target/wasm32-wasip2/release/$(echo $tool | tr - _).wasm \
         /app/tools-dist/${tool}.wasm && \
      cp /app/tools-src/${tool}/capabilities.json /app/tools-dist/${tool}.capabilities.json; \
    done

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
RUN mkdir -p /home/ironclaw/.config/ironclaw/tools && chown -R ironclaw:ironclaw /home/ironclaw/.config
COPY --from=builder /app/tools-dist/*.wasm /home/ironclaw/.config/ironclaw/tools/
COPY --from=builder /app/tools-dist/*.capabilities.json /home/ironclaw/.config/ironclaw/tools/
RUN chown -R ironclaw:ironclaw /home/ironclaw/.config/ironclaw/tools
USER ironclaw

EXPOSE 3000 8080

ENV RUST_LOG=ironclaw=info

ENTRYPOINT ["/usr/local/bin/forge-init"]
