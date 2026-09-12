#!/bin/bash
set -euo pipefail

VERSION_REGEX="[0-9]+\.[0-9]+\.[0-9]+"
MAGIC_COMMENT="!rust-toolchain-sync"

REPO_ROOT="$(dirname "$0")/.."

TOOLCHAIN_FILE="${1:-$REPO_ROOT/rust-toolchain.toml}"
[[ $# -ge 1 ]] && shift

toolchain="$(rg -o "channel = \"($VERSION_REGEX)\"" -r '$1' "$TOOLCHAIN_FILE")" || {
    echo >&2 "$0: couldn't extract version from rust-toolchain.toml"
    exit 1
}

rg -.F "$MAGIC_COMMENT" -l "$@" | xargs gawk -i inplace -v toolchain_ver="$toolchain" "
/$MAGIC_COMMENT/ { version = 1; print; next }
{ if (version) { gsub(/$VERSION_REGEX/, toolchain_ver); version = 0; } print }
"
