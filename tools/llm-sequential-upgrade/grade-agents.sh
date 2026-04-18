#!/bin/bash
# Sequential Upgrade — Playwright Agents Grading
#
# Uses Playwright's AI-powered agents to grade a deployed app.
# The Generator agent discovers UI elements from the live DOM,
# writes tests with validated selectors, and runs them.
# The Healer agent auto-fixes failing selectors.
#
# Usage:
#   ./grade-agents.sh <app-dir>
#
# Prerequisites:
#   cd test-plans/playwright && npm install && npx playwright install chromium
#   npx playwright init-agents --loop=claude

set -euo pipefail

APP_DIR="${1:?Usage: ./grade-agents.sh <app-dir>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAYWRIGHT_DIR="$SCRIPT_DIR/test-plans/playwright"

if [[ ! -d "$APP_DIR" ]]; then
  echo "ERROR: App directory not found: $APP_DIR"
  exit 1
fi

# Check Playwright agents are initialized
if [[ ! -f "$PLAYWRIGHT_DIR/.claude/agents/playwright-test-generator.md" ]]; then
  echo "ERROR: Playwright agents not initialized."
  echo "Run: cd test-plans/playwright && npx playwright init-agents --loop=claude"
  exit 1
fi

# Auto-detect backend
if [[ -d "$APP_DIR/backend/spacetimedb" ]]; then
  GRADE_BACKEND="spacetime"
  DEFAULT_PORT=5173
elif [[ -d "$APP_DIR/server" ]]; then
  GRADE_BACKEND="postgres"
  DEFAULT_PORT=5174
else
  GRADE_BACKEND="unknown"
  DEFAULT_PORT=5173
fi

# Try to read port from metadata
VITE_PORT="$DEFAULT_PORT"
RUN_BASE="$(cd "$APP_DIR/../../.." 2>/dev/null && pwd)"
if [[ -d "$RUN_BASE/telemetry" ]]; then
  LATEST_META=$(find "$RUN_BASE/telemetry" -name "metadata.json" -path "*$GRADE_BACKEND*" -exec ls -t {} + 2>/dev/null | head -1)
  if [[ -n "$LATEST_META" ]]; then
    META_PORT=$(node -e "const m=JSON.parse(require('fs').readFileSync(process.argv[1],'utf-8')); process.stdout.write(String(m.vitePort||''))" -- "$(cygpath -w "$LATEST_META" 2>/dev/null || echo "$LATEST_META")" 2>/dev/null)
    if [[ -n "$META_PORT" ]]; then
      VITE_PORT="$META_PORT"
    fi
  fi
fi

APP_URL="http://localhost:$VITE_PORT"

echo "=== Sequential Upgrade: Playwright Agents Grade ==="
echo "  App dir:  $APP_DIR"
echo "  Backend:  $GRADE_BACKEND (port $VITE_PORT)"
echo "  URL:      $APP_URL"
echo ""

# Reset backend state for a clean test
echo "Resetting backend state..."
"$SCRIPT_DIR/reset-app.sh" "$APP_DIR" || echo "WARNING: Backend reset failed"
sleep 3

# Update seed test to point at the correct URL
cat > "$PLAYWRIGHT_DIR/specs/seed.spec.ts" <<EOF
import { test, expect } from '@playwright/test';

test.describe('Seed', () => {
  test('seed', async ({ page }) => {
    await page.goto('$APP_URL');
    await page.waitForSelector('input, button', { timeout: 30_000 });
  });
});
EOF

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

CLAUDE_CMD=""
if command -v claude &>/dev/null; then
  CLAUDE_CMD="claude"
elif command -v claude.exe &>/dev/null; then
  CLAUDE_CMD="claude.exe"
else
  echo "ERROR: Claude Code CLI not found."
  exit 1
fi

echo ""
echo "=== Phase 1: Generate Tests ==="
echo "Running Playwright Test Generator agent..."
echo ""

cd "$PLAYWRIGHT_DIR"

# Invoke the Generator agent via Claude Code to create tests from the plan
$CLAUDE_CMD --print --dangerously-skip-permissions -p "
You are running the Playwright Test Generator agent.

Read the test plan at specs/plans/chat-app-features.md.
For each test scenario in the plan:
1. Use generator_setup_page to open the app
2. Execute each step using the Playwright MCP tools (browser_click, browser_type, browser_snapshot, etc.)
3. Read the generator log with generator_read_log
4. Write the test with generator_write_test

The app is running at $APP_URL. Generate tests for all scenarios in the plan.
Important: Use browser_snapshot to inspect the DOM before interacting — do NOT guess selectors.
" 2>&1 | tee "$APP_DIR/agent-generator-output.log"

echo ""
echo "=== Phase 2: Run Generated Tests ==="

# Run whatever tests were generated
APP_URL="$APP_URL" npx playwright test --reporter=json \
  1>/tmp/pw-agent-results.json 2>/dev/null || true

RESULTS_SIZE=$(wc -c < /tmp/pw-agent-results.json 2>/dev/null || echo "0")

if [[ "$RESULTS_SIZE" -gt 100 ]]; then
  echo ""
  echo "=== Phase 3: Parse Results ==="

  PW_RESULTS="/tmp/pw-agent-results.json"
  APP_DIR_NATIVE="$APP_DIR"
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    PW_RESULTS=$(cygpath -w "$PW_RESULTS")
    APP_DIR_NATIVE=$(cygpath -w "$APP_DIR")
  fi

  node "$SCRIPT_DIR/parse-playwright-results.mjs" "$PW_RESULTS" "$APP_DIR_NATIVE" "$GRADE_BACKEND"

  echo ""
  echo "=== Results ==="
  echo "  GRADING_RESULTS.md: $APP_DIR"
  echo "  Generator log: $APP_DIR/agent-generator-output.log"
else
  echo "WARNING: No test results produced."
  echo "Check the generator output: $APP_DIR/agent-generator-output.log"
fi

cd "$SCRIPT_DIR"
