#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SERVER="${SPACETIME_SERVER:-local}"
A_CLIENTS="${A_CLIENTS:-4}"
B_CLIENTS="${B_CLIENTS:-4}"
CYCLE_CLIENTS="${CYCLE_CLIENTS:-0}"
ITERATIONS="${ITERATIONS:-25}"
BURN_ITERS="${BURN_ITERS:-0}"
RUN_ID="$(date +%Y%m%d%H%M%S)-$$"
DB_A="independent-repro-a-${RUN_ID}"
DB_B="independent-repro-b-${RUN_ID}"
DB_C="independent-repro-c-${RUN_ID}"
TMP_DIR="$(mktemp -d)"

cleanup() {
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
  local seq

  for ((seq = 1; seq <= ITERATIONS; seq++)); do
    if ! (
      cd "$SCRIPT_DIR" &&
      spacetime call --server "$SERVER" -- "$DB_A_ID" call_b_from_a \
        "$DB_B_ID" \
        "a-client-${client_id}" \
        "$seq" \
        "a-msg-client-${client_id}-seq-${seq}" \
        "$BURN_ITERS"
    ) >"$TMP_DIR/a-client-${client_id}-seq-${seq}.log" 2>&1; then
      failures=$((failures + 1))
    fi
  done

  printf '%s\n' "$failures" >"$TMP_DIR/a-client-${client_id}.failures"
}

run_b_client() {
  local client_id="$1"
  local failures=0
  local seq

  for ((seq = 1; seq <= ITERATIONS; seq++)); do
    if ! (
      cd "$SCRIPT_DIR" &&
      spacetime call --server "$SERVER" -- "$DB_B_ID" call_c_from_b \
        "$DB_C_ID" \
        "b-client-${client_id}" \
        "$seq" \
        "b-msg-client-${client_id}-seq-${seq}" \
        "$BURN_ITERS"
    ) >"$TMP_DIR/b-client-${client_id}-seq-${seq}.log" 2>&1; then
      failures=$((failures + 1))
    fi
  done

  printf '%s\n' "$failures" >"$TMP_DIR/b-client-${client_id}.failures"
}

run_cycle_client() {
  local client_id="$1"
  local failures=0
  local detected=0
  local seq

  for ((seq = 1; seq <= ITERATIONS; seq++)); do
    local output
    if output=$(
      cd "$SCRIPT_DIR" &&
      spacetime call --server "$SERVER" -- "$DB_A_ID" cycle_a_calls_b \
        "$DB_B_ID" \
        "$DB_A_ID" \
        "cycle-client-${client_id}" \
        "$seq" \
        "cycle-msg-client-${client_id}-seq-${seq}" \
        "$BURN_ITERS" 2>&1
    ); then
      : # success (should not happen -- this is a deadlock)
    else
      if echo "$output" | grep -q "cycle detected\|deadlock"; then
        detected=$((detected + 1))
      else
        failures=$((failures + 1))
      fi
    fi
  done

  printf '%s\n' "$failures" >"$TMP_DIR/cycle-client-${client_id}.failures"
  printf '%s\n' "$detected" >"$TMP_DIR/cycle-client-${client_id}.detected"
}

echo "Publishing independent-call repro module to A, B, and C on server '$SERVER'..."
DB_C_ID="$(publish_db "$DB_C")"
DB_B_ID="$(publish_db "$DB_B")"
DB_A_ID="$(publish_db "$DB_A")"

echo "A identity: $DB_A_ID"
echo "B identity: $DB_B_ID"
echo "C identity: $DB_C_ID"
echo "Starting $A_CLIENTS A-clients, $B_CLIENTS B-clients, and $CYCLE_CLIENTS cycle-clients with $ITERATIONS calls each..."

for ((client_id = 1; client_id <= A_CLIENTS; client_id++)); do
  run_a_client "$client_id" &
done
for ((client_id = 1; client_id <= B_CLIENTS; client_id++)); do
  run_b_client "$client_id" &
done
for ((client_id = 1; client_id <= CYCLE_CLIENTS; client_id++)); do
  run_cycle_client "$client_id" &
done
wait

A_FAILURES=0
for ((client_id = 1; client_id <= A_CLIENTS; client_id++)); do
  client_failures="$(cat "$TMP_DIR/a-client-${client_id}.failures")"
  A_FAILURES=$((A_FAILURES + client_failures))
done

B_FAILURES=0
for ((client_id = 1; client_id <= B_CLIENTS; client_id++)); do
  client_failures="$(cat "$TMP_DIR/b-client-${client_id}.failures")"
  B_FAILURES=$((B_FAILURES + client_failures))
done

CYCLE_FAILURES=0
CYCLE_DETECTED=0
for ((client_id = 1; client_id <= CYCLE_CLIENTS; client_id++)); do
  client_failures="$(cat "$TMP_DIR/cycle-client-${client_id}.failures")"
  client_detected="$(cat "$TMP_DIR/cycle-client-${client_id}.detected")"
  CYCLE_FAILURES=$((CYCLE_FAILURES + client_failures))
  CYCLE_DETECTED=$((CYCLE_DETECTED + client_detected))
done

A_SUCCESSES=$((A_CLIENTS * ITERATIONS - A_FAILURES))
B_SUCCESSES=$((B_CLIENTS * ITERATIONS - B_FAILURES))
CYCLE_TOTAL=$((CYCLE_CLIENTS * ITERATIONS))
CYCLE_OTHER=$((CYCLE_TOTAL - CYCLE_DETECTED - CYCLE_FAILURES))
TOTAL_FAILURES=$((A_FAILURES + B_FAILURES + CYCLE_FAILURES))

echo "Successful A->B calls: $A_SUCCESSES"
echo "Failed A->B calls: $A_FAILURES"
echo "Successful B->C calls: $B_SUCCESSES"
echo "Failed B->C calls: $B_FAILURES"
if [[ "$CYCLE_CLIENTS" -gt 0 ]]; then
  echo "Cycle calls (A->B->A): $CYCLE_TOTAL total"
  echo "  Deadlock detected: $CYCLE_DETECTED"
  echo "  Other failures: $CYCLE_FAILURES"
  echo "  Unexpectedly succeeded: $CYCLE_OTHER"
fi

if [[ "$A_SUCCESSES" -gt 0 ]]; then
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_A_ID" assert_kind_count sent_to_b "$A_SUCCESSES")
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_B_ID" assert_kind_count recv_from_a "$A_SUCCESSES")
fi

if [[ "$B_SUCCESSES" -gt 0 ]]; then
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_B_ID" assert_kind_count sent_to_c "$B_SUCCESSES")
  (cd "$SCRIPT_DIR" && spacetime call --server "$SERVER" -- "$DB_C_ID" assert_kind_count recv_from_b "$B_SUCCESSES")
fi

if [[ "$TOTAL_FAILURES" -ne 0 ]]; then
  echo
  echo "At least one client call failed. Sample failure logs:"
  find "$TMP_DIR" -name '*-client-*-seq-*.log' -type f -print0 \
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
