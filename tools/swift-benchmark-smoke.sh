#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_PACKAGE_PATH="$ROOT_DIR/sdks/swift"

swift package --package-path "$SDK_PACKAGE_PATH" benchmark list

swift package \
  --package-path "$SDK_PACKAGE_PATH" \
  benchmark \
  --target SpacetimeDBBenchmarks \
  --filter "^(BSATN Encode Point3D|Message Encode Subscribe)$" \
  --no-progress \
  --quiet
