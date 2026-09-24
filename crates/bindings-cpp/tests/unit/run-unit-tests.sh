#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BUILD_DIR="$SCRIPT_DIR/build"
VERBOSE=0

if command -v emcmake >/dev/null 2>&1; then
    EMCMAKE_CMD="emcmake"
elif command -v emcmake.bat >/dev/null 2>&1; then
    EMCMAKE_CMD="emcmake.bat"
else
    echo "Unable to locate emcmake or emcmake.bat" >&2
    exit 1
fi

if ! command -v node >/dev/null 2>&1; then
    echo "Unable to locate node" >&2
    exit 1
fi

while [[ $# -gt 0 ]]; do
    case "$1" in
        -v|--verbose)
            VERBOSE=1
            shift
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

echo
echo "==> Configuring unit tests"
"$EMCMAKE_CMD" cmake -S "$SCRIPT_DIR" -B "$BUILD_DIR"

echo
echo "==> Building unit tests"
cmake --build "$BUILD_DIR"

echo
echo "==> Running unit tests"
for LAUNCHER in "$BUILD_DIR"/*_unit_tests.cjs; do
if [[ ! -f "$LAUNCHER" ]]; then
    echo "Could not find built unit test launcher" >&2
    exit 1
fi

if [[ $VERBOSE -eq 1 ]]; then
    node "$LAUNCHER" -v
else
    node "$LAUNCHER"
fi
done
