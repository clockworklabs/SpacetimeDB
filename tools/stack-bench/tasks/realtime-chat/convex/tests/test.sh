#!/usr/bin/env bash
# Thin launcher — Harbor's verifier entry point. Runs the shared behavioral scenario
# through the Convex adapter; the harness writes /logs/verifier/reward.txt. Connects
# to the convex-backend compose service (SHARED mode).
set -euo pipefail
cd "$(dirname "$0")"
npm install --no-audit --no-fund --silent
exec npx tsx harness/src/runScenario.ts "$(pwd)/adapter.ts"
