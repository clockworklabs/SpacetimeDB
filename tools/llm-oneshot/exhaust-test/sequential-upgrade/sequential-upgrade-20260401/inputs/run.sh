#!/bin/bash -l
# Exhaust Test Launcher — Phase 1: Generate & Deploy
#
# Runs code generation and deployment in headless Claude Code with OTel tracking.
# After this completes, run grade.sh to do browser testing and grading interactively.
#
# Usage:
#   ./run.sh                                    # defaults: level=1, backend=spacetime
#   ./run.sh --level 5 --backend postgres       # generate from scratch at level 5
#   ./run.sh --fix <app-dir>                    # fix bugs in existing app (reads BUG_REPORT.md)
#   ./run.sh --upgrade <app-dir> --level 3      # add level 3 features to existing level 2 app
#   ./run.sh --upgrade <app-dir> --level 3 --resume-session  # same, but resume prior session for cache
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
UPGRADE_MODE=""
UPGRADE_APP_DIR=""
RESUME_SESSION=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --level) LEVEL="$2"; shift 2 ;;
    --backend) BACKEND="$2"; shift 2 ;;
    --fix) FIX_MODE=1; FIX_APP_DIR="$2"; shift 2 ;;
    --upgrade) UPGRADE_MODE=1; UPGRADE_APP_DIR="$2"; shift 2 ;;
    --resume-session) RESUME_SESSION=1; shift ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Determine mode label early (used in metadata and output)
if [[ -n "$FIX_MODE" ]]; then
  MODE_LABEL="fix"
elif [[ -n "$UPGRADE_MODE" ]]; then
  MODE_LABEL="upgrade"
else
  MODE_LABEL="generate"
fi

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

if [[ -n "$UPGRADE_MODE" ]]; then
  RUN_ID="$BACKEND-upgrade-to-level$LEVEL-$TIMESTAMP"
  APP_DIR="$UPGRADE_APP_DIR"
elif [[ -n "$FIX_MODE" ]]; then
  RUN_ID="$BACKEND-fix-level$LEVEL-$TIMESTAMP"
  APP_DIR="$FIX_APP_DIR"
else
  RUN_ID="$BACKEND-level$LEVEL-$TIMESTAMP"
  APP_DIR="$RESULTS_DIR/$BACKEND/chat-app-$TIMESTAMP"
  mkdir -p "$APP_DIR"
fi

RUN_DIR="$TELEMETRY_DIR/$RUN_ID"
mkdir -p "$RUN_DIR"

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

echo "=== Exhaust Test: ${MODE_LABEL^} ==="
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

# ─── Generate session ID ───────────────────────────────────────────────────
# NOTE: OTEL_RESOURCE_ATTRIBUTES is set AFTER SESSION_ID is generated (below)
# Pre-generate a UUID so we can pass --session-id to Claude and save it in
# metadata for future --resume-session use.

SESSION_ID=$(python3 -c "import uuid; print(uuid.uuid4())" 2>/dev/null || node -e "const c=require('crypto');console.log([c.randomBytes(4),c.randomBytes(2),c.randomBytes(2),c.randomBytes(2),c.randomBytes(6)].map(b=>b.toString('hex')).join('-'))")

# Tag all OTel records with run.id and session.id so parse-telemetry.mjs can
# filter by session even when multiple backends run in parallel on the same collector.
export OTEL_RESOURCE_ATTRIBUTES="run.id=$RUN_ID,session.id=$SESSION_ID"

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
  "phase": "$MODE_LABEL",
  "sessionId": "$SESSION_ID"
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
7. Make sure the dev server is running on the correct port (SpacetimeDB: 5173, PostgreSQL: 5174)
8. Append this fix iteration to ITERATION_LOG.md in the app directory

Do NOT do browser testing — that happens in the grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: FIX_COMPLETE
PROMPT_EOF
  )

elif [[ -n "$UPGRADE_MODE" ]]; then
  # ─── UPGRADE MODE: Add new features from a higher level prompt ─────────

  APP_DIR="$UPGRADE_APP_DIR"
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    APP_DIR_NATIVE=$(cygpath -w "$APP_DIR")
  else
    APP_DIR_NATIVE="$APP_DIR"
  fi

  # ─── Snapshot previous level before upgrading ─────────────────────────
  PREV_LEVEL=$((LEVEL - 1))
  SNAPSHOT_DIR="$APP_DIR/level-$PREV_LEVEL"
  if [[ -d "$SNAPSHOT_DIR" ]]; then
    echo "Snapshot level-$PREV_LEVEL already exists — skipping snapshot"
  else
    echo "Snapshotting current app state to level-$PREV_LEVEL..."
    mkdir -p "$SNAPSHOT_DIR"
    # Copy app source dirs (exclude node_modules, dist, snapshots)
    for item in "$APP_DIR"/*; do
      base=$(basename "$item")
      case "$base" in
        level-*|node_modules|dist|.vite|drizzle|dev-server.log) continue ;;
        *) cp -r "$item" "$SNAPSHOT_DIR/" 2>/dev/null ;;
      esac
    done
    echo "  Saved to $SNAPSHOT_DIR"
  fi

  # Detect backend from existing app directory structure
  if [[ -d "$APP_DIR/backend/spacetimedb" ]]; then
    UPGRADE_BACKEND="spacetime"
  elif [[ -d "$APP_DIR/server" ]]; then
    UPGRADE_BACKEND="postgres"
  else
    UPGRADE_BACKEND="unknown"
  fi

  # Resolve prompt file path
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    PROMPT_FILE_NATIVE=$(cygpath -w "$PROMPT_FILE")
    LANG_PROMPT_NATIVE=$(cygpath -w "$SCRIPT_DIR/../apps/chat-app/prompts/language/typescript-$UPGRADE_BACKEND.md")
  else
    PROMPT_FILE_NATIVE="$PROMPT_FILE"
    LANG_PROMPT_NATIVE="$SCRIPT_DIR/../apps/chat-app/prompts/language/typescript-$UPGRADE_BACKEND.md"
  fi

  PREV_LEVEL=$((LEVEL - 1))

  echo "=== Exhaust Test: Upgrade to Level $LEVEL ==="
  echo "  App dir: $APP_DIR_NATIVE"
  echo "  Backend: $UPGRADE_BACKEND"
  echo "  From level: $PREV_LEVEL → $LEVEL"
  echo "  Prompt: $(basename "$PROMPT_FILE")"
  echo ""

  PROMPT=$(cat <<PROMPT_EOF
Upgrade the existing chat app to add the new feature(s) from level $LEVEL.

**App directory:** $APP_DIR_NATIVE
**Backend:** $UPGRADE_BACKEND
**Current level:** $PREV_LEVEL (all features from level $PREV_LEVEL are already implemented and working)
**Target level:** $LEVEL

**Instructions:**
1. Read the CLAUDE.md in this directory for backend-specific architecture and constraints
2. Read the language prompt: $LANG_PROMPT_NATIVE
3. Read the full feature prompt: $PROMPT_FILE_NATIVE
   - Features from level $PREV_LEVEL and below are ALREADY IMPLEMENTED — do NOT rewrite them
   - Only add the NEW feature(s) that appear in level $LEVEL but not in level $PREV_LEVEL
4. Read the existing source code to understand the current architecture
5. Add the new feature(s) to both backend and frontend, integrating with the existing code
6. Rebuild and redeploy (see CLAUDE.md for backend-specific steps)
7. Verify the build succeeds: npx tsc --noEmit && npm run build (if applicable)
8. Make sure the dev server is running on the correct port (SpacetimeDB: 5173, PostgreSQL: 5174)

IMPORTANT: Do NOT rewrite existing features. Only add new code for the new feature(s).
Do NOT do browser testing — that happens in a separate grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: UPGRADE_COMPLETE
PROMPT_EOF
  )

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
fi

echo "Starting Claude Code session ($MODE_LABEL)..."
echo "─────────────────────────────────────────────"

# ─── Assemble backend-specific CLAUDE.md into app directory ─────────────────
# Build CLAUDE.md at runtime by concatenating the workflow, SDK rules, and
# templates. This ensures Claude always gets the latest rules inlined directly
# (no "go find and read this other file" that it might skip).

if [[ -z "$FIX_MODE" && -z "$UPGRADE_MODE" ]]; then
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

# Build resume flag if --resume-session was passed and a prior session ID exists
RESUME_FLAG=""
if [[ -n "$RESUME_SESSION" && -n "$UPGRADE_MODE" ]]; then
  # Find the most recent telemetry dir for this app to get its session ID
  PREV_SESSION_ID=""
  for tdir in "$TELEMETRY_DIR"/*; do
    if [[ -f "$tdir/metadata.json" ]]; then
      TDIR_APP=$(node -e "const m=JSON.parse(require('fs').readFileSync(process.argv[1],'utf-8')); process.stdout.write(m.appDir||'')" -- "$(cygpath -w "$tdir/metadata.json" 2>/dev/null || echo "$tdir/metadata.json")" 2>/dev/null)
      if [[ "$TDIR_APP" == "$APP_DIR_NATIVE" || "$TDIR_APP" == "$APP_DIR_JSON" ]]; then
        SID=$(node -e "const m=JSON.parse(require('fs').readFileSync(process.argv[1],'utf-8')); process.stdout.write(m.sessionId||'')" -- "$(cygpath -w "$tdir/metadata.json" 2>/dev/null || echo "$tdir/metadata.json")" 2>/dev/null)
        if [[ -n "$SID" ]]; then
          PREV_SESSION_ID="$SID"
        fi
      fi
    fi
  done
  if [[ -n "$PREV_SESSION_ID" ]]; then
    RESUME_FLAG="--resume $PREV_SESSION_ID"
    echo "Resuming prior session: $PREV_SESSION_ID"
  else
    echo "No prior session ID found for this app — starting fresh"
  fi
fi

$CLAUDE_CMD --print --verbose --output-format text --dangerously-skip-permissions --session-id "$SESSION_ID" $RESUME_FLAG -p "$PROMPT"
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
m.sessionId = '$SESSION_ID';
fs.writeFileSync(f, JSON.stringify(m, null, 2));
" -- "$METADATA_FILE_NATIVE" || echo "WARNING: Failed to update metadata with end time"

# ─── Snapshot completed level (upgrade mode) ─────────────────────────────────

if [[ -n "$UPGRADE_MODE" && $EXIT_CODE -eq 0 ]]; then
  LEVEL_SNAPSHOT="$APP_DIR/level-$LEVEL"
  if [[ ! -d "$LEVEL_SNAPSHOT" ]]; then
    echo "Snapshotting upgraded app state to level-$LEVEL..."
    mkdir -p "$LEVEL_SNAPSHOT"
    for item in "$APP_DIR"/*; do
      base=$(basename "$item")
      case "$base" in
        level-*|node_modules|dist|.vite|drizzle|dev-server.log) continue ;;
        *) cp -r "$item" "$LEVEL_SNAPSHOT/" 2>/dev/null ;;
      esac
    done
    echo "  Saved to $LEVEL_SNAPSHOT"
  fi
fi

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
  if [[ -n "$FIX_MODE" ]]; then
    echo "=== Next Step: Re-grade the app ==="
    echo "  In Claude Code, say:"
    echo "    Re-grade the app at $APP_DIR_NATIVE"
    echo ""
  elif [[ -n "$UPGRADE_MODE" ]]; then
    echo "=== Next Step: Grade the upgraded app (level $LEVEL) ==="
    echo "  In Claude Code, say:"
    echo "    Grade the app at $APP_DIR_NATIVE at level $LEVEL"
    echo ""
    NEXT_LEVEL=$((LEVEL + 1))
    NEXT_PROMPT="$SCRIPT_DIR/../apps/chat-app/prompts/composed/$(printf '%02d' "$NEXT_LEVEL")_"*".md"
    if ls $NEXT_PROMPT &>/dev/null 2>&1; then
      echo "  To continue upgrading after grading:"
      echo "    ./run.sh --upgrade $APP_DIR --level $NEXT_LEVEL"
      echo ""
    fi
  else
    echo "=== Next Step: Grade the app ==="
    echo "  In Claude Code, say:"
    echo "    Grade the app at $APP_DIR_NATIVE"
    echo ""
  fi
else
  echo "WARNING: Telemetry parsing failed. Raw logs at: $TELEMETRY_DIR/logs.jsonl"
fi
