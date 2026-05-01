#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR" rev-parse --show-toplevel 2>/dev/null || true)"

if [[ -z "$REPO_ROOT" ]]; then
  echo "Could not determine git repository root from $SCRIPT_DIR." >&2
  exit 1
fi

cd "$REPO_ROOT"

DEFAULT_COMMIT="e45cf891c20d87b11976e1d54c04c0e4639dbe81"
COMMIT="${1:-$DEFAULT_COMMIT}"
VERSION="${2:-1.12.0}"

if ! command -v git >/dev/null 2>&1; then
  echo "git is required." >&2
  exit 1
fi

if ! command -v node >/dev/null 2>&1; then
  echo "node is required." >&2
  exit 1
fi

if ! command -v pnpm >/dev/null 2>&1; then
  echo "pnpm is required." >&2
  exit 1
fi

NODE_MAJOR="$(node -p 'process.versions.node.split(".")[0]')"
if [[ "$NODE_MAJOR" -lt 20 ]]; then
  echo "Node >= 20 is required. Current: $(node -v)" >&2
  exit 1
fi

if [[ ! -d docs ]]; then
  echo "Run this from the repo root (expected ./docs)." >&2
  exit 1
fi

if ! git rev-parse --verify "${COMMIT}^{commit}" >/dev/null 2>&1; then
  echo "Commit not found: $COMMIT" >&2
  exit 1
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/spacetimedb-docs-recut.XXXXXX")"

cleanup() {
  if [[ -n "${TMP_DIR:-}" && -d "${TMP_DIR:-}" ]]; then
    git -C "$REPO_ROOT" worktree remove --force "$TMP_DIR" >/dev/null 2>&1 || true
    rm -rf "$TMP_DIR" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

echo "Creating temp worktree at $TMP_DIR"
git worktree add --detach "$TMP_DIR" HEAD >/dev/null

echo "Restoring docs source from commit $COMMIT"
git -C "$TMP_DIR" restore --source "$COMMIT" --worktree -- docs/docs docs/sidebars.ts

TMP_CUT_VERSION="__recut_${VERSION}_$(date +%s)"
TMP_CUT_VERSIONED_DOCS="$TMP_DIR/docs/versioned_docs/version-$TMP_CUT_VERSION"
TMP_CUT_VERSIONED_SIDEBAR="$TMP_DIR/docs/versioned_sidebars/version-$TMP_CUT_VERSION-sidebars.json"

echo "Preparing temp cut version: $TMP_CUT_VERSION"
rm -rf "$TMP_CUT_VERSIONED_DOCS" "$TMP_CUT_VERSIONED_SIDEBAR"

echo "Recutting docs version: $VERSION"
echo "Installing docs dependencies in temp worktree"
pnpm --dir "$TMP_DIR/docs" install --frozen-lockfile
pnpm --dir "$TMP_DIR/docs" docusaurus docs:version "$TMP_CUT_VERSION"

DEST_VERSIONED_DOCS="$REPO_ROOT/docs/versioned_docs/version-$VERSION"
DEST_VERSIONED_SIDEBAR="$REPO_ROOT/docs/versioned_sidebars/version-$VERSION-sidebars.json"
DEST_VERSIONS_JSON="$REPO_ROOT/docs/versions.json"

echo "Copying regenerated artifacts into current branch working tree"
rm -rf "$DEST_VERSIONED_DOCS" "$DEST_VERSIONED_SIDEBAR"
cp -R "$TMP_CUT_VERSIONED_DOCS" "$REPO_ROOT/docs/versioned_docs/version-$VERSION"
cp "$TMP_CUT_VERSIONED_SIDEBAR" "$DEST_VERSIONED_SIDEBAR"

node -e '
  const fs = require("fs");
  const path = process.argv[1];
  const version = process.argv[2];
  let versions = [];
  try {
    versions = JSON.parse(fs.readFileSync(path, "utf8"));
    if (!Array.isArray(versions)) versions = [];
  } catch {
    versions = [];
  }
  versions = versions.filter((v) => v !== version);
  versions.unshift(version);
  fs.writeFileSync(path, JSON.stringify(versions, null, 2) + "\n");
' "$DEST_VERSIONS_JSON" "$VERSION"

echo
echo "Done."
echo "Updated:"
echo "  docs/versioned_docs/version-$VERSION"
echo "  docs/versioned_sidebars/version-$VERSION-sidebars.json"
echo "  docs/versions.json"
echo
echo "Run: git status"
