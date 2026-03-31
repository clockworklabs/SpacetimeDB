#!/bin/bash
# Exhaust Test Launcher
#
# Runs the benchmark in Claude Code with OpenTelemetry enabled
# for exact per-request token tracking.
#
# Usage:
#   ./run.sh                          # defaults: level=1, backend=spacetime
#   ./run.sh --level 5 --backend postgres
#   ./run.sh --level 12 --backend spacetime
#
# Prerequisites:
#   - Claude Code CLI installed (claude or npx @anthropic-ai/claude-code)
#   - Docker running (for OTel Collector)
#   - SpacetimeDB running (spacetime start)
#   - Chrome open with Claude MCP extension

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TELEMETRY_DIR="$SCRIPT_DIR/telemetry"
RESULTS_DIR="$SCRIPT_DIR/results"

# ─── Parse arguments ─────────────────────────────────────────────────────────

LEVEL=1
BACKEND="spacetime"
while [[ $# -gt 0 ]]; do
  case $1 in
    --level) LEVEL="$2"; shift 2 ;;
    --backend) BACKEND="$2"; shift 2 ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# ─── Find Claude CLI ─────────────────────────────────────────────────────────

CLAUDE_CMD=""
if command -v claude &>/dev/null; then
  CLAUDE_CMD="claude"
elif command -v claude.exe &>/dev/null; then
  CLAUDE_CMD="claude.exe"
else
  # Try npx as fallback
  if npx @anthropic-ai/claude-code --version &>/dev/null; then
    CLAUDE_CMD="npx @anthropic-ai/claude-code"
  else
    echo "ERROR: Claude Code CLI not found."
    echo "Install it with: npm install -g @anthropic-ai/claude-code"
    echo "Or ensure 'claude' is on your PATH."
    exit 1
  fi
fi
echo "Using Claude CLI: $CLAUDE_CMD"

# ─── Pre-flight checks ──────────────────────────────────────────────────────

echo ""
echo "=== Pre-flight Checks ==="

# Check SpacetimeDB
if spacetime server ping local &>/dev/null; then
  echo "[OK] SpacetimeDB is running"
else
  echo "[FAIL] SpacetimeDB is not running. Start it with: spacetime start"
  exit 1
fi

# Check Docker & OTel Collector
if ! docker info &>/dev/null; then
  echo "[FAIL] Docker is not running."
  exit 1
fi

# Start OTel Collector if not running
if docker compose -f "$SCRIPT_DIR/docker-compose.otel.yaml" ps --status running 2>/dev/null | grep -q otel-collector; then
  echo "[OK] OTel Collector is running"
else
  echo "[...] Starting OTel Collector..."
  docker compose -f "$SCRIPT_DIR/docker-compose.otel.yaml" up -d
  echo "[OK] OTel Collector started"
fi

# Check Node.js
if command -v node &>/dev/null; then
  echo "[OK] Node.js $(node --version)"
else
  echo "[FAIL] Node.js not found."
  exit 1
fi

# Check prompt files exist
COMPOSED_PROMPT="$SCRIPT_DIR/../apps/chat-app/prompts/composed/$(printf '%02d' "$LEVEL")_"*".md"
# shellcheck disable=SC2086
if ls $COMPOSED_PROMPT &>/dev/null; then
  PROMPT_FILE=$(ls $COMPOSED_PROMPT 2>/dev/null | head -1)
  echo "[OK] Prompt file: $(basename "$PROMPT_FILE")"
else
  echo "[FAIL] No composed prompt found for level $LEVEL"
  exit 1
fi

echo ""

# ─── Create run directories ─────────────────────────────────────────────────

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
START_TIME=$(date +%Y-%m-%dT%H:%M:%S%z)
RUN_ID="$BACKEND-level$LEVEL-$TIMESTAMP"
RUN_DIR="$TELEMETRY_DIR/$RUN_ID"
APP_DIR="$RESULTS_DIR/$BACKEND/chat-app-$TIMESTAMP"
mkdir -p "$RUN_DIR"
mkdir -p "$APP_DIR"
mkdir -p "$TELEMETRY_DIR"

# On Windows (Git Bash/MSYS2), convert paths to native format for Node.js
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
  RUN_DIR_NATIVE=$(cygpath -w "$RUN_DIR")
  APP_DIR_NATIVE=$(cygpath -w "$APP_DIR")
  SCRIPT_DIR_NATIVE=$(cygpath -w "$SCRIPT_DIR")
else
  RUN_DIR_NATIVE="$RUN_DIR"
  APP_DIR_NATIVE="$APP_DIR"
  SCRIPT_DIR_NATIVE="$SCRIPT_DIR"
fi

echo "=== Exhaust Test ==="
echo "  Level:     $LEVEL"
echo "  Backend:   $BACKEND"
echo "  Run ID:    $RUN_ID"
echo "  App dir:   $APP_DIR"
echo "  Telemetry: $RUN_DIR"
echo ""

# ─── Enable OpenTelemetry ────────────────────────────────────────────────────

export CLAUDE_CODE_ENABLE_TELEMETRY=1
export OTEL_LOGS_EXPORTER=otlp
export OTEL_METRICS_EXPORTER=otlp
export OTEL_EXPORTER_OTLP_PROTOCOL=grpc
export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
export OTEL_LOGS_EXPORT_INTERVAL=1000
export OTEL_METRIC_EXPORT_INTERVAL=5000

# ─── Save run metadata ──────────────────────────────────────────────────────

cat > "$RUN_DIR/metadata.json" <<EOF
{
  "level": $LEVEL,
  "backend": "$BACKEND",
  "timestamp": "$TIMESTAMP",
  "startedAt": "$START_TIME",
  "runId": "$RUN_ID",
  "appDir": "$APP_DIR",
  "promptFile": "$(basename "$PROMPT_FILE")"
}
EOF

# ─── Build the prompt ────────────────────────────────────────────────────────

# The prompt tells Claude Code exactly what to do.
# Claude Code will auto-read CLAUDE.md from the CWD for additional context.
PROMPT=$(cat <<PROMPT_EOF
Run the exhaust test benchmark.

**Configuration:**
- Level: $LEVEL
- Backend: $BACKEND
- App output directory: $APP_DIR
- Run ID: $RUN_ID

**Instructions:**
Follow the CLAUDE.md workflow (Phases 0 through 8). Key points:
1. Read the SDK rules and prompt files (Phase 0)
2. Generate, build, and deploy the app (Phases 1-5)
3. Test every feature via Chrome MCP browser interaction (Phase 6)
4. Fix bugs → redeploy → retest loop (Phase 7)
5. Write GRADING_RESULTS.md in the app directory (Phase 8)
6. Write ITERATION_LOG.md in the app directory after each fix iteration

The app directory is: $APP_DIR_NATIVE
Write all generated code there (backend/ and client/ subdirectories).
Write GRADING_RESULTS.md and ITERATION_LOG.md there when done.

Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.
PROMPT_EOF
)

echo "Starting Claude Code session..."
echo "─────────────────────────────────────────────"

# ─── Run Claude Code ─────────────────────────────────────────────────────────

cd "$SCRIPT_DIR"
$CLAUDE_CMD --print --verbose --output-format text --max-turns 200 --dangerously-skip-permissions -p "$PROMPT"
EXIT_CODE=$?

echo ""
echo "─────────────────────────────────────────────"

# ─── Record end time ─────────────────────────────────────────────────────────

END_TIME=$(date +%Y-%m-%dT%H:%M:%S%z)

# Update metadata with end time
node -e "
const fs = require('fs');
const f = '$RUN_DIR_NATIVE/metadata.json'.replace(/\\\\/g, '/');
const m = JSON.parse(fs.readFileSync(f, 'utf-8'));
m.endedAt = '$END_TIME';
m.exitCode = $EXIT_CODE;
fs.writeFileSync(f, JSON.stringify(m, null, 2));
"

# ─── Parse telemetry ─────────────────────────────────────────────────────────

echo ""
echo "=== Session Complete ==="
echo "  Started: $START_TIME"
echo "  Ended:   $END_TIME"
echo ""

echo "Parsing telemetry..."
if node "$SCRIPT_DIR_NATIVE/parse-telemetry.mjs" "$RUN_DIR_NATIVE"; then
  echo ""
  echo "=== Results ==="
  echo "  App:        $APP_DIR"
  echo "  Grading:    $APP_DIR/GRADING_RESULTS.md"
  echo "  Iterations: $APP_DIR/ITERATION_LOG.md"
  echo "  Cost:       $RUN_DIR/COST_REPORT.md"
  echo "  Telemetry:  $RUN_DIR/"
else
  echo "WARNING: Telemetry parsing failed. Raw logs at: $TELEMETRY_DIR/logs.jsonl"
fi
