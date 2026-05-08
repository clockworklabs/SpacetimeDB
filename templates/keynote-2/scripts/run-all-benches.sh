#!/usr/bin/env bash
# Usage: run-all-benches.sh [RUNS] [SECONDS] [CONNECTORS_CSV] [ALPHAS_CSV] [OUT_NAME]
#
# Defaults:
#   RUNS=5  SECONDS=60  CONNECTORS=all  ALPHAS=0,1.5
#   OUT_NAME=auto-generated timestamp + tags
#
# Output goes to /tmp/<OUT_NAME>.tsv  (or /tmp/bench-<timestamp>-<tags>.tsv if not specified)
# A symlink /tmp/bench-results.tsv points at the most recent run.

set -uo pipefail
cd ~/SpacetimeDB/templates/keynote-2

RUNS=${1:-5}
SECS=${2:-60}
CONNECTORS_CSV=${3:-sqlite_rpc,postgres_rpc,cockroach_rpc,bun,supabase_rpc,convex}
ALPHAS_CSV=${4:-0,1.5}
OUT_NAME=${5:-}

if [ -z "$OUT_NAME" ]; then
  TS=$(date +%Y%m%dT%H%M%S)
  CONN_TAG=$(echo "$CONNECTORS_CSV" | tr ',' '-')
  ALPHA_TAG=$(echo "$ALPHAS_CSV" | tr ',' '-')
  OUT_NAME="bench-${TS}-${CONN_TAG}-a${ALPHA_TAG}-${RUNS}x${SECS}s"
fi

OUT="/tmp/${OUT_NAME}.tsv"
LOG="/tmp/${OUT_NAME}.log"
LATEST_LINK="/tmp/bench-results.tsv"
: > "$LOG"

CONVEX_URL=$(grep '^CONVEX_URL=' convex-app/.env.local 2>/dev/null | cut -d= -f2)
[ -z "$CONVEX_URL" ] && CONVEX_URL=http://127.0.0.1:3210

echo "config: runs=$RUNS seconds=$SECS connectors=$CONNECTORS_CSV alphas=$ALPHAS_CSV convex=$CONVEX_URL" | tee -a "$LOG"
echo "out: $OUT" | tee -a "$LOG"

printf 'connector\talpha\trun\ttps\tsamples\tp50_ms\tp95_ms\tp99_ms\tcollision_rate\tverify_ok\tverify_total\tverify_changed\n' > "$OUT"

verify_convex() {
  local count=0
  local changed=0
  local total_seen=0
  for id in $(seq 0 63); do
    local r=$(curl -s --max-time 5 -X POST "$CONVEX_URL/api/query" \
      -H 'content-type: application/json' \
      -d "{\"path\":\"accounts:get_account\",\"args\":{\"id\":$id}}")
    local bal=$(echo "$r" | jq -r '.value.balance // empty' 2>/dev/null)
    if [ -n "$bal" ]; then
      bal=${bal%.*}
      count=$((count+1))
      total_seen=$((total_seen+bal))
      [ "$bal" != "10000000" ] && changed=$((changed+1))
    fi
  done
  printf '{"ok":"success","result":{"accounts":"%d","total":"sampled_64=%d","changed":"%d"}}' \
    "$count" "$total_seen" "$changed"
}

verify() {
  local c=$1
  case "$c" in
    sqlite_rpc)    curl -s --max-time 30 -X POST http://127.0.0.1:4103/rpc -H 'content-type: application/json' -d '{"name":"verify","args":{}}' ;;
    postgres_rpc)  curl -s --max-time 30 -X POST http://127.0.0.1:4101/rpc -H 'content-type: application/json' -d '{"name":"verify","args":{}}' ;;
    cockroach_rpc) curl -s --max-time 30 -X POST http://127.0.0.1:4102/rpc -H 'content-type: application/json' -d '{"name":"verify","args":{}}' ;;
    bun)           curl -s --max-time 30 -X POST http://127.0.0.1:4001/rpc -H 'content-type: application/json' -d '{"name":"verify","args":{}}' ;;
    supabase_rpc)  curl -s --max-time 30 -X POST http://127.0.0.1:4106/rpc -H 'content-type: application/json' -d '{"name":"verify","args":{}}' ;;
    convex)        verify_convex ;;
  esac
}

IFS=',' read -ra CONNECTORS <<< "$CONNECTORS_CSV"
IFS=',' read -ra ALPHAS <<< "$ALPHAS_CSV"

for c in "${CONNECTORS[@]}"; do
  for a in "${ALPHAS[@]}"; do
    for ((i=1; i<=RUNS; i++)); do
      ts=$(date '+%H:%M:%S')
      echo "[$ts] $c alpha=$a run $i/$RUNS" | tee -a "$LOG"

      pnpm run bench test-1 --seconds "$SECS" --concurrency 50 --alpha "$a" --connectors "$c" \
        >> "$LOG" 2>&1

      latest=$(ls -t runs/test-1-*.json 2>/dev/null | head -1)

      read tps samples p50 p95 p99 crate < <(
        jq -r '.results[0].res | "\(.tps) \(.samples) \(.p50_ms) \(.p95_ms) \(.p99_ms) \(.collision_rate)"' "$latest" 2>/dev/null \
        || echo "NA NA NA NA NA NA"
      )

      vraw=$(verify "$c")
      vok=$(echo "$vraw"      | jq -r '.ok // .status // "?"' 2>/dev/null)
      vtotal=$(echo "$vraw"   | jq -r '.result.total // .value.total // "?"' 2>/dev/null)
      vchanged=$(echo "$vraw" | jq -r '.result.changed // .value.changed // "?"' 2>/dev/null)

      printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
        "$c" "$a" "$i" "$tps" "$samples" "$p50" "$p95" "$p99" "$crate" "$vok" "$vtotal" "$vchanged" \
        | tee -a "$OUT"
    done
  done
done

ln -sfn "$OUT" "$LATEST_LINK"

echo
echo "=== DONE ==="
echo "Results: $OUT"
echo "Log:     $LOG"
echo "Latest symlink: $LATEST_LINK -> $OUT"
