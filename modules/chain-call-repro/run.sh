#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SERVER="${SPACETIME_SERVER:-local}"
A_CLIENTS="${A_CLIENTS:-4}"
B_CLIENTS="${B_CLIENTS:-4}"
ITERATIONS="${ITERATIONS:-25}"
BURN_ITERS="${BURN_ITERS:-0}"
HOLD_ITERS="${HOLD_ITERS:-25000000}"
RUN_ID="$(date +%Y%m%d%H%M%S)-$$"
DB_A="independent-repro-a-${RUN_ID}"
DB_B="independent-repro-b-${RUN_ID}"
DB_C="independent-repro-c-${RUN_ID}"
TMP_DIR="$(mktemp -d)"
PUBLISH_FIRST=1
RUN_FOREVER=0
RUN_A_CLIENTS=1
RUN_B_CLIENTS=1
DB_A_ID="${DB_A_ID:-}"
DB_B_ID="${DB_B_ID:-}"
DB_C_ID="${DB_C_ID:-}"

usage() {
  cat <<'EOF'
Usage: ./run.sh [options]

Options:
  --skip-publish     Reuse existing DB identities from DB_A_ID, DB_B_ID, and DB_C_ID.
  --forever          Run client calls forever instead of stopping after ITERATIONS.
  --only-a-client    Run only A clients.
  --only-b-client    Run only B clients.
  --help             Show this help.

Environment:
  SPACETIME_SERVER   Server name. Defaults to local.
  A_CLIENTS          Number of A clients. Defaults to 4.
  B_CLIENTS          Number of B clients. Defaults to 4.
  ITERATIONS         Calls per client when not using --forever. Defaults to 25.
  BURN_ITERS         Burn work per reducer call. Defaults to 0.
  HOLD_ITERS         Burn work after remote prepare succeeds. Defaults to 25000000.
  DB_A_ID            Required with --skip-publish.
  DB_B_ID            Required with --skip-publish.
  DB_C_ID            Required with --skip-publish.
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-publish)
      PUBLISH_FIRST=0
      ;;
    --forever)
      RUN_FOREVER=1
      ;;
    --only-a-client)
      RUN_B_CLIENTS=0
      ;;
    --only-b-client)
      RUN_A_CLIENTS=0
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      echo "unknown option: $1" >&2
      echo >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

if [[ "$RUN_A_CLIENTS" -eq 0 && "$RUN_B_CLIENTS" -eq 0 ]]; then
  echo "nothing to do: both A and B clients were disabled" >&2
  exit 1
fi

cleanup() {
  local pids

  pids="$(jobs -pr)" || true
  if [[ -n "$pids" ]]; then
    kill $pids 2>/dev/null || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

publish_db() {
  local db_name="$1"
  local output
  local identity

  output="$(cd "$SCRIPT_DIR" && spacetime publish --server "$SERVER" --clear-database -y "$db_name")"
  printf '%s\n' "$output" >&2

  identity="$(
    printf '%s\n' "$output" \
      | grep -Eo 'identity: [0-9a-fA-F]+' \
      | sed 's/^identity: //' \
      | tail -n1
  )"
  if [[ -z "$identity" ]]; then
    echo "failed to parse identity from publish output for $db_name" >&2
    return 1
  fi

  printf '%s\n' "$identity"
}

run_a_client() {
  local client_id="$1"
  local failures=0
  local log_file
  local seq

  log_file="$TMP_DIR/a-client-${client_id}.log"
  seq=1
  while :; do
    if ! (
      cd "$SCRIPT_DIR" &&
      spacetime call --server "$SERVER" -- "$DB_A_ID" call_b_from_a \
        "$DB_B_ID" \
        "a-client-${client_id}" \
        "$seq" \
        "a-msg-client-${client_id}-seq-${seq}" \
        "$BURN_ITERS" \
        "$HOLD_ITERS"
    ) >"$log_file" 2>&1; then
      failures=$((failures + 1))
    fi

    if [[ "$RUN_FOREVER" -eq 0 && "$seq" -ge "$ITERATIONS" ]]; then
      break
    fi
    seq=$((seq + 1))
  done

  printf '%s\n' "$failures" >"$TMP_DIR/a-client-${client_id}.failures"
}

run_b_client() {
  local client_id="$1"
  local failures=0
  local log_file
  local seq

  log_file="$TMP_DIR/b-client-${client_id}.log"
  seq=1
  while :; do
    if ! (
      cd "$SCRIPT_DIR" &&
      spacetime call --server "$SERVER" -- "$DB_B_ID" call_c_from_b \
        "$DB_C_ID" \
        "b-client-${client_id}" \
        "$seq" \
        "b-msg-client-${client_id}-seq-${seq}" \
        "$BURN_ITERS" \
        "$HOLD_ITERS"
    ) >"$log_file" 2>&1; then
      failures=$((failures + 1))
    fi

    if [[ "$RUN_FOREVER" -eq 0 && "$seq" -ge "$ITERATIONS" ]]; then
      break
    fi
    seq=$((seq + 1))
  done

  printf '%s\n' "$failures" >"$TMP_DIR/b-client-${client_id}.failures"
}

if [[ "$PUBLISH_FIRST" -eq 1 ]]; then
  echo "Publishing independent-call repro module to A, B, and C on server '$SERVER'..."
  DB_C_ID="$(publish_db "$DB_C")"
  DB_B_ID="$(publish_db "$DB_B")"
  DB_A_ID="$(publish_db "$DB_A")"
else
  if [[ -z "$DB_A_ID" || -z "$DB_B_ID" || -z "$DB_C_ID" ]]; then
    echo "DB_A_ID, DB_B_ID, and DB_C_ID are required with --skip-publish" >&2
    exit 1
  fi
fi

echo "A identity: $DB_A_ID"
echo "B identity: $DB_B_ID"
echo "C identity: $DB_C_ID"
echo "Client logs directory: $TMP_DIR"
if [[ "$RUN_A_CLIENTS" -eq 1 ]]; then
  echo "A client logs: $TMP_DIR/a-client-<id>.log"
fi
if [[ "$RUN_B_CLIENTS" -eq 1 ]]; then
  echo "B client logs: $TMP_DIR/b-client-<id>.log"
fi
if [[ "$RUN_FOREVER" -eq 1 ]]; then
  echo "Starting clients in forever mode..."
else
  echo "Starting clients with $ITERATIONS calls each..."
fi
echo "Prepare hold burn iters: $HOLD_ITERS"
echo "Workload note: run both A and B clients together to create contention on B and drive wound flow."
echo "A clients enabled: $RUN_A_CLIENTS ($A_CLIENTS configured)"
echo "B clients enabled: $RUN_B_CLIENTS ($B_CLIENTS configured)"

if [[ "$RUN_A_CLIENTS" -eq 1 ]]; then
  for ((client_id = 1; client_id <= A_CLIENTS; client_id++)); do
    run_a_client "$client_id" &
  done
fi
if [[ "$RUN_B_CLIENTS" -eq 1 ]]; then
  for ((client_id = 1; client_id <= B_CLIENTS; client_id++)); do
    run_b_client "$client_id" &
  done
fi
wait

if [[ "$RUN_FOREVER" -eq 1 ]]; then
  exit 0
fi

A_FAILURES=0
if [[ "$RUN_A_CLIENTS" -eq 1 ]]; then
  for ((client_id = 1; client_id <= A_CLIENTS; client_id++)); do
    client_failures="$(cat "$TMP_DIR/a-client-${client_id}.failures")"
    A_FAILURES=$((A_FAILURES + client_failures))
  done
fi

B_FAILURES=0
if [[ "$RUN_B_CLIENTS" -eq 1 ]]; then
  for ((client_id = 1; client_id <= B_CLIENTS; client_id++)); do
    client_failures="$(cat "$TMP_DIR/b-client-${client_id}.failures")"
    B_FAILURES=$((B_FAILURES + client_failures))
  done
fi

A_SUCCESSES=$((RUN_A_CLIENTS * A_CLIENTS * ITERATIONS - A_FAILURES))
B_SUCCESSES=$((RUN_B_CLIENTS * B_CLIENTS * ITERATIONS - B_FAILURES))
TOTAL_FAILURES=$((A_FAILURES + B_FAILURES))

echo "Successful A->B calls: $A_SUCCESSES"
echo "Failed A->B calls: $A_FAILURES"
echo "Successful B->C calls: $B_SUCCESSES"
echo "Failed B->C calls: $B_FAILURES"

if [[ "$RUN_A_CLIENTS" -eq 1 && "$A_SUCCESSES" -gt 0 ]]; then
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_A_ID" assert_kind_count sent_to_b "$A_SUCCESSES")
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_B_ID" assert_kind_count recv_from_a "$A_SUCCESSES")
fi

if [[ "$RUN_B_CLIENTS" -eq 1 && "$B_SUCCESSES" -gt 0 ]]; then
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_B_ID" assert_kind_count sent_to_c "$B_SUCCESSES")
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_C_ID" assert_kind_count recv_from_b "$B_SUCCESSES")
fi

if [[ "$TOTAL_FAILURES" -ne 0 ]]; then
  echo
  echo "At least one client call failed. Sample failure logs:"
  find "$TMP_DIR" -name '*-client-*.log' -type f -print0 \
    | xargs -0 grep -l "Error\|failed\|panic" \
    | head -n 10 \
    | while read -r log_file; do
        echo "--- $log_file ---"
        cat "$log_file"
      done
  exit 1
fi

echo
echo "Run complete."
echo "Flows exercised independently:"
echo "A reducer calls B"
echo "B reducer calls C"
echo "Use these database identities to inspect state manually if needed:"
echo "A: $DB_A_ID"
echo "B: $DB_B_ID"
echo "C: $DB_C_ID"
