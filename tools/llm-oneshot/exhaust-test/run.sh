#!/bin/bash -l
# Exhaust Test Launcher — Phase 1: Generate & Deploy
#
# Runs code generation and deployment in headless Claude Code with OTel tracking.
# After this completes, run grade.sh to do browser testing and grading interactively.
#
# Usage:
#   ./run.sh                                    # defaults: level=1, backend=spacetime, variant=sequential-upgrade
#   ./run.sh --level 5 --backend postgres       # generate from scratch at level 5
#   ./run.sh --variant one-shot --backend spacetime  # one-shot: all features in one prompt
#   ./run.sh --rules standard --backend spacetime   # standard: SDK rules only, no templates
#   ./run.sh --run-index 1 --backend spacetime      # parallel run with offset ports
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

# Configurable container name for PostgreSQL backend
POSTGRES_CONTAINER="${POSTGRES_CONTAINER:-exhaust-test-postgres-1}"

# ─── Parse arguments ─────────────────────────────────────────────────────────

LEVEL=1
LEVEL_EXPLICIT=""
BACKEND="spacetime"
VARIANT="sequential-upgrade"
RULES="guided"
TEST_MODE=""  # playwright | chrome-mcp | (empty = no automated testing)
RUN_INDEX=0
FIX_MODE=""
FIX_APP_DIR=""
UPGRADE_MODE=""
UPGRADE_APP_DIR=""
RESUME_SESSION=""
while [[ $# -gt 0 ]]; do
  case $1 in
    --level) LEVEL="$2"; LEVEL_EXPLICIT=1; shift 2 ;;
    --backend) BACKEND="$2"; shift 2 ;;
    --variant) VARIANT="$2"; shift 2 ;;
    --rules) RULES="$2"; shift 2 ;;
    --test) TEST_MODE="$2"; shift 2 ;;
    --run-index) RUN_INDEX="$2"; shift 2 ;;
    --fix) FIX_MODE=1; FIX_APP_DIR="$2"; shift 2 ;;
    --upgrade) UPGRADE_MODE=1; UPGRADE_APP_DIR="$2"; shift 2 ;;
    --resume-session) RESUME_SESSION=1; shift ;;
    *) echo "Unknown option: $1"; exit 1 ;;
  esac
done

# Validate rules level
case "$RULES" in
  guided|standard|minimal) ;;
  *) echo "ERROR: --rules must be guided, standard, or minimal"; exit 1 ;;
esac

# ─── Port allocation ──────────────────────────────────────────────────────────
# Each run-index offsets all ports by 100 to allow parallel execution.
#   Run 0: Vite(stdb)=5173, Vite(pg)=5174, Express=3001, PG=5433
#   Run 1: Vite(stdb)=5273, Vite(pg)=5274, Express=3101, PG=5533
#   Run 2: Vite(stdb)=5373, Vite(pg)=5374, Express=3201, PG=5633
PORT_OFFSET=$((RUN_INDEX * 100))
VITE_PORT_STDB=$((5173 + PORT_OFFSET))
VITE_PORT_PG=$((5174 + PORT_OFFSET))
EXPRESS_PORT=$((3001 + PORT_OFFSET))
PG_PORT=5433  # Shared container, isolation via per-run database names
STDB_PORT=3000  # SpacetimeDB server is shared, modules are isolated by name

if [[ "$BACKEND" == "spacetime" ]]; then
  VITE_PORT=$VITE_PORT_STDB
else
  VITE_PORT=$VITE_PORT_PG
fi

# Variant-specific defaults
if [[ "$VARIANT" == "one-shot" ]]; then
  if [[ -z "$LEVEL_EXPLICIT" ]]; then
    LEVEL=12  # one-shot defaults to all features
  fi
  if [[ -n "$UPGRADE_MODE" ]]; then
    echo "WARNING: --upgrade is not meaningful with --variant one-shot"
    echo "One-shot generates all features in a single session."
    UPGRADE_MODE=""
    UPGRADE_APP_DIR=""
  fi
fi

# Determine mode label early (used in metadata and output)
if [[ -n "$FIX_MODE" ]]; then
  MODE_LABEL="fix"
elif [[ -n "$UPGRADE_MODE" ]]; then
  MODE_LABEL="upgrade"
else
  MODE_LABEL="generate"
fi

# ─── Find Claude CLI ─────────────────────────────────────────────────────────

# Add Claude Code desktop install to PATH if not already findable
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

CLAUDE_CMD=""
if command -v claude &>/dev/null; then
  CLAUDE_CMD="claude"
elif command -v claude.exe &>/dev/null; then
  CLAUDE_CMD="claude.exe"
else
  if command -v npx &>/dev/null; then
    if npx @anthropic-ai/claude-code --version &>/dev/null; then
      CLAUDE_CMD="npx @anthropic-ai/claude-code"
    else
      echo "ERROR: Claude Code CLI not found via npx."
      echo "Install it with: npm install -g @anthropic-ai/claude-code"
      exit 1
    fi
  else
    echo "ERROR: Claude Code CLI not found (tried: claude, claude.exe, npx)."
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

PG_DATABASE="spacetime"
PG_CONNECTION_URL="postgresql://spacetime:spacetime@localhost:5433/spacetime"

if [[ "$BACKEND" == "spacetime" ]]; then
  if spacetime server ping local &>/dev/null; then
    echo "[OK] SpacetimeDB is running"
  else
    echo "[FAIL] SpacetimeDB is not running. Start it with: spacetime start"
    exit 1
  fi
elif [[ "$BACKEND" == "postgres" ]]; then
  if docker exec "$POSTGRES_CONTAINER" psql -U spacetime -d spacetime -c "SELECT 1" &>/dev/null; then
    echo "[OK] PostgreSQL container is running"
  else
    echo "[FAIL] PostgreSQL is not reachable. Check Docker container $POSTGRES_CONTAINER."
    exit 1
  fi

  # Per-run database isolation: each run-index gets its own database
  # Run 0 uses "spacetime" (default), Run N uses "spacetime_runN"
  if [[ $RUN_INDEX -gt 0 ]]; then
    PG_DATABASE="spacetime_run${RUN_INDEX}"
    # Create the database if it doesn't exist
    docker exec "$POSTGRES_CONTAINER" psql -U spacetime -d spacetime -c \
      "SELECT 1 FROM pg_database WHERE datname = '$PG_DATABASE'" | grep -q 1 || \
      docker exec "$POSTGRES_CONTAINER" psql -U spacetime -d spacetime -c \
      "CREATE DATABASE $PG_DATABASE OWNER spacetime;" 2>/dev/null
    echo "[OK] PostgreSQL database: $PG_DATABASE (run-index $RUN_INDEX)"
  else
    PG_DATABASE="spacetime"
    echo "[OK] PostgreSQL database: $PG_DATABASE (default)"
  fi
  PG_CONNECTION_URL="postgresql://spacetime:spacetime@localhost:5433/$PG_DATABASE"
fi

if ! docker info &>/dev/null; then
  echo "[FAIL] Docker is not running."
  exit 1
fi

# Shared telemetry directory (OTel Collector writes here)
SHARED_TELEMETRY_DIR="$SCRIPT_DIR/telemetry"
mkdir -p "$SHARED_TELEMETRY_DIR"

# Rotate telemetry log if over 10MB to prevent unbounded growth
LOGS_FILE="$SHARED_TELEMETRY_DIR/logs.jsonl"
if [[ -f "$LOGS_FILE" ]]; then
  SIZE=$(wc -c < "$LOGS_FILE")
  if [[ $SIZE -gt 10485760 ]]; then
    ARCHIVE="$SHARED_TELEMETRY_DIR/logs-$(date +%Y%m%d-%H%M%S).jsonl.bak"
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

# Strip UI contracts from prompt if not using Playwright testing
if [[ "$TEST_MODE" != "playwright" ]]; then
  STRIPPED_PROMPT="/tmp/exhaust-prompt-${RUN_INDEX}-$(basename "$PROMPT_FILE")"
  # Remove **UI contract:** blocks (from the line through the next blank line or next ###)
  sed '/^\*\*UI contract:\*\*/,/^$/d; /^\*\*Important:\*\* Each feature below includes/d' "$PROMPT_FILE" > "$STRIPPED_PROMPT"
  PROMPT_FILE="$STRIPPED_PROMPT"
  echo "[OK] UI contracts stripped (test=$TEST_MODE)"
fi

echo ""

# ─── Create run directories ─────────────────────────────────────────────────

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
DATE_STAMP=$(date +%Y%m%d)
START_TIME=$(date +%Y-%m-%dT%H:%M:%S%z)
START_TIME_UTC=$(date -u +%Y-%m-%dT%H:%M:%SZ)

# Variant-based directory structure:
#   exhaust-test/<variant>/<variant>-YYYYMMDD/
#     results/<backend>/chat-app-<timestamp>/
#     telemetry/<run-id>/
#     inputs/  (snapshot of all inputs)
VARIANT_DIR="$SCRIPT_DIR/$VARIANT"

# For upgrade/fix, reuse the existing RUN_BASE_DIR from the app's parent structure.
# For generate, create a new dated run directory.
if [[ -n "$UPGRADE_MODE" || -n "$FIX_MODE" ]]; then
  # Derive RUN_BASE_DIR from existing app directory structure:
  #   <variant>/<variant>-DATE/results/<backend>/chat-app-*/
  if [[ -n "$UPGRADE_MODE" ]]; then
    APP_DIR="$UPGRADE_APP_DIR"
  else
    APP_DIR="$FIX_APP_DIR"
  fi
  # Walk up from app dir: chat-app-* → <backend> → results → <variant>-DATE
  RUN_BASE_DIR="$(cd "$APP_DIR/../../.." 2>/dev/null && pwd)"
  # Validate it looks like a run base dir (has results/ subdir)
  if [[ ! -d "$RUN_BASE_DIR/results" ]]; then
    # Fallback: create new run base dir (legacy app dir not under variant structure)
    RUN_BASE_DIR="$VARIANT_DIR/$VARIANT-$DATE_STAMP"
  fi
  TELEMETRY_DIR="$RUN_BASE_DIR/telemetry"
  RESULTS_DIR="$RUN_BASE_DIR/results"
else
  # Generate mode: create new dated run directory
  RUN_BASE_DIR="$VARIANT_DIR/$VARIANT-$DATE_STAMP"
  # Handle duplicate dates (second run on same day)
  if [[ -d "$RUN_BASE_DIR" ]]; then
    SEQ=2
    while [[ -d "$RUN_BASE_DIR-$SEQ" ]]; do ((SEQ++)); done
    RUN_BASE_DIR="$RUN_BASE_DIR-$SEQ"
  fi
  TELEMETRY_DIR="$RUN_BASE_DIR/telemetry"
  RESULTS_DIR="$RUN_BASE_DIR/results"
fi

if [[ -n "$UPGRADE_MODE" ]]; then
  RUN_ID="$BACKEND-upgrade-to-level$LEVEL-$TIMESTAMP"
elif [[ -n "$FIX_MODE" ]]; then
  RUN_ID="$BACKEND-fix-level$LEVEL-$TIMESTAMP"
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
echo "  Variant:   $VARIANT"
echo "  Rules:     $RULES"
echo "  Level:     $LEVEL"
echo "  Backend:   $BACKEND"
echo "  Run index: $RUN_INDEX (Vite=$VITE_PORT)"
echo "  Run ID:    $RUN_ID"
echo "  Run base:  $RUN_BASE_DIR"
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
  "variant": "$VARIANT",
  "rules": "$RULES",
  "testMode": "${TEST_MODE:-none}",
  "runIndex": $RUN_INDEX,
  "vitePort": $VITE_PORT,
  "expressPort": $EXPRESS_PORT,
  "pgDatabase": "${PG_DATABASE:-}",
  "sessionId": "$SESSION_ID"
}
EOF

# ─── Snapshot inputs ───────────────────────────────────────────────────────
# Copy all inputs (prompts, backend specs, tooling, etc.) into the run directory
# so each run is self-contained and reproducible even if the tooling changes.

snapshot_inputs() {
  local INPUTS_DIR="$RUN_BASE_DIR/inputs"
  if [[ -d "$INPUTS_DIR" ]]; then
    return  # already snapshotted (upgrade/fix into existing run)
  fi
  mkdir -p "$INPUTS_DIR/backends" "$INPUTS_DIR/test-plans" \
           "$INPUTS_DIR/prompts/composed" "$INPUTS_DIR/prompts/language"

  # Shared tooling
  for f in CLAUDE.md run.sh grade.sh parse-telemetry.mjs \
           docker-compose.otel.yaml otel-collector-config.yaml \
           DEVELOP.md .gitignore; do
    cp "$SCRIPT_DIR/$f" "$INPUTS_DIR/" 2>/dev/null || true
  done

  # Backend specs (only relevant backend)
  cp "$SCRIPT_DIR/backends/$BACKEND.md" "$INPUTS_DIR/backends/" 2>/dev/null || true
  if [[ "$BACKEND" == "spacetime" ]]; then
    cp "$SCRIPT_DIR/backends/spacetime-sdk-rules.md" "$INPUTS_DIR/backends/" 2>/dev/null || true
    cp "$SCRIPT_DIR/backends/spacetime-templates.md" "$INPUTS_DIR/backends/" 2>/dev/null || true
  fi

  # Test plans
  cp "$SCRIPT_DIR/test-plans/"*.md "$INPUTS_DIR/test-plans/" 2>/dev/null || true

  # Prompts (only relevant language file, all composed levels)
  local PROMPTS_SRC="$SCRIPT_DIR/../apps/chat-app/prompts"
  cp "$PROMPTS_SRC/composed/"*.md "$INPUTS_DIR/prompts/composed/" 2>/dev/null || true
  cp "$PROMPTS_SRC/language/typescript-$BACKEND.md" "$INPUTS_DIR/prompts/language/" 2>/dev/null || true

  echo "  Inputs snapshotted to $INPUTS_DIR"
}

snapshot_inputs

# Write app-dir.txt so benchmark.sh can find the app directory without racing
echo "$APP_DIR" > "$RUN_DIR/app-dir.txt"

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
7. Make sure the dev server is running on port $VITE_PORT
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

  # Read language and feature files to inline into the prompt
  LANG_CONTENT=$(cat "$SCRIPT_DIR/../apps/chat-app/prompts/language/typescript-$UPGRADE_BACKEND.md" 2>/dev/null || echo "")
  FEATURE_CONTENT=$(cat "$PROMPT_FILE" 2>/dev/null || echo "")

  PROMPT=$(cat <<PROMPT_EOF
Upgrade the existing chat app to add the new feature(s) from level $LEVEL.

**App directory:** $APP_DIR_NATIVE
**Backend:** $UPGRADE_BACKEND
**Current level:** $PREV_LEVEL (all features from level $PREV_LEVEL are already implemented and working)
**Target level:** $LEVEL

**Instructions:**
1. Read the CLAUDE.md in this directory for backend-specific architecture and SDK reference
2. Read the existing source code to understand the current architecture
3. Add the new feature(s) to both backend and frontend, integrating with the existing code
4. Rebuild and redeploy (see CLAUDE.md for backend-specific steps)
5. Verify the build succeeds: npx tsc --noEmit && npm run build (if applicable)
6. Make sure the dev server is running on port $VITE_PORT

Features from level $PREV_LEVEL and below are ALREADY IMPLEMENTED — do NOT rewrite them.
Only add the NEW feature(s) that appear in the feature spec below but not in level $PREV_LEVEL.

Do NOT do browser testing — that happens in a separate grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: UPGRADE_COMPLETE

---

$LANG_CONTENT

---

$FEATURE_CONTENT
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

  # Read language and feature files to inline into the prompt
  LANG_CONTENT=$(cat "$SCRIPT_DIR/../apps/chat-app/prompts/language/typescript-$BACKEND.md" 2>/dev/null || echo "")
  FEATURE_CONTENT=$(cat "$PROMPT_FILE" 2>/dev/null || echo "")

  PROMPT=$(cat <<PROMPT_EOF
Run the exhaust test benchmark — GENERATE AND DEPLOY ONLY.

**Configuration:**
- Level: $LEVEL
- Backend: $BACKEND
- App output directory: $APP_DIR_NATIVE (this is also your working directory)
- Run ID: $RUN_ID

**Instructions:**
1. Read the CLAUDE.md in this directory — it has backend-specific setup, architecture, and SDK reference
2. Follow the phases in CLAUDE.md to generate, build, and deploy the app
3. Write all code in the current directory

If the build fails, fix and retry (up to 3 times per phase).
Write an ITERATION_LOG.md tracking any build reprompts.

Do NOT do browser testing — that happens in a separate grading session.
Cost tracking is automatic via OpenTelemetry — do NOT estimate tokens.

When done, output: DEPLOY_COMPLETE

---

$LANG_CONTENT

---

$FEATURE_CONTENT
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
  # Assemble CLAUDE.md based on --rules level:
  #   guided:   full phases + SDK rules + code templates (most prescriptive)
  #   standard: SDK rules only (no templates, no step-by-step phases)
  #   minimal:  just the tech stack name (least prescriptive)
  if [[ "$RULES" == "minimal" ]]; then
    if [[ "$BACKEND" == "spacetime" ]]; then
      echo "Build this app using the SpacetimeDB TypeScript SDK (npm package: spacetimedb)." > "$APP_DIR/CLAUDE.md"
      echo "Server module in backend/spacetimedb/, React client in client/." >> "$APP_DIR/CLAUDE.md"
      echo "Vite dev server port: $VITE_PORT" >> "$APP_DIR/CLAUDE.md"
    else
      echo "Build this app using PostgreSQL + Express + Socket.io + Drizzle ORM." > "$APP_DIR/CLAUDE.md"
      echo "Express server in server/, React client in client/." >> "$APP_DIR/CLAUDE.md"
      echo "PostgreSQL connection: $PG_CONNECTION_URL" >> "$APP_DIR/CLAUDE.md"
      echo "Express port: $EXPRESS_PORT | Vite port: $VITE_PORT" >> "$APP_DIR/CLAUDE.md"
    fi
    echo "Assembled minimal CLAUDE.md (rules=$RULES)"
  elif [[ "$RULES" == "standard" ]]; then
    if [[ "$BACKEND" == "spacetime" ]]; then
      cat "$SCRIPT_DIR/backends/spacetime-sdk-rules.md" > "$APP_DIR/CLAUDE.md"
    else
      echo "# PostgreSQL Backend" > "$APP_DIR/CLAUDE.md"
      echo "" >> "$APP_DIR/CLAUDE.md"
      echo "PostgreSQL connection: \`$PG_CONNECTION_URL\`" >> "$APP_DIR/CLAUDE.md"
      echo "" >> "$APP_DIR/CLAUDE.md"
      echo "Use Express (port $EXPRESS_PORT) + Socket.io + Drizzle ORM. Server in \`server/\`, client in \`client/\`." >> "$APP_DIR/CLAUDE.md"
      echo "Vite dev server port: $VITE_PORT" >> "$APP_DIR/CLAUDE.md"
    fi
    echo "Assembled standard CLAUDE.md (rules=$RULES)"
  else
    # guided (default) — full phases + SDK rules + templates
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
      echo "Assembled guided CLAUDE.md from spacetime.md + sdk-rules + templates"
    else
      cp "$SCRIPT_DIR/backends/$BACKEND.md" "$APP_DIR/CLAUDE.md"
      echo "Copied backends/$BACKEND.md → app CLAUDE.md"
    fi
  fi

  # Patch ports and database names in CLAUDE.md for parallel runs (run-index > 0)
  if [[ $RUN_INDEX -gt 0 ]]; then
    sed -i \
      -e "s/5173/$VITE_PORT_STDB/g" \
      -e "s/5174/$VITE_PORT_PG/g" \
      -e "s/:3001/:$EXPRESS_PORT/g" \
      -e "s/localhost:3001/localhost:$EXPRESS_PORT/g" \
      -e "s|localhost:5433/spacetime|localhost:5433/$PG_DATABASE|g" \
      -e "s|spacetime:spacetime@localhost:5433/spacetime|spacetime:spacetime@localhost:5433/$PG_DATABASE|g" \
      "$APP_DIR/CLAUDE.md"
    echo "  Patched for run-index=$RUN_INDEX (Vite=$VITE_PORT, Express=$EXPRESS_PORT, DB=$PG_DATABASE)"
  fi
fi

# ─── Run Claude Code ─────────────────────────────────────────────────────────
# Run from the APP directory so CLAUDE.md auto-discovery picks up the
# backend-specific file, not the parent exhaust-test/CLAUDE.md.

cd "$APP_DIR"

# Initialize a git repo in the app dir to isolate Claude Code from the parent repo.
# This prevents Claude from walking up and finding SpacetimeDB repo configs/code.
if [[ ! -d "$APP_DIR/.git" ]]; then
  git init -q "$APP_DIR" 2>/dev/null || true
  echo "node_modules/" > "$APP_DIR/.gitignore"
  git -C "$APP_DIR" add -A 2>/dev/null && git -C "$APP_DIR" commit -q -m "initial" 2>/dev/null || true
fi

# Build resume flag if --resume-session was passed and a prior session ID exists
RESUME_FLAG=""
if [[ -n "$RESUME_SESSION" && -n "$UPGRADE_MODE" ]]; then
  # Find the most recent telemetry dir for this app to get its session ID.
  # Search variant structure: <variant>/<variant>-DATE/telemetry/*/
  # Sort by modification time (newest first), break on first match.
  PREV_SESSION_ID=""
  SEARCH_DIRS=$(find "$VARIANT_DIR" -path "*/telemetry/*" -name "metadata.json" -exec dirname {} \; 2>/dev/null | sort -r)
  for tdir in $SEARCH_DIRS; do
    if [[ -f "$tdir/metadata.json" ]]; then
      META_PATH="$(cygpath -w "$tdir/metadata.json" 2>/dev/null || echo "$tdir/metadata.json")"
      TDIR_APP=$(node -e "const m=JSON.parse(require('fs').readFileSync(process.argv[1],'utf-8')); process.stdout.write(m.appDir||'')" -- "$META_PATH" 2>/dev/null)
      if [[ "$TDIR_APP" == "$APP_DIR_NATIVE" || "$TDIR_APP" == "$APP_DIR_JSON" ]]; then
        SID=$(node -e "const m=JSON.parse(require('fs').readFileSync(process.argv[1],'utf-8')); process.stdout.write(m.sessionId||'')" -- "$META_PATH" 2>/dev/null)
        if [[ -n "$SID" ]]; then
          PREV_SESSION_ID="$SID"
          break  # newest match found, stop searching
        fi
      fi
    fi
  done
  if [[ -n "$PREV_SESSION_ID" ]]; then
    RESUME_FLAG="--continue $PREV_SESSION_ID"
    echo "Continuing prior session: $PREV_SESSION_ID"
  else
    echo "No prior session ID found for this app — starting fresh"
  fi
fi

if [[ -n "$RESUME_FLAG" ]]; then
  # When continuing a prior session, don't pass --session-id (reuses the existing one)
  $CLAUDE_CMD --print --verbose --output-format text --dangerously-skip-permissions $RESUME_FLAG -p "$PROMPT"
else
  $CLAUDE_CMD --print --verbose --output-format text --dangerously-skip-permissions --session-id "$SESSION_ID" -p "$PROMPT"
fi
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

# Resolve shared logs file path for telemetry parser
LOGS_FILE_NATIVE="$SHARED_TELEMETRY_DIR/logs.jsonl"
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
  LOGS_FILE_NATIVE=$(cygpath -w "$SHARED_TELEMETRY_DIR/logs.jsonl")
fi

echo "Parsing telemetry..."
if node "$SCRIPT_DIR_NATIVE/parse-telemetry.mjs" "$RUN_DIR_NATIVE" "--logs-file=$LOGS_FILE_NATIVE" "--extract-raw"; then
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
  echo "WARNING: Telemetry parsing failed. Raw logs at: $SHARED_TELEMETRY_DIR/logs.jsonl"
fi

# ─── Auto-grade with Playwright (if installed) ──────────────────────────────

PLAYWRIGHT_DIR="$SCRIPT_DIR/test-plans/playwright"
if [[ $EXIT_CODE -eq 0 && "$TEST_MODE" == "playwright" && -f "$PLAYWRIGHT_DIR/node_modules/.bin/playwright" ]]; then
  echo ""
  echo "=== Auto-grading with Playwright ==="
  echo "  App URL: http://localhost:$VITE_PORT"

  # Wait for dev server to be ready
  READY=0
  for i in $(seq 1 30); do
    if curl -s -o /dev/null -w "%{http_code}" "http://localhost:$VITE_PORT" 2>/dev/null | grep -q "200"; then
      READY=1
      break
    fi
    sleep 1
  done

  if [[ $READY -eq 1 ]]; then
    # Reset backend state for a clean test (fresh module or DB)
    echo "Resetting backend state for clean test..."
    "$SCRIPT_DIR/reset-app.sh" "$APP_DIR" || echo "WARNING: Backend reset failed — tests may use stale state"

    # Wait for the app to reconnect after reset
    sleep 3

    # Determine which feature specs to run based on prompt level
    # Level → max feature number mapping:
    #   1=4, 2=5, 3=6, 4=7, 5=8, 6=9, 7=10, 8=11, 9=12, 10=13, 11=14, 12=15,
    #   13=16, 14=17, 15=18, 16=19, 17=20, 18=21, 19=22
    MAX_FEATURE=$((LEVEL + 3))
    if [[ $MAX_FEATURE -gt 22 ]]; then MAX_FEATURE=22; fi

    PW_SPEC_FILES=""
    for feat_num in $(seq 1 $MAX_FEATURE); do
      FEAT_PAD=$(printf '%02d' "$feat_num")
      SPEC_FILE=$(ls "$PLAYWRIGHT_DIR/specs/feature-${FEAT_PAD}-"*.spec.ts 2>/dev/null | head -1)
      if [[ -n "$SPEC_FILE" ]]; then
        PW_SPEC_FILES="$PW_SPEC_FILES $SPEC_FILE"
      fi
    done
    echo "  Testing features 1-$MAX_FEATURE ($LEVEL prompt level)"

    mkdir -p /tmp/pw-results-$RUN_INDEX
    cd "$PLAYWRIGHT_DIR"
    APP_URL="http://localhost:$VITE_PORT" npx playwright test $PW_SPEC_FILES --reporter=json \
      1>/tmp/pw-results-$RUN_INDEX/results.json 2>/dev/null || true
    cd "$APP_DIR"

    RESULTS_SIZE=$(wc -c < /tmp/pw-results-$RUN_INDEX/results.json 2>/dev/null || echo "0")
    if [[ "$RESULTS_SIZE" -gt 100 ]]; then
      PW_RESULTS="/tmp/pw-results-$RUN_INDEX/results.json"
      if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
        PW_RESULTS=$(cygpath -w "$PW_RESULTS")
      fi
      node "$SCRIPT_DIR_NATIVE/parse-playwright-results.mjs" "$PW_RESULTS" "$APP_DIR_NATIVE" "$BACKEND"
      # Copy raw results into telemetry dir for archival
      cp /tmp/pw-results-$RUN_INDEX/results.json "$RUN_DIR/playwright-results.json" 2>/dev/null || true
    else
      echo "WARNING: Playwright produced no results (app may not have loaded)"
    fi
  else
    echo "WARNING: Dev server not responding on port $VITE_PORT — skipping Playwright grading"
  fi
elif [[ $EXIT_CODE -eq 0 && "$TEST_MODE" == "agents" ]]; then
  echo ""
  echo "=== Auto-grading with Playwright Agents ==="
  "$SCRIPT_DIR/grade-agents.sh" "$APP_DIR" 2>&1 || echo "WARNING: Agent grading failed"
elif [[ $EXIT_CODE -ne 0 ]]; then
  echo "Skipping auto-grade — code generation failed (exit $EXIT_CODE)"
fi

