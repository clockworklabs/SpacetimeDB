#!/bin/bash
# Clean up generated app directories after testing is complete.
#
# Removes isolation git repos, build artifacts, and temp files.
# Run this after you're done grading and have recorded results.
#
# Usage:
#   ./cleanup.sh <app-dir>            # clean one app
#   ./cleanup.sh --all                # clean all apps in all variants
#   ./cleanup.sh --variant one-shot   # clean all apps in a variant

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cleanup_app() {
  local app_dir="$1"
  if [[ ! -d "$app_dir" ]]; then return; fi

  echo "Cleaning: $app_dir"

  # Remove isolation git repo
  rm -rf "$app_dir/.git" 2>/dev/null && echo "  removed .git"

  # Remove node_modules (can be reinstalled)
  for nm in "$app_dir"/*/node_modules "$app_dir"/node_modules; do
    if [[ -d "$nm" ]]; then
      rm -rf "$nm" 2>/dev/null && echo "  removed $(basename $(dirname $nm))/node_modules"
    fi
  done

  # Remove build artifacts
  rm -rf "$app_dir"/*/dist "$app_dir"/*/.vite 2>/dev/null

  # Remove dev server logs
  rm -f "$app_dir"/*.log "$app_dir"/*/*.log 2>/dev/null

  echo "  done"
}

if [[ "${1:-}" == "--all" ]]; then
  for app_dir in "$SCRIPT_DIR"/*/*/results/*/chat-app-*; do
    [[ -d "$app_dir" ]] && cleanup_app "$app_dir"
  done
elif [[ "${1:-}" == "--variant" ]]; then
  VARIANT="${2:?Usage: ./cleanup.sh --variant <variant-name>}"
  for app_dir in "$SCRIPT_DIR/$VARIANT"/*/results/*/chat-app-*; do
    [[ -d "$app_dir" ]] && cleanup_app "$app_dir"
  done
else
  APP_DIR="${1:?Usage: ./cleanup.sh <app-dir> | --all | --variant <name>}"
  cleanup_app "$APP_DIR"
fi
