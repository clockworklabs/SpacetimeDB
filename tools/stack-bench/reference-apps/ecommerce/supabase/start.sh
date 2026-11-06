#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
[ -d node_modules ] || npm ci --no-audit --no-fund
[ -d client/node_modules ] || npm --prefix client ci --no-audit --no-fund
npm run migrate
npm run seed
export VITE_SUPABASE_URL="${VITE_SUPABASE_URL:-${SUPABASE_URL}}"
export VITE_SUPABASE_ANON_KEY="${VITE_SUPABASE_ANON_KEY:-${SUPABASE_ANON_KEY}}"
npm --prefix client run build
exec npm --prefix client run preview -- --host 0.0.0.0 --port "${VITE_PORT:?VITE_PORT is required}"
