#!/bin/bash
# Sequential Upgrade — Grade & Test Loop
#
# Tests a deployed app via Chrome MCP, writes bug reports for the fix agent.
# This runs INTERACTIVELY in Claude Code (not headless) because it needs Chrome MCP.
#
# Usage:
#   ./grade.sh <app-dir>
#   ./grade.sh sequential-upgrade/sequential-upgrade-20260401/results/spacetime/chat-app-20260401-123403
#
# This script is a convenience wrapper. You can also just open Claude Code
# in the llm-sequential-upgrade/ directory and say:
#   "Grade the app at results/spacetime/chat-app-20260331-083613"

set -euo pipefail

APP_DIR="${1:?Usage: ./grade.sh <app-dir>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

if [[ ! -d "$APP_DIR" ]]; then
  echo "ERROR: App directory not found: $APP_DIR"
  exit 1
fi

# On Windows, convert to native path
if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
  APP_DIR_NATIVE=$(cygpath -w "$APP_DIR")
  SCRIPT_DIR_NATIVE=$(cygpath -w "$SCRIPT_DIR")
else
  APP_DIR_NATIVE="$APP_DIR"
  SCRIPT_DIR_NATIVE="$SCRIPT_DIR"
fi

# Find Claude CLI
CLAUDE_CMD=""
if command -v claude &>/dev/null; then
  CLAUDE_CMD="claude"
elif command -v claude.exe &>/dev/null; then
  CLAUDE_CMD="claude.exe"
elif command -v npx &>/dev/null; then
  CLAUDE_CMD="npx @anthropic-ai/claude-code"
else
  echo "ERROR: Claude Code CLI not found (tried: claude, claude.exe, npx)."
  echo "Install it with: npm install -g @anthropic-ai/claude-code"
  exit 1
fi

echo "=== Sequential Upgrade: Grade ==="
echo "  App dir: $APP_DIR_NATIVE"
echo ""
echo "This launches an INTERACTIVE Claude Code session with Chrome MCP."
echo "It will test the deployed app, write bug reports, and grade features."
echo ""

# Auto-detect backend. Prefer the marker run.sh writes at generate time; fall
# back to directory shape. The marker is the only reliable way to tell postgres
# and mongodb apart (both use a server/ dir).
if [[ -f "$APP_DIR/.benchmark-backend" ]]; then
  GRADE_BACKEND="$(tr -d '[:space:]' < "$APP_DIR/.benchmark-backend")"
elif [[ -d "$APP_DIR/backend/spacetimedb" ]]; then
  GRADE_BACKEND="spacetime"
elif [[ -d "$APP_DIR/server" ]]; then
  GRADE_BACKEND="postgres"
else
  GRADE_BACKEND="unknown"
fi

# Resolve the Vite port the app was actually deployed on. Default to the
# per-backend range used by run.sh (spacetime 6173 / postgres 6273 / mongodb 6373),
# then override with the recorded vitePort from the run's metadata.json if present
# (handles parallel runs with run-index port offsets).
case "$GRADE_BACKEND" in
  spacetime) VITE_PORT=6173 ;;
  postgres)  VITE_PORT=6273 ;;
  mongodb)   VITE_PORT=6373 ;;
  *)         VITE_PORT=6173 ;;
esac
_META=$(ls -t "$APP_DIR"/../../telemetry/*/metadata.json 2>/dev/null | head -1)
if [[ -n "$_META" ]]; then
  _META_ARG=$(cygpath -w "$_META" 2>/dev/null || echo "$_META")
  _VP=$(node -e "try{const j=JSON.parse(require('fs').readFileSync(process.argv[1],'utf8'));if(j.vitePort)process.stdout.write(String(j.vitePort));}catch(e){}" -- "$_META_ARG" 2>/dev/null || echo "")
  [[ -n "$_VP" ]] && VITE_PORT="$_VP"
fi
echo "  Backend:  $GRADE_BACKEND (port $VITE_PORT)"

# Interactive mode — no --print, no --dangerously-skip-permissions
cd "$SCRIPT_DIR"
$CLAUDE_CMD -p "Grade the sequential upgrade app at: $APP_DIR_NATIVE

Backend: $GRADE_BACKEND

Follow CLAUDE.md Phases 6-8:
1. Open http://localhost:$VITE_PORT in Chrome and verify the app loads
2. Test each feature using the test plans in test-plans/feature-*.md
3. Score each feature 0-3 based on browser observations
4. If any features score < 3, write a BUG_REPORT.md in the app directory with:
   - Which features failed and why
   - Exact error messages or broken behaviors observed
   - Console errors from read_console_messages
5. Write GRADING_RESULTS.md with scores
6. Write/update ITERATION_LOG.md with this test iteration

After grading, if there are bugs, tell the user to run:
  ./run.sh --fix $APP_DIR_NATIVE"
