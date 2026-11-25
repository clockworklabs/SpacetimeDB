#!/usr/bin/env bash
set -e
: "${SPACETIMEDB_MODULE:?}"
: "${SPACETIMEDB_URI:?}"
: "${APP_PORT:?}"
export VITE_MODULE_NAME="$SPACETIMEDB_MODULE" VITE_SPACETIMEDB_URI="$SPACETIMEDB_URI"
[ -d backend/spacetimedb/node_modules ] || npm --prefix backend/spacetimedb ci --no-audit --no-fund
[ -d client/node_modules ] || npm --prefix client ci --no-audit --no-fund
/deps/spacetimedb-cli generate --lang typescript --module-path /app/backend/spacetimedb --out-dir /app/client/src/module_bindings --yes --no-config
npm --prefix client run build
exec npm --prefix client run dev -- --host 0.0.0.0 --port "$APP_PORT" --strictPort
