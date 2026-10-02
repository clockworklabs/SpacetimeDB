#!/usr/bin/env bash
# Thin launcher — Harbor's verifier entry point (Harbor requires test.sh on Linux).
# Runs the shared behavioral scenario through the SpacetimeDB adapter; the harness
# writes the reward to /logs/verifier/reward.txt. Connects to localhost (SHARED mode).
set -euo pipefail
cd "$(dirname "$0")"
npm install --no-audit --no-fund --silent
exec npx tsx harness/src/runScenario.ts "$(pwd)/adapter.ts"
