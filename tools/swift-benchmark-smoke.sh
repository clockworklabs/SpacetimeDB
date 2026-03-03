#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_PACKAGE_PATH="$ROOT_DIR/sdks/swift"

swift package --package-path "$SDK_PACKAGE_PATH" benchmark list

swift package \
  --package-path "$SDK_PACKAGE_PATH" \
  benchmark \
  --target SpacetimeDBBenchmarks \
  --filter "^(BSATN Encode Point3D|Message Encode Subscribe|RoundTrip Reducer.*)$" \
  --no-progress \
  --quiet

swift package \
  --package-path "$SDK_PACKAGE_PATH" \
  benchmark \
  --target GeneratedBindingsBenchmarks \
  --filter "^(Generated Encode Row \\(Codable\\)|Generated Encode Row \\(BSATNSpecial\\)|Generated Cache Insert 1000 rows \\(BSATNSpecial\\))$" \
  --no-progress \
  --quiet
