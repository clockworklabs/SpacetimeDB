#!/usr/bin/env bash
# Wipe a generated app's data so a grading run starts from a known state.
#
# Grading dirty state silently lowers scores — an accumulated room or user list
# breaks assertions that pass on a clean app, with no error to show for it — so
# every suite resets first.
#
# Usage: reset-db.sh <backend> <app-dir> [run-index]
set -euo pipefail

BACKEND="${1:?backend required}"
APP_DIR="${2:?app dir required}"
RUN_INDEX="${3:-0}"

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-stack-bench-postgres}"
MONGO_CONTAINER="${MONGO_CONTAINER:-stack-bench-mongodb}"
DB_NAME="stackbench_run${RUN_INDEX}"

case "$BACKEND" in
  spacetime)
    # Republishing with --delete-data clears the module's tables in place, so the
    # client keeps pointing at the same module name.
    MODULE=$(grep -oE "MODULE_NAME\s*=\s*'[^']+'" "$APP_DIR/client/src/config.ts" 2>/dev/null | grep -oE "'[^']+'" | tr -d "'")
    MODULE="${MODULE:-stackbench-run${RUN_INDEX}}"
    echo y | spacetime publish "$MODULE" --module-path "$APP_DIR/backend/spacetimedb" --delete-data >/dev/null 2>&1
    echo "reset spacetime module $MODULE"
    ;;

  postgres)
    docker exec "$POSTGRES_CONTAINER" psql -U stackbench -d "$DB_NAME" -c "
      DO \$\$ DECLARE r RECORD;
      BEGIN
        FOR r IN (SELECT tablename FROM pg_tables WHERE schemaname = 'public') LOOP
          EXECUTE 'TRUNCATE TABLE ' || quote_ident(r.tablename) || ' RESTART IDENTITY CASCADE';
        END LOOP;
      END \$\$;" >/dev/null
    echo "reset postgres database $DB_NAME"
    ;;

  mongodb)
    docker exec "$MONGO_CONTAINER" mongosh "$DB_NAME" --quiet --eval "db.dropDatabase()" >/dev/null
    echo "reset mongodb database $DB_NAME"
    ;;

  *)
    echo "unknown backend: $BACKEND" >&2
    exit 2
    ;;
esac
