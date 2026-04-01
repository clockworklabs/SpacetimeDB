#!/bin/bash -l
# Exhaust Test Launcher — Phase 1: Generate & Deploy
#
# Runs code generation and deployment in headless Claude Code with OTel tracking.
# After this completes, run grade.sh to do browser testing and grading interactively.
#
# Usage:
#   ./run.sh                          # defaults: level=1, backend=spacetime
#   ./run.sh --level 5 --backend postgres
#
# Prerequisites:
#   - Claude Code CLI installed (claude or npx @anthropic-ai/claude-code)
#   - Docker running (for OTel Collector)
#   - SpacetimeDB running (spacetime start)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TELEMETRY_DIR="$SCRIPT_DIR/telemetry"
RESULTS_DIR="$SCRIPT_DIR/results"

# ─── Parse arguments ─────────────────────────────────────────────────────────

LEVEL=1
BACKEND="spacetime"
FIX_MODE=""
FIX_APP_DIR=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --level) LEVEL="$2"; shift 2 ;;
    --backend) BACKEND="$2"; shift 2 ;;
    --fix) FIX_MODE=1; FIX_APP_DIR="$2"; shift 2 ;;
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
  if npx @anthropic-ai/claude-code --version &>/dev/null; then
    CLAUDE_CMD="npx @anthropic-ai/claude-code"
  else
    echo "ERROR: Claude Code CLI not found."
    echo "Install it with: npm install -g @anthropic-ai/claude-code"
    exit 1
  fi
fi
echo "Using Claude CLI: $CLAUDE_CMD"

# ─── Pre-flight checks ──────────────────────────────────────────────────────

echo ""
echo "=== Pre-flight Checks ==="

# Ensure spacetime is in PATH (Windows installs to AppData/Local/SpacetimeDB)
SPACETIME_DIR="${USERPROFILE:-$HOME}/AppData/Local/SpacetimeDB"
if [[ -d "$SPACETIME_DIR" ]]; then
  export PATH="$PATH:$SPACETIME_DIR"
fi
# Also try the cygpath-resolved home
_USER="${USER:-${USERNAME:-$(whoami)}}"
if [[ -d "/c/Users/$_USER/AppData/Local/SpacetimeDB" ]]; then
  export PATH="$PATH:/c/Users/$_USER/AppData/Local/SpacetimeDB"
fi

if [[ "$BACKEND" == "spacetime" ]]; then
  if spacetime server ping local &>/dev/null; then
    echo "[OK] SpacetimeDB is running"
  else
    echo "[FAIL] SpacetimeDB is not running. Start it with: spacetime start"
    exit 1
  fi
elif [[ "$BACKEND" == "postgres" ]]; then
  if docker exec spacetime-web-postgres-1 psql -U spacetime -d spacetime -c "SELECT 1" &>/dev/null; then
    echo "[OK] PostgreSQL is running (port 5433)"
  else
    echo "[FAIL] PostgreSQL is not reachable. Check Docker container spacetime-web-postgres-1."
    exit 1
  fi
fi

if ! docker info &>/dev/null; then
  echo "[FAIL] Docker is not running."
  exit 1
fi

# Rotate telemetry log if over 10MB to prevent unbounded growth
LOGS_FILE="$TELEMETRY_DIR/logs.jsonl"
if [[ -f "$LOGS_FILE" ]]; then
  SIZE=$(wc -c < "$LOGS_FILE")
  if [[ $SIZE -gt 10485760 ]]; then
    ARCHIVE="$TELEMETRY_DIR/logs-$(date +%Y%m%d-%H%M%S).jsonl.bak"
    mv "$LOGS_FILE" "$ARCHIVE"
    echo "[INFO] Rotated logs.jsonl ($SIZE bytes) to $(basename "$ARCHIVE")"
  fi
fi

if docker compose -f "$SCRIPT_DIR/docker-compose.otel.yaml" ps --status running 2>/dev/null | grep -q otel-collector; then
  echo "[OK] OTel Collector is running"
else
  echo "[...] Starting OTel Collector..."
  docker compose -f "$SCRIPT_DIR/docker-compose.otel.yaml" up -d
  echo "[OK] OTel Collector started"
fi

if command -v node &>/dev/null; then
  echo "[OK] Node.js $(node --version)"
else
  echo "[FAIL] Node.js not found."
  exit 1
fi

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
START_TIME_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)
RUN_ID="$BACKEND-level$LEVEL-$TIMESTAMP"
RUN_DIR="$TELEMETRY_DIR/$RUN_ID"
APP_DIR="$RESULTS_DIR/$BACKEND/chat-app-$TIMESTAMP"
mkdir -p "$RUN_DIR"
mkdir -p "$APP_DIR"

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

echo "=== Exhaust Test: Generate & Deploy ==="
echo "  Level:     $LEVEL"
echo "  Backend:   $BACKEND"
echo "  Run ID:    $RUN_ID"
echo "  App dir:   $APP_DIR_NATIVE"
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

# Escape backslashes for JSON (Windows paths have backslashes)
APP_DIR_JSON="${APP_DIR_NATIVE//\\/\\\\}"

cat > "$RUN_DIR/metadata.json" <<EOF
{
  "level": $LEVEL,
  "backend": "$BACKEND",
  "timestamp": "$TIMESTAMP",
  "startedAt": "$START_TIME",
  "startedAtUtc": "$START_TIME_UTC",
  "runId": "$RUN_ID",
  "appDir": "$APP_DIR_JSON",
  "promptFile": "$(basename "$PROMPT_FILE")",
  "phase": "generate"
}
EOF

# ─── Build the prompt ────────────────────────────────────────────────────────

if [[ -n "$FIX_MODE" ]]; then
  # ─── FIX MODE: Read bug report, fix code, redeploy ──────────────────────

  # In fix mode, APP_DIR is the existing app dir
  APP_DIR="$FIX_APP_DIR"
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    APP_DIR_NATIVE=$(cygpath -w "$APP_DIR")
  else
    APP_DIR_NATIVE="$APP_DIR"
  fi

  if [[ ! -f "$APP_DIR/BUG_REPORT.md" ]]; then
    echo "ERROR: No BUG_REPORT.md found in $APP_DIR"
    echo "Run the grading session first to produce a bug report."
    exit 1
  fi

  echo "=== Exhaust Test: Fix Iteration ==="
  echo "  App dir: $APP_DIR_NATIVE"
  echo "  Bug report: $APP_DIR_NATIVE/BUG_REPORT.md"
  echo ""

  # Detect backend from existing app directory structure
  if [[ -d "$APP_DIR/backend/spacetimedb" ]]; then
    FIX_BACKEND="spacetime"
  elif [[ -d "$APP_DIR/server" ]]; then
    FIX_BACKEND="postgres"
  else
    FIX_BACKEND="unknown"
  fi

  PROMPT=$(cat <<PROMPT_EOF
Fix the bugs in the exhaust test app.

**App directory:** $APP_DIR_NATIVE
**Backend:** $FIX_BACKEND

**Instructions:**
1. Read backends/$FIX_BACKEND.md for backend-specific redeploy instructions
2. Read BUG_REPORT.md in the app directory — it describes what's broken
3. Read the relevant source code files mentioned in the bug report
4. Fix each bug described in the report
5. Redeploy as needed (see backend file for steps)
6. Verify: npx tsc --noEmit && npm run build
7. Make sure the dev server is running on port 5173
8. Append this fix iteration to ITERATION_LOG.md in the app directory

Do NOT do browser testing — that happens in the grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: FIX_COMPLETE
PROMPT_EOF
  )

  MODE_LABEL="fix"

else
  # ─── GENERATE MODE: Initial code generation and deploy ──────────────────

  # Resolve absolute paths for prompt references
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    PROMPT_FILE_NATIVE=$(cygpath -w "$PROMPT_FILE")
    LANG_PROMPT_NATIVE=$(cygpath -w "$SCRIPT_DIR/../apps/chat-app/prompts/language/typescript-$BACKEND.md")
  else
    PROMPT_FILE_NATIVE="$PROMPT_FILE"
    LANG_PROMPT_NATIVE="$SCRIPT_DIR/../apps/chat-app/prompts/language/typescript-$BACKEND.md"
  fi

  PROMPT=$(cat <<PROMPT_EOF
Run the exhaust test benchmark — GENERATE AND DEPLOY ONLY.

**Configuration:**
- Level: $LEVEL
- Backend: $BACKEND
- App output directory: $APP_DIR_NATIVE (this is also your working directory)
- Run ID: $RUN_ID

**Instructions:**
1. Read the CLAUDE.md in this directory — it has backend-specific setup, architecture, and phase instructions
2. Read the language prompt: $LANG_PROMPT_NATIVE
3. Read the feature prompt: $PROMPT_FILE_NATIVE
4. Follow the phases in CLAUDE.md to generate, build, and deploy the app
5. Write all code in the current directory (server/ and client/ subdirectories)

If the build fails, fix and retry (up to 3 times per phase).
Write an ITERATION_LOG.md tracking any build reprompts.

Do NOT do browser testing — that happens in a separate grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: DEPLOY_COMPLETE
PROMPT_EOF
  )

  MODE_LABEL="generate"
fi

echo "Starting Claude Code session ($MODE_LABEL)..."
echo "─────────────────────────────────────────────"

# ─── Assemble backend-specific CLAUDE.md into app directory ─────────────────
# Build CLAUDE.md at runtime by concatenating the workflow, SDK rules, and
# templates. This ensures Claude always gets the latest rules inlined directly
# (no "go find and read this other file" that it might skip).

if [[ -z "$FIX_MODE" ]]; then
  if [[ "$BACKEND" == "spacetime" ]]; then
    {
      cat "$SCRIPT_DIR/backends/spacetime.md"
      echo ""
      echo "---"
      echo ""
      cat "$SCRIPT_DIR/backends/spacetime-sdk-rules.md"
      echo ""
      echo "---"
      echo ""
      cat "$SCRIPT_DIR/backends/spacetime-templates.md"
    } > "$APP_DIR/CLAUDE.md"
    echo "Assembled CLAUDE.md from spacetime.md + sdk-rules + templates"
  else
    cp "$SCRIPT_DIR/backends/$BACKEND.md" "$APP_DIR/CLAUDE.md"
    echo "Copied backends/$BACKEND.md → app CLAUDE.md"
  fi
fi

# ─── Run Claude Code ─────────────────────────────────────────────────────────
# Run from the APP directory so CLAUDE.md auto-discovery picks up the
# backend-specific file, not the parent exhaust-test/CLAUDE.md.

cd "$APP_DIR"
$CLAUDE_CMD --print --verbose --output-format text --dangerously-skip-permissions -p "$PROMPT"
EXIT_CODE=$?

echo ""
echo "─────────────────────────────────────────────"

# ─── Record end time ─────────────────────────────────────────────────────────

END_TIME=$(date +%Y-%m-%dT%H:%M:%S%z)
END_TIME_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Update metadata with end time — use native path for Node.js on Windows
METADATA_FILE_NATIVE="$RUN_DIR_NATIVE/metadata.json"
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
  METADATA_FILE_NATIVE=$(cygpath -w "$RUN_DIR/metadata.json")
fi
node -e "
const fs = require('fs');
const f = process.argv[1];
const m = JSON.parse(fs.readFileSync(f, 'utf-8'));
m.endedAt = '$END_TIME';
m.endedAtUtc = '$END_TIME_UTC';
m.exitCode = $EXIT_CODE;
m.mode = '$MODE_LABEL';
fs.writeFileSync(f, JSON.stringify(m, null, 2));
" -- "$METADATA_FILE_NATIVE" || echo "WARNING: Failed to update metadata with end time"

# ─── Parse telemetry ─────────────────────────────────────────────────────────

echo ""
echo "=== $MODE_LABEL Complete ==="
echo "  Started: $START_TIME"
echo "  Ended:   $END_TIME"
echo ""

echo "Parsing telemetry..."
if node "$SCRIPT_DIR_NATIVE/parse-telemetry.mjs" "$RUN_DIR_NATIVE"; then
  echo ""
  echo "=== Results ==="
  echo "  App:        $APP_DIR_NATIVE"
  echo "  Cost:       $RUN_DIR/COST_REPORT.md"
  echo ""
  if [[ -z "$FIX_MODE" ]]; then
    echo "=== Next Step: Grade the app ==="
    echo "  In Claude Code, say:"
    echo "    Grade the app at $APP_DIR_NATIVE"
    echo ""
  else
    echo "=== Next Step: Re-grade the app ==="
    echo "  In Claude Code, say:"
    echo "    Re-grade the app at $APP_DIR_NATIVE"
    echo ""
  fi
else
  echo "WARNING: Telemetry parsing failed. Raw logs at: $TELEMETRY_DIR/logs.jsonl"
fi
