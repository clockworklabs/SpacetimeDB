#!/usr/bin/env bash
# Wipe a generated app's data so a grading run starts from a known state.
#
# Grading dirty state silently lowers scores — an accumulated room or user list
# breaks assertions that pass on a clean app, with no error to show for it — so
# every suite resets first.
#
# Usage: reset-db.sh <backend> <app-dir> [run-index] [track-slug]
#
# The track slug separates one application's databases from another's, so a run
# of one track cannot wipe the other's data. Empty (the default) is the chat
# track, whose names predate tracks and must not change.
set -euo pipefail

BACKEND="${1:?backend required}"
APP_DIR="${2:?app dir required}"
RUN_INDEX="${3:-0}"
SLUG="${4:-}"

POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-stack-bench-postgres}"
MONGO_CONTAINER="${MONGO_CONTAINER:-stack-bench-mongodb}"
DB_NAME="stackbench${SLUG:+_$SLUG}_run${RUN_INDEX}"
MODULE_FALLBACK="stackbench${SLUG:+-$SLUG}-run${RUN_INDEX}"

case "$BACKEND" in
  spacetime)
    # Republishing with --delete-data clears the module's tables in place, so the
    # client keeps pointing at the same module name.
    MODULE=$(grep -oE "MODULE_NAME\s*=\s*'[^']+'" "$APP_DIR/client/src/config.ts" 2>/dev/null | grep -oE "'[^']+'" | tr -d "'")
    MODULE="${MODULE:-$MODULE_FALLBACK}"

    # Reset the host the APP READS, not the one we would prefer it used. These
    # drifted apart during the move to a benchmark-owned instance: the reset
    # published to :3210 while the app served :3000, reported success, and every
    # later grade ran against an accumulating database. A reset that resets
    # something else is worse than no reset, because it is silent.
    APP_URI=$(grep -oE "URI\s*=\s*'[^']+'" "$APP_DIR/client/src/config.ts" 2>/dev/null | grep -oE "'[^']+'" | tr -d "'")
    STDB_URI="${APP_URI:-${STACK_BENCH_STDB_URI:-http://127.0.0.1:3210}}"
    if [ -n "$APP_URI" ] && [ -n "${STACK_BENCH_STDB_URI:-}" ] && [ "$APP_URI" != "$STACK_BENCH_STDB_URI" ]; then
      echo "note: app targets $APP_URI but STACK_BENCH_STDB_URI is $STACK_BENCH_STDB_URI — resetting the app's own host" >&2
    fi

    if ! echo y | spacetime publish "$MODULE" --module-path "$APP_DIR/backend/spacetimedb" \
         -s "$STDB_URI" --delete-data > /tmp/reset-spacetime.log 2>&1; then
      echo "FAILED to reset spacetime module $MODULE on $STDB_URI" >&2
      tail -3 /tmp/reset-spacetime.log >&2
      exit 1
    fi
    echo "reset spacetime module $MODULE on $STDB_URI"
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
