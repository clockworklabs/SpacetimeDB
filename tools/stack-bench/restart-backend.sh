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
    NAME="stack-bench-$(basename "$(dirname "$APP_DIR")")"
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
    # The SpacetimeDB host is a shared machine-wide service — other projects'
    # databases live on it. Killing it to test one benchmark app takes them all
    # down, so this refuses unless it is pointed at an instance the benchmark
    # owns. Set STACK_BENCH_STDB_OWNED=1 only for a dedicated host started with
    # its own --data-dir and --listen-addr.
    if [ "${STACK_BENCH_STDB_OWNED:-0}" != "1" ]; then
      echo "refusing to restart a shared SpacetimeDB host." >&2
      echo "Start a benchmark-owned instance and set STACK_BENCH_STDB_OWNED=1." >&2
      exit 3
    fi
    STDB_URI="${STACK_BENCH_STDB_URI:-http://127.0.0.1:3210}"
    STDB_PORT="${STDB_URI##*:}"
    STDB_DATA_DIR="$(cd "$(dirname "$0")" && pwd)/.spacetime-data"
    echo "stopping the benchmark's SpacetimeDB host on :$STDB_PORT"
    # Kill the standalone host only; the commitlog on disk is what must carry
    # the scheduled rows across the restart.
    if command -v taskkill >/dev/null 2>&1; then
      pid=$(netstat -ano | grep -E ":${STDB_PORT}[[:space:]]" | grep -i LISTENING | awk '{print $NF}' | head -1)
      [ -n "$pid" ] && taskkill //F //PID "$pid" //T >/dev/null 2>&1 || true
    else
      pkill -f "listen-addr 127.0.0.1:${STDB_PORT}" || true
    fi
    sleep 4
    nohup "$SPACETIME_BIN" start --listen-addr "127.0.0.1:${STDB_PORT}" --data-dir "$STDB_DATA_DIR" < /dev/null > /tmp/restart-spacetime.log 2>&1 &
    wait_for "${STACK_BENCH_STDB_URI:-http://127.0.0.1:3210}/v1/ping" 240
    echo "SpacetimeDB host is back"
    ;;

  *)
    echo "unknown backend: $BACKEND" >&2
    exit 2
    ;;
esac
