#!/bin/bash

set -euo pipefail

# Add rustfmt if rustup is available (non-fatal if it fails)
if command -v rustup >/dev/null 2>&1; then
	rustup component add rustfmt || true
fi

# Change to repo root (parent of this script's dir)
cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd -P)"

# Prefer configuring Git's hooks path (cross-platform) instead of creating a symlink.
# This avoids symlink requirements on Windows and preserves existing .git/hooks behavior.
if command -v git >/dev/null 2>&1; then
	git -C "$REPO_ROOT" config core.hooksPath git-hooks/hooks
	echo "Configured Git core.hooksPath to git-hooks/hooks"
else
	# Fallback: attempt to replace .git/hooks with a symlink (may require privileges on Windows)
	rm -rf "$REPO_ROOT/.git/hooks"
	if ln -s "git-hooks/hooks" "$REPO_ROOT/.git/hooks" 2>/dev/null; then
		echo "Created symlink .git/hooks -> git-hooks/hooks"
	else
		echo "Warning: could not create symlink .git/hooks -> git-hooks/hooks."
		echo "Please run: git config core.hooksPath git-hooks/hooks"
	fi
fi
