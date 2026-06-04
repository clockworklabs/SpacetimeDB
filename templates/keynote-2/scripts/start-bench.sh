#!/usr/bin/env bash
# Start every backend service we bench in its own tmux window inside session "bench".
# Idempotent — re-running kills+recreates each named window.
#
# Prerequisites the script doesn't manage:
#   - Postgres running as a system service (sudo systemctl start postgresql)
#   - Supabase Docker stack up (supabase start)
#   - bun_bench database created in Postgres
#   - .env populated (CONVEX_URL etc.)

set -uo pipefail

SESSION=bench
ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." &> /dev/null && pwd)"

tmux has-session -t "$SESSION" 2>/dev/null || tmux new-session -d -s "$SESSION" -n bench

start_window() {
  local name=$1; shift
  local cmd="$*"
  tmux kill-window -t "${SESSION}:${name}" 2>/dev/null || true
  tmux new-window -a -t "${SESSION}:" -n "${name}" \
    "bash -c '${cmd}; rc=\$?; echo; echo \"[${name} exited rc=\$rc]\"; read'"
}

start_window sqlite-rpc     "cd $ROOT && pnpm tsx src/rpc-servers/sqlite-rpc-server.ts"
start_window postgres-rpc   "cd $ROOT && pnpm tsx src/rpc-servers/postgres-rpc-server.ts"
start_window bun-rpc        "cd $ROOT && bun run bun/bun-server.ts"

start_window cockroach      "mkdir -p /tmp/crdb-data && cockroach start-single-node --insecure --listen-addr=127.0.0.1:26257 --http-addr=127.0.0.1:8081 --store=/tmp/crdb-data"
sleep 5
start_window cockroach-rpc  "cd $ROOT && pnpm tsx src/rpc-servers/cockroach-rpc-server.ts"

start_window supabase-rpc   "cd $ROOT && pnpm tsx src/rpc-servers/supabase-rpc-server.ts"
start_window convex         "cd $ROOT/convex-app && npx convex dev --local"

# PlanetScale RPC server: reuses postgres-rpc-server.ts but with PLANETSCALE_PG_URL
# and a different port (4104) to avoid colliding with local postgres-rpc on 4101.
# Only starts if PLANETSCALE_PG_URL is set in .env.
if [ -f "$ROOT/.env" ] && grep -q '^PLANETSCALE_PG_URL=postgres' "$ROOT/.env"; then
  start_window planetscale-rpc "cd $ROOT && set -a && . ./.env && set +a && PG_URL=\"\$PLANETSCALE_PG_URL\" PG_RPC_PORT=4104 pnpm tsx src/rpc-servers/postgres-rpc-server.ts"
fi

echo "All windows started in tmux session '$SESSION'."
echo "Attach: tmux attach -t $SESSION   (then Ctrl+B w to browse)"
