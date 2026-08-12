#!/usr/bin/env bash
# Restart the backend process an app depends on, so scheduled work can be tested
# for durability. The equivalent component differs per stack:
#
#   postgres / mongodb : the Express server process holding in-process timers
#   spacetime          : the SpacetimeDB host running the module
#
# Restarting only the Express side would not be a fair comparison, so the
# SpacetimeDB host is restarted too.
#
# Usage: restart-backend.sh <backend> <app-dir> [express-port] [probe-path] [mode]
#
# mode: restart (default) | stop | start. Stop/start exist for the
# deploy-window test: a back-office write lands WHILE the server is down, and
# the app must converge once it returns. For spacetime, stop/start are no-ops
# on the app tier — the module host is the database and is deliberately not
# touched; there is no app server to miss anything.
#
# The probe path is whatever endpoint proves this application's server is
# answering again; it differs per track, so it is passed in rather than assumed.
set -euo pipefail

SPACETIME_BIN="${SPACETIME_BIN:-$(cd "$(dirname "$0")/../.." && pwd)/target/release/spacetimedb-cli.exe}"
LEASE_CLI="$(cd "$(dirname "$0")" && pwd)/lease-cli.mjs"
NODE_BIN="${STACK_BENCH_NODE_BIN:-node}"
command -v "$NODE_BIN" >/dev/null 2>&1 || NODE_BIN=node.exe
LEASE_CLI_NODE="$LEASE_CLI"
case "$NODE_BIN" in
  *.exe)
    LEASE_CLI_NODE="$(wslpath -w "$LEASE_CLI")"
    export STACK_BENCH_LEASE="$(wslpath -w "${STACK_BENCH_LEASE:-}")"
    # We converted explicitly for node.exe, so remove the /p bridge before the
    # WSL -> Windows boundary or WSL will translate the Windows path again.
    CLEAN_WSLENV=""
    IFS=':' read -ra WSL_VARS <<< "${WSLENV:-}"
    for WSL_VAR in "${WSL_VARS[@]}"; do
      [ "$WSL_VAR" = "STACK_BENCH_LEASE/p" ] && continue
      CLEAN_WSLENV="${CLEAN_WSLENV:+$CLEAN_WSLENV:}$WSL_VAR"
    done
    export WSLENV="$CLEAN_WSLENV"
    ;;
esac
BACKEND="${1:?backend required}"
APP_DIR="${2:?app dir required}"
PORT="${3:-}"
PROBE="${4:-/api/rooms}"
MODE="${5:-restart}"

wait_for() {  # wait_for <url> <seconds>
  local url="$1" deadline=$(( $(date +%s) + ${2:-60} ))
  until curl -s -o /dev/null -m 3 "$url"; do
    [ "$(date +%s)" -gt "$deadline" ] && { echo "timed out waiting for $url" >&2; return 1; }
    sleep 2
  done
}

# A containerised run keeps the app — and its Express server — inside a
# long-lived container, so restarting it from the host cannot work: the process
# is in another namespace, and the app's node_modules hold linux-x64 esbuild and
# rollup binaries that this host cannot execute anyway.
#
# The same logic therefore runs INSIDE the container. The script is piped in on
# stdin rather than mounted, so no harness file ever appears on the filesystem
# the build can read — the property the container exists for.
#
# Only the app tier redirects. The spacetime branch below restarts the
# SpacetimeDB host, which is a machine-level service and stays on the host.
if [ -z "${STACK_BENCH_IN_CONTAINER:-}" ] && [ "$BACKEND" != "spacetime" ]; then
  ISO_FILE="$(dirname "$APP_DIR")/.stack-bench-isolation"
  if [ -f "$ISO_FILE" ] && [ "$(cat "$ISO_FILE")" = "container" ]; then
    "$NODE_BIN" "$LEASE_CLI_NODE" validate "$BACKEND"
    NAME="$("$NODE_BIN" "$LEASE_CLI_NODE" field "$BACKEND" buildContainerName)"
    EXPECTED_ID="$("$NODE_BIN" "$LEASE_CLI_NODE" field "$BACKEND" buildContainerId)"
    ACTUAL_ID="$(docker inspect --format '{{.Id}}' "$NAME")"
    [ "$ACTUAL_ID" = "$EXPECTED_ID" ] || {
      echo "refusing restart: build container changed after lease creation" >&2; exit 3;
    }
    # Git Bash rewrites a container-side /app into a Windows path before docker
    # ever sees it.
    export MSYS_NO_PATHCONV=1
    # `tr -d '\r'`: this file is checked out with CRLF on Windows (git reports
    # i/lf w/crlf). Git Bash tolerates that; the container's Linux bash does not
    # — it reads `set -euo pipefail\r` and rejects "pipefail\r" as an option
    # name, so the script dies on line 21 and the restart silently does nothing.
    tr -d '\r' < "$0" | docker exec -i -e STACK_BENCH_IN_CONTAINER=1 "$NAME" \
      bash -s -- "$BACKEND" /app "$PORT" "$PROBE" "$MODE"
    exit $?
  fi
fi

case "$BACKEND" in
  postgres|mongodb)
    [ -n "$PORT" ] || { echo "express port required for $BACKEND" >&2; exit 2; }
    if [ "$MODE" != "start" ]; then
      echo "stopping Express on :$PORT"
      npx --yes kill-port "$PORT" >/dev/null 2>&1 || true
      sleep 3
      # Verify it actually died, because kill-port reports success when it
      # cannot see the process at all: with no lsof on the box it printed
      # "Process on port N killed", exited 0, and left the server running. A
      # restart that silently does nothing turns every durability and
      # deploy-window test into a pass that means nothing, so this is a hard
      # failure rather than a warning.
      if curl -s -o /dev/null -m 3 "http://localhost:$PORT$PROBE"; then
        echo "still answering on :$PORT after kill-port — refusing to report a restart that did not happen" >&2
        exit 3
      fi
    fi
    [ "$MODE" = "stop" ] && exit 0
    # Where the server lives is a prescribed-stack assumption. A stack-free app
    # put package.json at the app root with only src/ under server/, so
    # `cd server && npm run dev` found no package.json, the restart timed out,
    # and grading aborted. Start from the directory that actually declares a
    # start script, preferring the conventional one when it is there.
    SERVER_DIR=""
    for d in "$APP_DIR/server" "$APP_DIR"; do
      [ -f "$d/package.json" ] && { SERVER_DIR="$d"; break; }
    done
    if [ -z "$SERVER_DIR" ]; then
      echo "no package.json in $APP_DIR/server or $APP_DIR — cannot restart this app" >&2
      exit 2
    fi
    # `dev` is the conventional script; fall back to `start` if the app named it
    # that way. Anything else and the app has not made itself restartable.
    SCRIPT=dev
    node -e "process.exit(require('$SERVER_DIR/package.json').scripts?.dev?0:1)" 2>/dev/null || SCRIPT=start
    ( cd "$SERVER_DIR" && PORT="$PORT" nohup npm run "$SCRIPT" < /dev/null > "/tmp/restart-$BACKEND-$PORT.log" 2>&1 & )
    wait_for "http://localhost:$PORT$PROBE" 180
    echo "Express on :$PORT is back"
    ;;

  spacetime)
    # stop/start address the APP tier, and spacetime has none — the client is
    # static files and the module host is the database, which stays up exactly
    # as postgres itself does for the other backends.
    if [ "$MODE" != "restart" ]; then echo "spacetime has no app server to $MODE"; exit 0; fi
    # A restart may kill only the exact listener captured after this run
    # started its dedicated host. URI, data directory and PID all come from the
    # token-authenticated lease; generated application config and ambient
    # "owned" booleans cannot select a target.
    "$NODE_BIN" "$LEASE_CLI_NODE" validate spacetime
    STDB_URI="$("$NODE_BIN" "$LEASE_CLI_NODE" field spacetime serverUri)"
    STDB_PORT="$("$NODE_BIN" "$LEASE_CLI_NODE" field spacetime serverPort)"
    STDB_DATA_DIR="$("$NODE_BIN" "$LEASE_CLI_NODE" field spacetime dataDir)"
    pid="$("$NODE_BIN" "$LEASE_CLI_NODE" listener-pid spacetime)"
    "$NODE_BIN" "$LEASE_CLI_NODE" mark-restarting spacetime
    echo "stopping the benchmark's SpacetimeDB host on :$STDB_PORT"
    # Kill the standalone host only; the commitlog on disk is what must carry
    # the scheduled rows across the restart.
    if command -v taskkill.exe >/dev/null 2>&1; then
      taskkill.exe /F /PID "$pid" /T >/dev/null 2>&1 || true
    elif command -v taskkill >/dev/null 2>&1; then
      taskkill //F //PID "$pid" //T >/dev/null 2>&1 || true
    else
      kill "$pid" 2>/dev/null || true
    fi
    sleep 4
    if curl -s -o /dev/null -m 3 "$STDB_URI/v1/ping"; then
      echo "SpacetimeDB still answers after stopping leased PID $pid" >&2
      exit 3
    fi
    # Deterministic crash-window coverage for fault-injection.mjs. This is
    # intentionally after the owned listener is gone and before its replacement
    # starts: teardown must be correct while the lease says `restarting` and the
    # port has no listener. Ordinary runs never set this variable.
    if [ "${STACK_BENCH_TEST_FAIL_AFTER_RESTART_STOP:-}" = "1" ]; then
      echo "injected failure after restart stop" >&2
      exit 86
    fi
    nohup "$SPACETIME_BIN" start --listen-addr "127.0.0.1:${STDB_PORT}" --data-dir "$STDB_DATA_DIR" < /dev/null > /tmp/restart-spacetime.log 2>&1 &
    wait_for "$STDB_URI/v1/ping" 240
    "$NODE_BIN" "$LEASE_CLI_NODE" capture-listener spacetime >/dev/null
    echo "SpacetimeDB host is back"
    ;;

  *)
    echo "unknown backend: $BACKEND" >&2
    exit 2
    ;;
esac
