#!/usr/bin/env bash
set -e
: "${VITE_MODULE_NAME:?}"
: "${VITE_SPACETIMEDB_URI:?}"
: "${VITE_PORT:?}"
: "${OIDC_ISSUER:?}"
: "${OIDC_CLIENT_ID:?}"
: "${OIDC_REDIRECT_URI:?}"
[ "$OIDC_ISSUER" = 'http://127.0.0.1:9090/realms/stack-bench' ] || { echo 'Unexpected OIDC issuer' >&2; exit 1; }
[ "$OIDC_CLIENT_ID" = 'storefront' ] || { echo 'Unexpected OIDC client' >&2; exit 1; }
export VITE_OIDC_ISSUER="$OIDC_ISSUER"
export VITE_OIDC_CLIENT_ID="$OIDC_CLIENT_ID"
export VITE_OIDC_REDIRECT_URI="$OIDC_REDIRECT_URI"
[ -d backend/spacetimedb/node_modules ] || npm --prefix backend/spacetimedb ci --no-audit --no-fund
[ -d client/node_modules ] || npm --prefix client ci --no-audit --no-fund
/deps/spacetimedb-cli generate --lang typescript --module-path /app/backend/spacetimedb --out-dir /app/client/src/module_bindings --yes --no-config
npm --prefix client run build
exec npm --prefix client run dev -- --host 0.0.0.0 --port "$VITE_PORT" --strictPort
