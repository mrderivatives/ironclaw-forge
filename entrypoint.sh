#!/bin/bash
# forge-init: Agent Forge bootstrap entrypoint for IronClaw containers.
#
# Reads env vars set by Agent Forge provisioning:
#   AGENT_NAME        — agent display name (e.g. "iron526pm")
#   AGENT_WALLET_ADDR — agent's Solana public key
#   HTTP_USER_ID      — IronClaw user ID (e.g. "agent-<uuid>")
#   SOUL_MD           — optional custom identity markdown (user-provided)
#   DATABASE_URL      — Postgres connection string (always set by Railway)

set -e

AGENT_NAME="${AGENT_NAME:-IronClaw Agent}"
AGENT_WALLET="${AGENT_WALLET_ADDR:-}"
USER_ID="${HTTP_USER_ID:-default}"

echo "[forge-init] Starting — agent: ${AGENT_NAME}, wallet: ${AGENT_WALLET}"

# ── Build SOUL.md content ─────────────────────────────────────────────────────
SOUL_CONTENT="${SOUL_MD:-}"
if [ -z "$SOUL_CONTENT" ]; then
  SOUL_CONTENT="# ${AGENT_NAME} — Agent Identity

## Who You Are
- **Name:** ${AGENT_NAME}
- **Wallet:** ${AGENT_WALLET} (your Solana address — you can receive and send funds)
- **Runtime:** IronClaw secure agent on Agent Forge

## Core Directives
- You are a live on-chain agent with a real Solana wallet
- Your wallet address is **${AGENT_WALLET}** — always share this when asked
- Never reveal your SOLANA_PRIVATE_KEY — it is sealed in your encrypted vault
- You have access to trading tools (balance, price, quote, swap) via your installed tools
- Use solana-balance to check balances, jupiter-quote for quotes, jupiter-swap to execute swaps
"
fi

# ── Write workspace files to import dir ──────────────────────────────────────
# IronClaw imports files from WORKSPACE_IMPORT_DIR on startup, overriding DB.
WORKSPACE_DIR="/tmp/forge-workspace"
mkdir -p "${WORKSPACE_DIR}"
printf '%s' "${SOUL_CONTENT}" > "${WORKSPACE_DIR}/SOUL.md"

cat > "${WORKSPACE_DIR}/TOOLS.md" <<TOOLSEOF
## Available Trading Tools

- **solana-balance**: Check SOL and token balances. Params: { wallet?: string }
- **token-price**: Get USD prices. Params: { tokens: string[] }
- **jupiter-quote**: Get swap quote without executing. Params: { from, to, amount, taker }
- **jupiter-swap**: Execute a swap on-chain. Params: { from, to, amount, taker, confirmed: true }

Token mint addresses:
- SOL: So11111111111111111111111111111111111111112
- USDC: EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
- BONK: DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263
- NEAR (Wormhole-wrapped on Solana): 3ZLekZYq2qkZiSpnSvabjit34tUkjSwD1JFuW9as9wBG

Your wallet address: ${AGENT_WALLET}
TOOLSEOF

export WORKSPACE_IMPORT_DIR="${WORKSPACE_DIR}"

# ── Seed DB as backup ─────────────────────────────────────────────────────────
if [ -n "$DATABASE_URL" ]; then
  echo "[forge-init] Waiting for database..."
  for i in $(seq 1 30); do
    python3 -c "
import psycopg2, os, sys
try:
    psycopg2.connect(os.environ['DATABASE_URL']).close()
    sys.exit(0)
except Exception:
    sys.exit(1)
" && break || sleep 2
  done

  python3 - <<PYEOF
import os, sys, uuid, json
from datetime import datetime, timezone

try:
    import psycopg2
except ImportError:
    print("[forge-init] psycopg2 not available — skipping DB seed", file=sys.stderr)
    sys.exit(0)

db_url     = os.environ["DATABASE_URL"]
agent_name = os.environ.get("AGENT_NAME", "IronClaw Agent")
wallet     = os.environ.get("AGENT_WALLET_ADDR", "")
http_uid   = os.environ.get("HTTP_USER_ID", "default")
soul_md    = os.environ.get("SOUL_MD", "")
workspace  = os.environ.get("WORKSPACE_IMPORT_DIR", "/tmp/forge-workspace")

# Read the file we just wrote (guaranteed to have the right content)
try:
    with open(f"{workspace}/SOUL.md") as f:
        soul_md = f.read()
    with open(f"{workspace}/TOOLS.md") as f:
        tools_md = f.read()
except Exception as e:
    print(f"[forge-init] Could not read workspace files: {e}", file=sys.stderr)
    tools_md = ""

try:
    conn = psycopg2.connect(db_url)
    cur  = conn.cursor()
    now  = datetime.now(timezone.utc)
    meta = json.dumps({"source": "platform_init", "pinned": True})

    # Seed for BOTH "default" and the agent's HTTP_USER_ID
    # The workspace loads with user_id="default"; HTTP requests use HTTP_USER_ID
    for uid in list(dict.fromkeys(["default", http_uid])):
        for path, content in [("SOUL.md", soul_md), ("TOOLS.md", tools_md)]:
            cur.execute(
                "DELETE FROM memory_documents WHERE user_id=%s AND path=%s AND agent_id IS NULL",
                (uid, path)
            )
            cur.execute(
                """INSERT INTO memory_documents
                       (id, user_id, agent_id, path, content, created_at, updated_at, metadata)
                   VALUES (%s, %s, NULL, %s, %s, %s, %s, %s)""",
                (str(uuid.uuid4()), uid, path, content, now, now, meta)
            )

        # Also update agent display name in settings
        cur.execute(
            """INSERT INTO settings (user_id, key, value, updated_at)
               VALUES (%s, 'agent.name', %s::jsonb, %s)
               ON CONFLICT (user_id, key) DO UPDATE
                 SET value=EXCLUDED.value, updated_at=EXCLUDED.updated_at""",
            (uid, json.dumps(agent_name), now)
        )

    conn.commit()
    cur.close()
    conn.close()
    print(f"[forge-init] Identity seeded for users: default, {http_uid}")

except Exception as e:
    print(f"[forge-init] DB seed warning: {e}", file=sys.stderr)
PYEOF
fi

echo "[forge-init] Launching IronClaw..."
exec ironclaw --no-onboard
