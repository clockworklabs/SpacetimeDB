#!/bin/bash
# Reset an app's backend state for a clean test run.
# Publishes a fresh SpacetimeDB module or resets PostgreSQL tables.
#
# Usage:
#   ./reset-app.sh <app-dir>
#
# This gives Playwright a clean slate — no leftover users, rooms, or messages.

set -euo pipefail

APP_DIR="${1:?Usage: ./reset-app.sh <app-dir>}"

if [[ ! -d "$APP_DIR" ]]; then
  echo "ERROR: App directory not found: $APP_DIR"
  exit 1
fi

# Ensure spacetime is in PATH
SPACETIME_DIR="${USERPROFILE:-$HOME}/AppData/Local/SpacetimeDB"
if [[ -d "$SPACETIME_DIR" ]]; then
  export PATH="$PATH:$SPACETIME_DIR"
fi
_USER="${USER:-${USERNAME:-$(whoami)}}"
if [[ -d "/c/Users/$_USER/AppData/Local/SpacetimeDB" ]]; then
  export PATH="$PATH:/c/Users/$_USER/AppData/Local/SpacetimeDB"
fi

# Auto-detect backend
if [[ -d "$APP_DIR/backend/spacetimedb" ]]; then
  BACKEND="spacetime"
elif [[ -d "$APP_DIR/server" ]]; then
  BACKEND="postgres"
else
  echo "ERROR: Cannot detect backend in $APP_DIR"
  exit 1
fi

RESET_ID="test-$(date +%s)"

if [[ "$BACKEND" == "spacetime" ]]; then
  echo "Resetting SpacetimeDB module..."

  # Generate a fresh module name
  NEW_MODULE="chat-app-$RESET_ID"

  # Publish fresh module
  BACKEND_DIR="$APP_DIR/backend/spacetimedb"
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    BACKEND_DIR_NATIVE=$(cygpath -w "$BACKEND_DIR")
  else
    BACKEND_DIR_NATIVE="$BACKEND_DIR"
  fi

  echo "  Publishing module: $NEW_MODULE"
  spacetime publish -p "$BACKEND_DIR_NATIVE" -s local "$NEW_MODULE" 2>&1 | tail -3

  # Update client config to point at new module
  CONFIG_FILE="$APP_DIR/client/src/config.ts"
  if [[ -f "$CONFIG_FILE" ]]; then
    sed -i "s/MODULE_NAME = '.*'/MODULE_NAME = '$NEW_MODULE'/" "$CONFIG_FILE"
    echo "  Updated config.ts: MODULE_NAME = '$NEW_MODULE'"
  else
    echo "  WARNING: config.ts not found at $CONFIG_FILE"
  fi

  echo "  Module reset complete. Vite will hot-reload."

elif [[ "$BACKEND" == "postgres" ]]; then
  echo "Resetting PostgreSQL database..."

  # Find the database name from the server code or .env
  POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-llm-sequential-upgrade-postgres-1}"
  DB_NAME="spacetime"

  # Look for DATABASE_URL in the server to find the actual database
  SERVER_DIR="$APP_DIR/server"
  if [[ -f "$SERVER_DIR/.env" ]]; then
    DB_URL=$(grep DATABASE_URL "$SERVER_DIR/.env" | head -1 | cut -d= -f2-)
    DB_NAME=$(echo "$DB_URL" | sed 's|.*/||; s|?.*||')
  fi

  # Drop all tables and recreate via Drizzle push
  echo "  Dropping all tables in $DB_NAME..."
  docker exec "$POSTGRES_CONTAINER" psql -U spacetime -d "$DB_NAME" -c "
    DO \$\$ DECLARE
      r RECORD;
    BEGIN
      FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
        EXECUTE 'DROP TABLE IF EXISTS ' || quote_ident(r.tablename) || ' CASCADE';
      END LOOP;
    END \$\$;
  " 2>&1 | tail -1

  # Re-push Drizzle schema
  echo "  Pushing Drizzle schema..."
  cd "$SERVER_DIR"
  npx drizzle-kit push 2>&1 | tail -3
  cd - > /dev/null

  echo "  Database reset complete."
fi

echo "Reset complete for $BACKEND backend."
