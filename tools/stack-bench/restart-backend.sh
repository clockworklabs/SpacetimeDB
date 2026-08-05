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
# Usage: restart-backend.sh <backend> <app-dir> [express-port] [probe-path]
#
# The probe path is whatever endpoint proves this application's server is
# answering again; it differs per track, so it is passed in rather than assumed.
set -euo pipefail

BACKEND="${1:?backend required}"
APP_DIR="${2:?app dir required}"
PORT="${3:-}"
PROBE="${4:-/api/rooms}"

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
    echo "stopping Express on :$PORT"
    npx --yes kill-port "$PORT" >/dev/null 2>&1 || true
    sleep 3
    ( cd "$APP_DIR/server" && PORT="$PORT" nohup npm run dev < /dev/null > "/tmp/restart-$BACKEND-$PORT.log" 2>&1 & )
    wait_for "http://localhost:$PORT$PROBE" 180
    echo "Express on :$PORT is back"
    ;;

  spacetime)
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
    echo "stopping SpacetimeDB host"
    # Kill the standalone host only; the commitlog on disk is what must carry
    # the scheduled rows across the restart.
    if command -v taskkill >/dev/null 2>&1; then
      taskkill //F //IM spacetimedb-standalone.exe >/dev/null 2>&1 || true
    else
      pkill -f spacetimedb-standalone || true
    fi
    sleep 4
    nohup spacetime start < /dev/null > /tmp/restart-spacetime.log 2>&1 &
    wait_for "http://localhost:3000/v1/ping" 240
    echo "SpacetimeDB host is back"
    ;;

  *)
    echo "unknown backend: $BACKEND" >&2
    exit 2
    ;;
esac
