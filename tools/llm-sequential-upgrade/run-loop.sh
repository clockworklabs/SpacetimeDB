#!/bin/bash
# Exhaust Loop — Full generate → grade → fix cycle for a single run.
#
# Drives one backend through the complete benchmark:
#   1. Generate (or upgrade) the app
#   2. Grade with Chrome MCP
#   3. If bugs: fix and re-grade (repeat until pass or max iterations)
#   4. For sequential: upgrade to next level, repeat from step 2
#
# Usage:
#   ./run-loop.sh --backend spacetime --level 7 --rules standard --run-index 0
#   ./run-loop.sh --backend postgres --variant one-shot --level 7 --run-index 1
#   ./run-loop.sh --backend spacetime --variant sequential-upgrade --level 12 --run-index 0
#
# Grading uses Chrome MCP (interactive Claude Code session).
# A lock file serializes grading across parallel runs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ─── Parse arguments ─────────────────────────────────────────────────────────

BACKEND="spacetime"
VARIANT="one-shot"
LEVEL=7
RULES="guided"
TEST_MODE=""
RUN_INDEX=0
MAX_FIX_ITERATIONS=5

while [[ $# -gt 0 ]]; do
  case $1 in
    --backend) BACKEND="$2"; shift 2 ;;
    --variant) VARIANT="$2"; shift 2 ;;
    --level) LEVEL="$2"; shift 2 ;;
    --rules) RULES="$2"; shift 2 ;;
    --test) TEST_MODE="$2"; shift 2 ;;
    --run-index) RUN_INDEX="$2"; shift 2 ;;
    --max-fixes) MAX_FIX_ITERATIONS="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

TEST_FLAG=""
if [[ -n "$TEST_MODE" ]]; then
  TEST_FLAG="--test $TEST_MODE"
fi

LOCK_FILE="$SCRIPT_DIR/.grade-lock"
LOG_PREFIX="[run-$RUN_INDEX/$BACKEND]"

echo "═══════════════════════════════════════════"
echo "$LOG_PREFIX Exhaust Loop"
echo "  Backend:   $BACKEND"
echo "  Variant:   $VARIANT"
echo "  Level:     $LEVEL"
echo "  Rules:     $RULES"
echo "  Run index: $RUN_INDEX"
echo "  Max fixes: $MAX_FIX_ITERATIONS"
echo "═══════════════════════════════════════════"

# ─── Helper: acquire grading lock ────────────────────────────────────────────
# Only one grading session at a time (Chrome MCP limitation).

acquire_grade_lock() {
  echo "$LOG_PREFIX Waiting for grading lock..."
  while ! mkdir "$LOCK_FILE" 2>/dev/null; do
    sleep 5
  done
  echo "$LOG_PREFIX Grading lock acquired"
}

release_grade_lock() {
  rmdir "$LOCK_FILE" 2>/dev/null || true
}

# Clean up lock on exit
trap 'release_grade_lock' EXIT

# ─── Helper: grade the app ──────────────────────────────────────────────────

grade_app() {
  local app_dir="$1"
  local grade_level="$2"

  acquire_grade_lock

  echo "$LOG_PREFIX Grading at level $grade_level..."
  "$SCRIPT_DIR/grade.sh" "$app_dir" 2>&1 | tee "$app_dir/grade-output-level${grade_level}.log"

  release_grade_lock

  # Check if bugs were found
  if [[ -f "$app_dir/BUG_REPORT.md" ]]; then
    echo "$LOG_PREFIX Bugs found — fix iteration needed"
    return 1
  else
    echo "$LOG_PREFIX All features passed at level $grade_level"
    return 0
  fi
}

# ─── Helper: fix bugs ───────────────────────────────────────────────────────

fix_bugs() {
  local app_dir="$1"
  local iteration="$2"

  echo "$LOG_PREFIX Fix iteration $iteration..."
  "$SCRIPT_DIR/run.sh" \
    --fix "$app_dir" \
    --variant "$VARIANT" \
    --rules "$RULES" \
    $TEST_FLAG \
    --run-index "$RUN_INDEX" \
    --level "$LEVEL" \
    --resume-session \
    2>&1 | tee "$app_dir/fix-output-iter${iteration}.log"
}

# ─── ONE-SHOT FLOW ──────────────────────────────────────────────────────────

if [[ "$VARIANT" == "one-shot" ]]; then
  echo "$LOG_PREFIX === One-Shot: Generating all features ==="

  # Step 1: Generate
  "$SCRIPT_DIR/run.sh" \
    --variant "$VARIANT" \
    --rules "$RULES" \
    $TEST_FLAG \
    --backend "$BACKEND" \
    --run-index "$RUN_INDEX" \
    --level "$LEVEL"

  # Find the app directory
  APP_DIR=$(ls -dt "$SCRIPT_DIR/$VARIANT"/*"/$BACKEND/results"/chat-app-* 2>/dev/null | head -1)
  if [[ -z "$APP_DIR" || ! -d "$APP_DIR" ]]; then
    echo "$LOG_PREFIX ERROR: Could not find generated app directory"
    exit 1
  fi
  echo "$LOG_PREFIX App dir: $APP_DIR"

  # Step 2: Grade → Fix loop
  ITERATION=0
  while true; do
    if grade_app "$APP_DIR" "$LEVEL"; then
      echo "$LOG_PREFIX === One-Shot Complete: All features pass ==="
      break
    fi

    ITERATION=$((ITERATION + 1))
    if [[ $ITERATION -ge $MAX_FIX_ITERATIONS ]]; then
      echo "$LOG_PREFIX === Max fix iterations ($MAX_FIX_ITERATIONS) reached ==="
      break
    fi

    fix_bugs "$APP_DIR" "$ITERATION"
  done

# ─── SEQUENTIAL-UPGRADE FLOW ────────────────────────────────────────────────

else
  echo "$LOG_PREFIX === Sequential Upgrade: Levels 1 → $LEVEL ==="

  # Step 1: Generate level 1
  echo "$LOG_PREFIX Generating level 1..."
  "$SCRIPT_DIR/run.sh" \
    --variant "$VARIANT" \
    --rules "$RULES" \
    --backend "$BACKEND" \
    --run-index "$RUN_INDEX" \
    --level 1

  APP_DIR=$(ls -dt "$SCRIPT_DIR/$VARIANT"/*"/$BACKEND/results"/chat-app-* 2>/dev/null | head -1)
  if [[ -z "$APP_DIR" || ! -d "$APP_DIR" ]]; then
    echo "$LOG_PREFIX ERROR: Could not find generated app directory"
    exit 1
  fi
  echo "$LOG_PREFIX App dir: $APP_DIR"

  # Grade level 1
  ITERATION=0
  while ! grade_app "$APP_DIR" 1; do
    ITERATION=$((ITERATION + 1))
    if [[ $ITERATION -ge $MAX_FIX_ITERATIONS ]]; then
      echo "$LOG_PREFIX Max fixes at level 1 — moving on"
      break
    fi
    fix_bugs "$APP_DIR" "$ITERATION"
  done

  # Step 2: Upgrade through remaining levels
  for current_level in $(seq 2 "$LEVEL"); do
    PROMPT_EXISTS=$(ls "$SCRIPT_DIR/../llm-oneshot/apps/chat-app/prompts/composed/$(printf '%02d' "$current_level")_"*.md 2>/dev/null | head -1)
    if [[ -z "$PROMPT_EXISTS" ]]; then
      echo "$LOG_PREFIX No prompt for level $current_level — stopping"
      break
    fi

    echo "$LOG_PREFIX === Upgrading to level $current_level ==="
    "$SCRIPT_DIR/run.sh" \
      --variant "$VARIANT" \
      --rules "$RULES" \
      $TEST_FLAG \
      --backend "$BACKEND" \
      --run-index "$RUN_INDEX" \
      --upgrade "$APP_DIR" \
      --level "$current_level" \
      --resume-session

    # Grade ALL features (regression test)
    ITERATION=0
    while ! grade_app "$APP_DIR" "$current_level"; do
      ITERATION=$((ITERATION + 1))
      if [[ $ITERATION -ge $MAX_FIX_ITERATIONS ]]; then
        echo "$LOG_PREFIX Max fixes at level $current_level — moving on"
        break
      fi
      fix_bugs "$APP_DIR" "$ITERATION"
    done
  done

  echo "$LOG_PREFIX === Sequential Upgrade Complete ==="
fi

# ─── Summary ────────────────────────────────────────────────────────────────

echo ""
echo "═══════════════════════════════════════════"
echo "$LOG_PREFIX Exhaust Loop Complete"
echo "  App dir: $APP_DIR"
echo "  Variant: $VARIANT"
echo "  Backend: $BACKEND"
echo "═══════════════════════════════════════════"
echo ""
echo "When done grading, clean up with: ./cleanup.sh $APP_DIR"
