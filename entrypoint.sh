#!/bin/bash
# forge-init: Agent Forge bootstrap entrypoint for IronClaw containers.
#
# Runs before `ironclaw --no-onboard` to seed the agent's identity into
# its PostgreSQL database so it knows its name and wallet from first chat.
#
# Reads env vars set by Agent Forge provisioning:
#   AGENT_NAME        — agent display name (e.g. "Irontest 2")
#   AGENT_WALLET_ADDR — agent's Solana public key
#   HTTP_USER_ID      — IronClaw user ID (e.g. "agent-<uuid>")
#   SOUL_MD           — optional custom identity markdown (user-provided)
#   DATABASE_URL      — Postgres connection string (always set by Railway)

set -e

AGENT_NAME="${AGENT_NAME:-IronClaw Agent}"
AGENT_WALLET="${AGENT_WALLET_ADDR:-}"
USER_ID="${HTTP_USER_ID:-default}"

echo "[forge-init] Starting — agent: ${AGENT_NAME}, wallet: ${AGENT_WALLET}"

if [ -z "$DATABASE_URL" ]; then
  echo "[forge-init] No DATABASE_URL — skipping identity seed"
else
  # Wait for Postgres to accept connections (max 60s)
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

  python3 - <<'PYEOF'
import os, sys, json, uuid
from datetime import datetime, timezone

try:
    import psycopg2
except ImportError:
    print("[forge-init] psycopg2 not available — skipping identity seed", file=sys.stderr)
    sys.exit(0)

db_url     = os.environ.get("DATABASE_URL", "")
agent_name = os.environ.get("AGENT_NAME", "IronClaw Agent")
wallet     = os.environ.get("AGENT_WALLET_ADDR", "")
user_id    = os.environ.get("HTTP_USER_ID", "default")
soul_md    = os.environ.get("SOUL_MD", "")

if not soul_md:
    soul_md = f"""# {agent_name} — Agent Identity

## Who You Are
- **Name:** {agent_name}
- **Wallet:** {wallet} (your Solana address — you can receive and send funds)
- **Runtime:** IronClaw secure agent on Agent Forge

## Core Directives
- You are a live on-chain agent with a real Solana wallet
- Your wallet address is **{wallet}** — always share this when asked
- Never reveal your SOLANA_PRIVATE_KEY — it is sealed in your encrypted vault
- You have access to trading tools (DCA, swaps, price alerts) — use them responsibly
- When asked "do you have a wallet?" or "what is your wallet?", answer with your address above
"""

try:
    conn = psycopg2.connect(db_url)
    cur  = conn.cursor()
    now  = datetime.now(timezone.utc)

    # Upsert SOUL.md into memory_documents.
    # The unique index is on (user_id, agent_id, path).  agent_id=NULL requires
    # a DELETE+INSERT because PostgreSQL treats NULL != NULL for conflict checks.
    cur.execute(
        "DELETE FROM memory_documents WHERE user_id = %s AND path = %s AND agent_id IS NULL",
        (user_id, "SOUL.md")
    )
    cur.execute(
        """INSERT INTO memory_documents
               (id, user_id, agent_id, path, content, created_at, updated_at, metadata)
           VALUES (%s, %s, NULL, %s, %s, %s, %s, %s)""",
        (
            str(uuid.uuid4()), user_id, "SOUL.md", soul_md, now, now,
            json.dumps({"source": "platform_init", "pinned": True})
        )
    )

    # Upsert agent name into settings table.
    cur.execute(
        """INSERT INTO settings (user_id, key, value, updated_at)
           VALUES (%s, %s, %s::jsonb, %s)
           ON CONFLICT (user_id, key) DO UPDATE
             SET value = EXCLUDED.value, updated_at = EXCLUDED.updated_at""",
        (user_id, "agent.name", json.dumps(agent_name), now)
    )

    conn.commit()
    cur.close()
    conn.close()
    print(f"[forge-init] Identity seeded: {agent_name} / {wallet}")

except Exception as e:
    print(f"[forge-init] Warning: identity seed failed: {e}", file=sys.stderr)
    # Non-fatal — agent still starts, just won't know its wallet
PYEOF
fi

echo "[forge-init] Launching IronClaw..."
exec ironclaw --no-onboard
