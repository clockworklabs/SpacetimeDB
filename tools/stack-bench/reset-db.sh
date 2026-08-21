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

SPACETIME_BIN="${SPACETIME_BIN:-$(cd "$(dirname "$0")/../.." && pwd)/target/release/spacetimedb-cli.exe}"
LEASE_CLI="$(cd "$(dirname "$0")" && pwd)/commands/lease-cli.mjs"
NODE_BIN="${STACK_BENCH_NODE_BIN:-node}"
command -v "$NODE_BIN" >/dev/null 2>&1 || NODE_BIN=node.exe
LEASE_CLI_NODE="$LEASE_CLI"
case "$NODE_BIN" in
  *.exe)
    LEASE_CLI_NODE="$(wslpath -w "$LEASE_CLI")"
    export STACK_BENCH_LEASE="$(wslpath -w "${STACK_BENCH_LEASE:-}")"
    CLEAN_WSLENV=""
    IFS=':' read -ra WSL_VARS <<< "${WSLENV:-}"
    for WSL_VAR in "${WSL_VARS[@]}"; do
      [ "$WSL_VAR" = "STACK_BENCH_LEASE/p" ] && continue
      CLEAN_WSLENV="${CLEAN_WSLENV:+$CLEAN_WSLENV:}$WSL_VAR"
    done
    export WSLENV="$CLEAN_WSLENV"
    ;;
esac

# Merely setting an "owned" boolean is not ownership: it identifies neither
# the run nor the destructive target. Every target below comes from the
# harness-created, token-authenticated lease. The positional arguments select
# behavior only and cannot redirect a delete.
"$NODE_BIN" "$LEASE_CLI_NODE" validate "$BACKEND"

case "$BACKEND" in
  spacetime)
    # Republishing with --delete-data clears the module's tables in place, so the
    # client keeps pointing at the same module name.
    # The harness owns the destructive target. Generated application output is
    # untrusted and must never choose the module or server passed to
    # `publish --delete-data`.
    MODULE="$("$NODE_BIN" "$LEASE_CLI_NODE" field spacetime module)"
    STDB_URI="$("$NODE_BIN" "$LEASE_CLI_NODE" field spacetime serverUri)"

    if ! echo y | "$SPACETIME_BIN" publish "$MODULE" --module-path "$APP_DIR/backend/spacetimedb" \
         -s "$STDB_URI" --delete-data > /tmp/reset-spacetime.log 2>&1; then
      echo "FAILED to reset spacetime module $MODULE on $STDB_URI" >&2
      tail -3 /tmp/reset-spacetime.log >&2
      exit 1
    fi
    echo "reset spacetime module $MODULE on $STDB_URI"
    ;;

  postgres)
    DB_NAME="$("$NODE_BIN" "$LEASE_CLI_NODE" field postgres database)"
    POSTGRES_CONTAINER="$("$NODE_BIN" "$LEASE_CLI_NODE" field postgres containerName)"
    EXPECTED_CONTAINER_ID="$("$NODE_BIN" "$LEASE_CLI_NODE" field postgres containerId)"
    ACTUAL_CONTAINER_ID="$(docker inspect --format '{{.Id}}' "$POSTGRES_CONTAINER")"
    [ "$ACTUAL_CONTAINER_ID" = "$EXPECTED_CONTAINER_ID" ] || {
      echo "refusing reset: postgres container changed after lease creation" >&2; exit 3;
    }
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
    DB_NAME="$("$NODE_BIN" "$LEASE_CLI_NODE" field mongodb database)"
    MONGO_CONTAINER="$("$NODE_BIN" "$LEASE_CLI_NODE" field mongodb containerName)"
    EXPECTED_CONTAINER_ID="$("$NODE_BIN" "$LEASE_CLI_NODE" field mongodb containerId)"
    ACTUAL_CONTAINER_ID="$(docker inspect --format '{{.Id}}' "$MONGO_CONTAINER")"
    [ "$ACTUAL_CONTAINER_ID" = "$EXPECTED_CONTAINER_ID" ] || {
      echo "refusing reset: mongodb container changed after lease creation" >&2; exit 3;
    }
    docker exec "$MONGO_CONTAINER" mongosh "$DB_NAME" --quiet --eval "db.dropDatabase()" >/dev/null
    echo "reset mongodb database $DB_NAME"
    ;;

  *)
    echo "unknown backend: $BACKEND" >&2
    exit 2
    ;;
esac
