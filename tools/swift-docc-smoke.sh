#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="$ROOT_DIR/sdks/swift"

docc_cmd=(
  xcodebuild docbuild
  -scheme SpacetimeDB-Package
  -destination 'generic/platform=macOS'
  -derivedDataPath .build/docc
  -quiet
)

cd "$SDK_DIR"

if "${docc_cmd[@]}"; then
  echo "DocC smoke build succeeded without -skipPackagePluginValidation."
else
  echo "DocC smoke build failed without plugin-validation skip; retrying with fallback."
  "${docc_cmd[@]}" -skipPackagePluginValidation
fi
