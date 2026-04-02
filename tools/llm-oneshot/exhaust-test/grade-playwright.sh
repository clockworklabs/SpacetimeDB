#!/bin/bash
# Exhaust Test — Playwright Grading
#
# Runs deterministic Playwright tests against a deployed app and generates
# GRADING_RESULTS.md. This is an alternative to the Chrome MCP grading agent.
#
# Usage:
#   ./grade-playwright.sh <app-dir>
#   ./grade-playwright.sh sequential-upgrade/sequential-upgrade-20260401/results/spacetime/chat-app-20260401-123403
#
# Prerequisites:
#   cd test-plans/playwright && npm install && npx playwright install chromium

set -euo pipefail

APP_DIR="${1:?Usage: ./grade-playwright.sh <app-dir>}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PLAYWRIGHT_DIR="$SCRIPT_DIR/test-plans/playwright"

if [[ ! -d "$APP_DIR" ]]; then
  echo "ERROR: App directory not found: $APP_DIR"
  exit 1
fi

# Check Playwright is installed
if [[ ! -f "$PLAYWRIGHT_DIR/node_modules/.bin/playwright" ]]; then
  echo "ERROR: Playwright not installed."
  echo "Run: cd test-plans/playwright && npm install && npx playwright install chromium"
  exit 1
fi

# Auto-detect backend from app directory structure
if [[ -d "$APP_DIR/backend/spacetimedb" ]]; then
  GRADE_BACKEND="spacetime"
  VITE_PORT=5173
elif [[ -d "$APP_DIR/server" ]]; then
  GRADE_BACKEND="postgres"
  VITE_PORT=5174
else
  GRADE_BACKEND="unknown"
  VITE_PORT=5173
fi

APP_URL="http://localhost:$VITE_PORT"

echo "=== Exhaust Test: Playwright Grade ==="
echo "  App dir:  $APP_DIR"
echo "  Backend:  $GRADE_BACKEND (port $VITE_PORT)"
echo "  URL:      $APP_URL"
echo ""

# Run Playwright tests
cd "$PLAYWRIGHT_DIR"
APP_URL="$APP_URL" npx playwright test --reporter=json 2>&1 | tee test-results/raw-output.json || true

# Parse results into GRADING_RESULTS.md
if [[ -f "test-results/results.json" ]]; then
  echo ""
  echo "Parsing Playwright results..."

  # On Windows, convert paths for Node.js
  APP_DIR_NATIVE="$APP_DIR"
  RESULTS_FILE="$PLAYWRIGHT_DIR/test-results/results.json"
  if [[ "$OSTYPE" == "msys" || "$OSTYPE" == "cygwin" ]]; then
    APP_DIR_NATIVE=$(cygpath -w "$APP_DIR")
    RESULTS_FILE=$(cygpath -w "$RESULTS_FILE")
  fi

  node "$SCRIPT_DIR/parse-playwright-results.mjs" "$RESULTS_FILE" "$APP_DIR_NATIVE" "$GRADE_BACKEND"

  echo ""
  echo "=== Results ==="
  echo "  GRADING_RESULTS.md written to: $APP_DIR"
else
  echo "ERROR: No Playwright results found at test-results/results.json"
  exit 1
fi
