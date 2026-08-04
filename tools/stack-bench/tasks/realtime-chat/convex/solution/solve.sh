#!/usr/bin/env bash
# Thin launcher — Harbor's oracle entry point. Deploys the reference Convex app to
# the self-hosted backend service. Readiness is gated by the task.toml healthcheck.
set -euo pipefail
cd "$(dirname "$0")/app"
npm install --no-audit --no-fund --silent
export CONVEX_SELF_HOSTED_URL="${CONVEX_SELF_HOSTED_URL:-http://convex-backend:3210}"
# Deterministic admin key for the fixed INSTANCE_NAME/SECRET in environment/docker-compose.yaml.
export CONVEX_SELF_HOSTED_ADMIN_KEY="${CONVEX_SELF_HOSTED_ADMIN_KEY:-convex-stack-bench|01be067fd1488e360c17a915fd342953e6450766d4831138261df4371e27342009f370391e}"
exec npx convex deploy
