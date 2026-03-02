#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

if ! command -v cargo >/dev/null 2>&1; then
  echo "error: cargo not found in PATH" >&2
  exit 1
fi

TMP_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/spacetimedb-swift-bindings-check.XXXXXX")"
trap 'rm -rf "${TMP_ROOT}"' EXIT

SIMPLE_TMP="${TMP_ROOT}/simple-generated"
NINJA_TMP="${TMP_ROOT}/ninja-generated"
mkdir -p "${SIMPLE_TMP}" "${NINJA_TMP}"

echo "==> Regenerating simple-module Swift bindings"
cargo run -q -p spacetimedb-cli --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
  generate \
  --lang swift \
  --out-dir "${SIMPLE_TMP}" \
  --module-path "${REPO_ROOT}/demo/simple-module/spacetimedb" \
  --no-config

echo "==> Regenerating ninja-game Swift bindings"
cargo run -q -p spacetimedb-cli --manifest-path "${REPO_ROOT}/Cargo.toml" -- \
  generate \
  --lang swift \
  --out-dir "${NINJA_TMP}" \
  --module-path "${REPO_ROOT}/demo/ninja-game/spacetimedb" \
  --no-config

SIMPLE_COMMITTED="${REPO_ROOT}/demo/simple-module/client-swift/Sources/SimpleModuleClient/Generated"
NINJA_COMMITTED="${REPO_ROOT}/demo/ninja-game/client-swift/Sources/NinjaGameClient/Generated"

echo "==> Checking simple-module generated bindings drift"
if ! diff -ru "${SIMPLE_TMP}" "${SIMPLE_COMMITTED}"; then
  echo "error: simple-module generated Swift bindings are out of date." >&2
  echo "run: cargo run -p spacetimedb-cli -- generate --lang swift --out-dir demo/simple-module/client-swift/Sources/SimpleModuleClient/Generated --module-path demo/simple-module/spacetimedb --no-config" >&2
  exit 1
fi

echo "==> Checking ninja-game generated bindings drift"
if ! diff -ru "${NINJA_TMP}" "${NINJA_COMMITTED}"; then
  echo "error: ninja-game generated Swift bindings are out of date." >&2
  echo "run: cargo run -p spacetimedb-cli -- generate --lang swift --out-dir demo/ninja-game/client-swift/Sources/NinjaGameClient/Generated --module-path demo/ninja-game/spacetimedb --no-config" >&2
  exit 1
fi

echo "==> Generated Swift bindings are in sync."
