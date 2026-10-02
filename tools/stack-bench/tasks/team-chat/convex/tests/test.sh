#!/usr/bin/env bash
# Thin launcher — Harbor's verifier entry point. Runs the shared team-chat
# behavioral scenario through the Convex adapter.
set -euo pipefail
cd "$(dirname "$0")"
npm install --no-audit --no-fund --silent
export RESTART_CMD="${RESTART_CMD:-/opt/stack-bench/backendctl restart}"
exec npx tsx harness/src/runTeamChat.ts "$(pwd)/adapter.ts"
