#!/bin/bash
# Exhaust Test — Parallel Benchmark Launcher
#
# Runs multiple test instances in parallel for statistical significance.
# Each instance gets isolated ports via --run-index.
#
# Usage:
#   ./benchmark.sh                                    # 3 sequential-upgrade runs, both backends
#   ./benchmark.sh --runs 5                           # 5 runs
#   ./benchmark.sh --variant one-shot --runs 3        # 3 one-shot runs
#   ./benchmark.sh --backend spacetime --runs 3       # single backend only
#   ./benchmark.sh --rules standard --runs 3          # SDK-only rules
#   ./benchmark.sh --level 15                         # up to level 15 (22 features)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ─── Parse arguments ─────────────────────────────────────────────────────────

NUM_RUNS=3
VARIANT="sequential-upgrade"
RULES="guided"
LEVEL=""
BACKENDS=("spacetime" "postgres")
EXTRA_ARGS=""

while [[ $# -gt 0 ]]; do
  case $1 in
    --runs) NUM_RUNS="$2"; shift 2 ;;
    --variant) VARIANT="$2"; shift 2 ;;
    --rules) RULES="$2"; shift 2 ;;
    --level) LEVEL="$2"; shift 2 ;;
    --backend) BACKENDS=("$2"); shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ─── Compute total parallel instances ────────────────────────────────────────

NUM_BACKENDS=${#BACKENDS[@]}
TOTAL_INSTANCES=$((NUM_RUNS * NUM_BACKENDS))

echo "═══════════════════════════════════════════════════"
echo "  Exhaust Test Benchmark"
echo "═══════════════════════════════════════════════════"
echo "  Variant:   $VARIANT"
echo "  Rules:     $RULES"
echo "  Level:     ${LEVEL:-auto}"
echo "  Backends:  ${BACKENDS[*]}"
echo "  Runs:      $NUM_RUNS per backend"
echo "  Total:     $TOTAL_INSTANCES parallel instances"
echo ""
echo "  Port allocation:"
for i in $(seq 0 $((TOTAL_INSTANCES - 1))); do
  OFFSET=$((i * 100))
  echo "    Run $i: Vite(stdb)=$((5173 + OFFSET)) Vite(pg)=$((5174 + OFFSET)) Express=$((3001 + OFFSET)) PG=$((5433 + OFFSET))"
done
echo "═══════════════════════════════════════════════════"
echo ""

# ─── Validate prerequisites ─────────────────────────────────────────────────

# Check Claude CLI
if ! command -v claude &>/dev/null && ! command -v claude.exe &>/dev/null; then
  echo "ERROR: Claude Code CLI not found."
  exit 1
fi

echo "Starting $TOTAL_INSTANCES parallel instances..."
echo ""

# ─── Status tracking ────────────────────────────────────────────────────────

STATUS_FILE="$SCRIPT_DIR/benchmark-status.json"
echo '{}' > "$STATUS_FILE"

update_status() {
  local idx="$1" backend="$2" status="$3" detail="${4:-}"
  node -e "
    const fs = require('fs');
    const f = process.argv[1];
    const s = JSON.parse(fs.readFileSync(f, 'utf-8'));
    s['run-${idx}-${backend}'] = {
      runIndex: $idx,
      backend: '${backend}',
      status: '${status}',
      detail: '${detail}',
      updatedAt: new Date().toISOString()
    };
    fs.writeFileSync(f, JSON.stringify(s, null, 2));
  " -- "$STATUS_FILE" 2>/dev/null || true
}

# ─── Launch all runs ────────────────────────────────────────────────────────

PIDS=()
RUN_INDEX=0

for run_num in $(seq 1 "$NUM_RUNS"); do
  for backend in "${BACKENDS[@]}"; do
    LEVEL_ARGS=""
    if [[ -n "$LEVEL" ]]; then
      LEVEL_ARGS="--level $LEVEL"
    fi

    LOG_FILE="$SCRIPT_DIR/benchmark-run${RUN_INDEX}-${backend}.log"

    echo "[Run $RUN_INDEX] $backend (run $run_num/$NUM_RUNS) → $LOG_FILE"
    update_status "$RUN_INDEX" "$backend" "starting" "level=${LEVEL:-auto}"

    if [[ "$VARIANT" == "one-shot" ]]; then
      # One-shot: single generate call
      (
        update_status "$RUN_INDEX" "$backend" "running" "one-shot"
        "$SCRIPT_DIR/run.sh" \
          --variant "$VARIANT" \
          --rules "$RULES" \
          --backend "$backend" \
          --run-index "$RUN_INDEX" \
          $LEVEL_ARGS
        update_status "$RUN_INDEX" "$backend" "completed" "exit=$?"
      ) > "$LOG_FILE" 2>&1 &
      PIDS+=($!)
    else
      # Sequential-upgrade: launch a wrapper that does L1 generate then upgrades
      (
        set -e
        MAX_LEVEL=19  # all composed prompt levels

        if [[ -n "$LEVEL" ]]; then
          MAX_LEVEL="$LEVEL"
        fi

        # Generate level 1
        update_status "$RUN_INDEX" "$backend" "running" "level=1/$MAX_LEVEL"
        echo "[Run $RUN_INDEX/$backend] Generating level 1..."
        "$SCRIPT_DIR/run.sh" \
          --variant "$VARIANT" \
          --rules "$RULES" \
          --backend "$backend" \
          --run-index "$RUN_INDEX" \
          --level 1

        # Find the app directory that was just created
        APP_DIR=$(ls -dt "$SCRIPT_DIR/$VARIANT"/*"/results/$backend"/chat-app-* 2>/dev/null | head -1)

        if [[ -z "$APP_DIR" || ! -d "$APP_DIR" ]]; then
          echo "[Run $RUN_INDEX/$backend] ERROR: Could not find generated app directory"
          exit 1
        fi

        echo "[Run $RUN_INDEX/$backend] App dir: $APP_DIR"

        # Upgrade through remaining levels
        for level in $(seq 2 "$MAX_LEVEL"); do
          PROMPT_EXISTS=$(ls "$SCRIPT_DIR/../apps/chat-app/prompts/composed/$(printf '%02d' "$level")_"*.md 2>/dev/null | head -1)
          if [[ -z "$PROMPT_EXISTS" ]]; then
            echo "[Run $RUN_INDEX/$backend] No prompt for level $level — stopping"
            break
          fi
          update_status "$RUN_INDEX" "$backend" "running" "level=$level/$MAX_LEVEL"
          echo "[Run $RUN_INDEX/$backend] Upgrading to level $level..."
          "$SCRIPT_DIR/run.sh" \
            --variant "$VARIANT" \
            --rules "$RULES" \
            --backend "$backend" \
            --run-index "$RUN_INDEX" \
            --upgrade "$APP_DIR" \
            --level "$level" \
            --resume-session
        done

        update_status "$RUN_INDEX" "$backend" "completed" "level=$MAX_LEVEL"
        echo "[Run $RUN_INDEX/$backend] Sequential upgrade complete through level $MAX_LEVEL"
      ) > "$LOG_FILE" 2>&1 &
      PIDS+=($!)
    fi

    RUN_INDEX=$((RUN_INDEX + 1))
  done
done

echo ""
echo "All $TOTAL_INSTANCES instances launched. PIDs: ${PIDS[*]}"
echo ""
echo "Monitor progress:"
echo "  cat benchmark-status.json                # run status summary"
echo "  tail -f benchmark-run*-*.log              # live output"
echo ""
echo "Waiting for all runs to complete..."

# ─── Wait for all runs ──────────────────────────────────────────────────────

FAILURES=0
for i in "${!PIDS[@]}"; do
  if wait "${PIDS[$i]}"; then
    echo "[Run $i] completed successfully"
  else
    echo "[Run $i] FAILED (exit code $?)"
    FAILURES=$((FAILURES + 1))
  fi
done

echo ""
echo "═══════════════════════════════════════════════════"
echo "  Benchmark Complete"
echo "  Successful: $((TOTAL_INSTANCES - FAILURES))/$TOTAL_INSTANCES"
if [[ $FAILURES -gt 0 ]]; then
  echo "  Failed: $FAILURES"
fi
echo "═══════════════════════════════════════════════════"
echo ""
echo "Next steps:"
echo "  1. Grade each app: ./grade-playwright.sh <app-dir>"
echo "  2. Or grade all: for d in $VARIANT/*/results/*/chat-app-*; do ./grade-playwright.sh \"\$d\"; done"
echo "  3. Generate comparison report (TODO: generate-report.mjs)"
