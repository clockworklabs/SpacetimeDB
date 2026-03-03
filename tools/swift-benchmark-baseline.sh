#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SDK_DIR="$ROOT_DIR/sdks/swift"
TARGET="SpacetimeDBBenchmarks"

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <baseline-name> [benchmark-filter-regex]" >&2
  echo "Example: $0 macos14-arm64-swift6.2 '^RoundTrip (Reducer|Procedure)'" >&2
  exit 64
fi

BASELINE_NAME="$1"
FILTER_REGEX="${2:-}"
TIMESTAMP_UTC="$(date -u +"%Y%m%dT%H%M%SZ")"

CAPTURE_DIR="$SDK_DIR/Benchmarks/Baselines/captures/$BASELINE_NAME"
RAW_RESULTS_FILE="$CAPTURE_DIR/${TIMESTAMP_UTC}.baseline-results.json"
SUMMARY_FILE="$CAPTURE_DIR/${TIMESTAMP_UTC}.summary.json"
METADATA_FILE="$CAPTURE_DIR/${TIMESTAMP_UTC}.metadata.txt"
LATEST_RAW_FILE="$CAPTURE_DIR/latest.baseline-results.json"
LATEST_SUMMARY_FILE="$CAPTURE_DIR/latest.summary.json"
LATEST_METADATA_FILE="$CAPTURE_DIR/latest.metadata.txt"

mkdir -p "$CAPTURE_DIR"

update_cmd=(
  swift package --allow-writing-to-package-directory
  benchmark baseline update "$BASELINE_NAME"
  --target "$TARGET"
  --benchmark-build-configuration release
  --no-progress
)

read_cmd=(
  swift package
  benchmark baseline read "$BASELINE_NAME"
  --target "$TARGET"
  --format jsonSmallerIsBetter
  --path stdout
  --no-progress
)

if [[ -n "$FILTER_REGEX" ]]; then
  update_cmd+=(--filter "$FILTER_REGEX")
  read_cmd+=(--filter "$FILTER_REGEX")
fi

(
  cd "$SDK_DIR"
  "${update_cmd[@]}"
)

SOURCE_RESULTS_FILE="$SDK_DIR/.benchmarkBaselines/$TARGET/$BASELINE_NAME/results.json"
cp "$SOURCE_RESULTS_FILE" "$RAW_RESULTS_FILE"
cp "$RAW_RESULTS_FILE" "$LATEST_RAW_FILE"

(
  cd "$SDK_DIR"
  "${read_cmd[@]}"
) > "$SUMMARY_FILE"
cp "$SUMMARY_FILE" "$LATEST_SUMMARY_FILE"

{
  echo "baseline_name=$BASELINE_NAME"
  echo "captured_at_utc=$TIMESTAMP_UTC"
  if [[ -n "$FILTER_REGEX" ]]; then
    echo "filter_regex=$FILTER_REGEX"
  fi
  echo "git_head=$(git -C "$ROOT_DIR" rev-parse HEAD)"
  echo "swift_version=$(swift --version 2>&1 | tr '\n' ' ' | sed 's/[[:space:]]\\+/ /g')"
  if command -v sw_vers >/dev/null 2>&1; then
    echo "os_product_version=$(sw_vers -productVersion)"
    echo "os_build_version=$(sw_vers -buildVersion)"
  fi
  echo "kernel=$(uname -sr)"
  echo "arch=$(uname -m)"
  printf "update_command="
  printf "%q " "${update_cmd[@]}"
  echo
  printf "read_command="
  printf "%q " "${read_cmd[@]}"
  echo
} > "$METADATA_FILE"
cp "$METADATA_FILE" "$LATEST_METADATA_FILE"

echo "Saved baseline capture:"
echo "  raw:     $RAW_RESULTS_FILE"
echo "  summary: $SUMMARY_FILE"
echo "  meta:    $METADATA_FILE"
echo
echo "Compare against another baseline:"
echo "  cd $SDK_DIR && swift package benchmark baseline compare $BASELINE_NAME <other-baseline> --target $TARGET --no-progress"
