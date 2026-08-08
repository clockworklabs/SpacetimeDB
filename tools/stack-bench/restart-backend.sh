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

case "$BACKEND" in
  postgres|mongodb)
    [ -n "$PORT" ] || { echo "express port required for $BACKEND" >&2; exit 2; }
    if [ "$MODE" != "start" ]; then
      echo "stopping Express on :$PORT"
      npx --yes kill-port "$PORT" >/dev/null 2>&1 || true
      sleep 3
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
