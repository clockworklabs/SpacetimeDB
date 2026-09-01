#!/usr/bin/env bash
set -e
: "${VITE_MODULE_NAME:?}"
: "${VITE_SPACETIMEDB_URI:?}"
: "${VITE_PORT:?}"
npm --prefix backend/spacetimedb ci --no-audit --no-fund
npm --prefix client ci --no-audit --no-fund
/deps/spacetimedb-cli generate --lang typescript --module-path /app/backend/spacetimedb --out-dir /app/client/src/module_bindings --yes --no-config
npm --prefix client run build
exec npm --prefix client run dev -- --host 0.0.0.0 --port "$VITE_PORT" --strictPort
