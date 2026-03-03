#!/usr/bin/env bash
set -euo pipefail

# Sync SpacetimeDB/demo/ninja-game from the standalone mirror repository.
# Usage: ./demo/sync-ninja-game-from-mirror.sh [repo-url] [branch]

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
TARGET_DIR="$REPO_ROOT/demo/ninja-game"
REPO_URL="${1:-https://github.com/avias8/spacetimedb-ninja-game.git}"
BRANCH="${2:-master}"

TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo "Cloning $REPO_URL ($BRANCH)..."
git clone --depth 1 --branch "$BRANCH" "$REPO_URL" "$TMP_DIR/repo"

echo "Syncing into $TARGET_DIR..."
rsync -a --delete \
  --exclude ".git" \
  --exclude ".DS_Store" \
  --exclude "client-swift/.build" \
  --exclude "spacetimedb/target" \
  "$TMP_DIR/repo/" "$TARGET_DIR/"

echo "Done. Review with: git -C \"$REPO_ROOT\" status -- demo/ninja-game"
