#!/bin/bash
# Sequential Upgrade — Parallel Benchmark Launcher
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
TEST_MODE=""
LEVEL=""
BACKENDS=("spacetime" "postgres")

while [[ $# -gt 0 ]]; do
  case $1 in
    --runs) NUM_RUNS="$2"; shift 2 ;;
    --variant) VARIANT="$2"; shift 2 ;;
    --rules) RULES="$2"; shift 2 ;;
    --test) TEST_MODE="$2"; shift 2 ;;
    --level) LEVEL="$2"; shift 2 ;;
    --backend) BACKENDS=("$2"); shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

TEST_FLAG=""
if [[ -n "$TEST_MODE" ]]; then
  TEST_FLAG="--test $TEST_MODE"
fi

# ─── Compute total parallel instances ────────────────────────────────────────

NUM_BACKENDS=${#BACKENDS[@]}
TOTAL_INSTANCES=$((NUM_RUNS * NUM_BACKENDS))

echo "═══════════════════════════════════════════════════"
echo "  Sequential Upgrade Benchmark"
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

# Add Claude Code desktop install to PATH
_APPDATA_UNIX="${APPDATA:-$HOME/AppData/Roaming}"
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
  _APPDATA_UNIX=$(cygpath "$_APPDATA_UNIX" 2>/dev/null || echo "$_APPDATA_UNIX")
fi
CLAUDE_DESKTOP_DIR="$_APPDATA_UNIX/Claude/claude-code"
if [[ -d "$CLAUDE_DESKTOP_DIR" ]]; then
  CLAUDE_LATEST=$(ls -d "$CLAUDE_DESKTOP_DIR"/*/ 2>/dev/null | sort -V | tail -1)
  if [[ -n "$CLAUDE_LATEST" ]]; then
    export PATH="$PATH:$CLAUDE_LATEST"
  fi
fi

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
# Each run gets its own run-loop.sh which handles:
#   - Code generation (parallel, headless)
#   - Chrome MCP grading (serialized via lock file)
#   - Bug fix iterations (headless)
#   - Sequential upgrades with regression testing (if applicable)

PIDS=()
RUN_INDEX=0

for run_num in $(seq 1 "$NUM_RUNS"); do
  for backend in "${BACKENDS[@]}"; do
    LOG_FILE="$SCRIPT_DIR/benchmark-run${RUN_INDEX}-${backend}.log"

    echo "[Run $RUN_INDEX] $backend (run $run_num/$NUM_RUNS) → $LOG_FILE"
    update_status "$RUN_INDEX" "$backend" "starting" "level=${LEVEL:-auto}"

    (
      update_status "$RUN_INDEX" "$backend" "running" "$VARIANT"
      "$SCRIPT_DIR/run-loop.sh" \
        --backend "$backend" \
        --variant "$VARIANT" \
        --level "${LEVEL:-7}" \
        --rules "$RULES" \
        $TEST_FLAG \
        --run-index "$RUN_INDEX"
      update_status "$RUN_INDEX" "$backend" "completed" "exit=$?"
    ) > "$LOG_FILE" 2>&1 &
    PIDS+=($!)

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
# ─── Auto-generate reports ──────────────────────────────────────────────────

echo ""
echo "Generating reports for each run..."
for run_dir in "$SCRIPT_DIR/$VARIANT"/*/; do
  if [[ -d "$run_dir/telemetry" ]]; then
    RUN_DIR_NATIVE="$run_dir"
    if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
      RUN_DIR_NATIVE=$(cygpath -w "$run_dir")
    fi
    node "$SCRIPT_DIR/generate-report.mjs" "$RUN_DIR_NATIVE" 2>/dev/null && \
      echo "  Report: $run_dir/BENCHMARK_REPORT.md" || \
      echo "  WARNING: Report generation failed for $run_dir"
  fi
done

echo ""
echo "═══════════════════════════════════════════════════"
echo "  All Done"
echo "═══════════════════════════════════════════════════"
echo ""
echo "Results:"
for run_dir in "$SCRIPT_DIR/$VARIANT"/*/; do
  if [[ -f "$run_dir/BENCHMARK_REPORT.md" ]]; then
    echo "  $run_dir/BENCHMARK_REPORT.md"
  fi
done
