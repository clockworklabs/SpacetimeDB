#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
[ -d node_modules ] || npm ci --no-audit --no-fund
[ -d client/node_modules ] || npm --prefix client ci --no-audit --no-fund
npm run prepare-auth
node node_modules/convex/bin/main.js deploy --yes --typecheck disable --codegen disable
npm run seed
export VITE_CONVEX_URL="${VITE_CONVEX_URL:-${CONVEX_SELF_HOSTED_URL}}"
npm --prefix client run build
exec npm --prefix client run preview -- --host 0.0.0.0 --port "${VITE_PORT:?VITE_PORT is required}"
